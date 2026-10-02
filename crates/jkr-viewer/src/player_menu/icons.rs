//! Model icons for the character grid: decoded on a background thread once
//! the catalogue is known, then uploaded into the UI icon atlas a few per
//! frame. Tile `i` of the grid (characters first, then species) samples
//! [`TextureId`] `i + 1`; cell 0 stays free for the HUD.

use crate::ui_renderer::{ICON_SIZE, ShapeRenderer};
use jkr_client::LegacyAssetCatalog;
use jkr_ui::TextureId;
use jkr_vfs::VirtualFileSystem;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

/// Most icons the loader tracks; the atlas has as many cells.
/// Atlas cells available to model icons (the banner strip takes the rest).
pub(super) const MAX_ICONS: usize = crate::ui_renderer::ICON_CELLS as usize;

struct DecodedIcon {
    id: TextureId,
    rgba: Vec<u8>,
}

pub(super) struct IconLoader {
    receiver: Option<Receiver<Vec<DecodedIcon>>>,
    decoded: Vec<DecodedIcon>,
    uploaded: usize,
    requested: bool,
    /// One bit per atlas cell that holds a finished upload.
    ready: [u64; MAX_ICONS.div_ceil(64)],
}

impl IconLoader {
    pub(super) fn new() -> Self {
        Self {
            receiver: None,
            decoded: Vec::new(),
            uploaded: 0,
            requested: false,
            ready: [0; MAX_ICONS.div_ceil(64)],
        }
    }

    pub(super) fn is_idle(&self) -> bool {
        !self.requested
    }

    /// Atlas cell of grid tile `index`.
    pub(super) fn texture_of(index: usize) -> TextureId {
        TextureId(index as u32 + 1)
    }

    /// Whether the icon of grid tile `index` is in the atlas.
    pub(super) fn is_ready(&self, index: usize) -> bool {
        let cell = Self::texture_of(index).0 as usize;
        cell < MAX_ICONS && self.ready[cell / 64] & (1 << (cell % 64)) != 0
    }

    /// Decode every character and species icon of `catalog` off-thread.
    pub(super) fn request(&mut self, vfs: Arc<VirtualFileSystem>, catalog: &LegacyAssetCatalog) {
        if self.requested {
            return;
        }
        self.requested = true;
        let mut requests = Vec::with_capacity(catalog.characters.len() + catalog.species.len());
        for (index, character) in catalog.characters.iter().enumerate() {
            requests.push((Self::texture_of(index), vec![character.icon.clone()]));
        }
        for (index, species) in catalog.species.iter().enumerate() {
            let id = Self::texture_of(catalog.characters.len() + index);
            let stem = species.heads.first().map_or("default", String::as_str);
            let base = format!("models/players/{}/icon_{stem}", species.model);
            requests.push((
                id,
                ["jpg", "png", "tga"]
                    .map(|extension| format!("{base}.{extension}"))
                    .to_vec(),
            ));
        }
        requests.truncate(MAX_ICONS - 1);
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let decoded = requests
                .into_iter()
                .filter_map(|(id, paths)| decode_first(&vfs, id, &paths))
                .collect();
            let _ = sender.send(decoded);
        });
        self.receiver = Some(receiver);
    }

    pub(super) fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        match receiver.try_recv() {
            Ok(decoded) => {
                self.decoded = decoded;
                self.receiver = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => self.receiver = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Upload up to `limit` decoded icons into the atlas.
    pub(super) fn upload_batch(
        &mut self,
        renderer: &ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
        limit: usize,
    ) {
        let end = (self.uploaded + limit).min(self.decoded.len());
        for icon in &self.decoded[self.uploaded..end] {
            renderer.upload_icon(queue, icon.id, &icon.rgba);
            let cell = icon.id.0 as usize;
            if cell < MAX_ICONS {
                self.ready[cell / 64] |= 1 << (cell % 64);
            }
        }
        self.uploaded = end;
        if self.uploaded == self.decoded.len() {
            self.decoded.clear();
            self.uploaded = 0;
        }
    }
}

fn decode_first(vfs: &VirtualFileSystem, id: TextureId, paths: &[String]) -> Option<DecodedIcon> {
    for path in paths {
        let Ok(Some(image)) = crate::decoded_image_cache::cached_decoded_image(vfs, path) else {
            continue;
        };
        let rgba = image::imageops::resize(
            image.as_ref(),
            ICON_SIZE,
            ICON_SIZE,
            image::imageops::FilterType::Triangle,
        )
        .into_raw();
        return Some(DecodedIcon { id, rgba });
    }
    None
}
