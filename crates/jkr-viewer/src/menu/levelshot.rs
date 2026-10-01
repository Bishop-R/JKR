//! Map previews for the Create game screen: `levelshots/<map>.jpg` (or
//! `.tga`/`.png`) decoded on a worker thread, scaled to the UI atlas'
//! single 4:3 preview slot and cached per map. The draw path only asks
//! [`Levelshots::preview`]; decoding never runs on it, and the atlas is
//! written only when the wanted map changes.

use crate::ui_renderer::LEVELSHOT_SIZE;
use jkr_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

/// Decoded previews kept before the cache starts over (about 0.8 MB each).
const CACHE_LIMIT: usize = 32;
/// Extensions tried after `levelshots/<map>`, in the stock renderer's order.
const EXTENSIONS: [&str; 3] = ["jpg", "tga", "png"];

/// What the preview area shows for a map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Preview {
    /// The map's levelshot is in the atlas slot.
    Image,
    /// Still decoding.
    Loading,
    /// The map ships no levelshot (or it did not decode).
    Missing,
}

type Decoded = Option<Arc<[u8]>>;

/// Per-map levelshot cache feeding the one atlas preview slot.
pub(crate) struct Levelshots {
    vfs: Option<Arc<VirtualFileSystem>>,
    requests: Option<Sender<String>>,
    results: Option<Receiver<(String, Decoded)>>,
    cache: HashMap<String, Decoded>,
    /// The map whose preview is wanted now.
    wanted: String,
    /// The map the atlas slot (or the placeholder) currently stands for.
    shown: String,
    shown_image: bool,
}

impl Levelshots {
    pub(crate) fn new() -> Self {
        Self {
            vfs: None,
            requests: None,
            results: None,
            cache: HashMap::with_capacity(CACHE_LIMIT),
            wanted: String::with_capacity(64),
            shown: String::with_capacity(64),
            shown_image: false,
        }
    }

    /// Read levelshots from `vfs` from now on; forgets everything decoded.
    pub(crate) fn attach_vfs(&mut self, vfs: Arc<VirtualFileSystem>) {
        self.vfs = Some(vfs);
        self.requests = None;
        self.results = None;
        self.cache.clear();
        self.shown.clear();
        self.shown_image = false;
        let wanted = std::mem::take(&mut self.wanted);
        self.want(&wanted);
    }

    /// Ask for `map`'s preview (`mp/ffa3`); cheap when it is already wanted.
    pub(crate) fn want(&mut self, map: &str) {
        if self.wanted == map {
            return;
        }
        self.wanted.clear();
        self.wanted.push_str(map);
        if map.is_empty() || self.cache.contains_key(map) {
            return;
        }
        let Some(vfs) = &self.vfs else { return };
        if self.requests.is_none() {
            let (requests, results) = spawn_worker(Arc::clone(vfs));
            self.requests = Some(requests);
            self.results = Some(results);
        }
        if let Some(requests) = &self.requests {
            let _ = requests.send(map.to_owned());
        }
    }

    /// Collect finished decodes and, when the wanted map's preview is ready
    /// and not yet in the slot, hand its RGBA pixels to `upload`.
    pub(crate) fn service(&mut self, mut upload: impl FnMut(&[u8])) {
        if let Some(results) = &self.results {
            while let Ok((map, decoded)) = results.try_recv() {
                if self.cache.len() >= CACHE_LIMIT {
                    // Start over, but never drop the one still wanted.
                    let wanted = &self.wanted;
                    self.cache.retain(|name, _| name == wanted);
                }
                self.cache.insert(map, decoded);
            }
        }
        if self.shown == self.wanted {
            return;
        }
        let Some(decoded) = self.cache.get(&self.wanted) else {
            return;
        };
        self.shown_image = match decoded {
            Some(pixels) => {
                upload(pixels);
                true
            }
            None => false,
        };
        self.shown.clear();
        self.shown.push_str(&self.wanted);
    }

    /// What to draw for `map` this frame.
    pub(crate) fn preview(&self, map: &str) -> Preview {
        if !self.shown.is_empty() && self.shown == map {
            return if self.shown_image {
                Preview::Image
            } else {
                Preview::Missing
            };
        }
        if self.vfs.is_none() || matches!(self.cache.get(map), Some(None)) {
            return Preview::Missing;
        }
        Preview::Loading
    }
}

/// Start the decoder thread; it always works on the newest request.
fn spawn_worker(vfs: Arc<VirtualFileSystem>) -> (Sender<String>, Receiver<(String, Decoded)>) {
    let (request_tx, request_rx) = mpsc::channel::<String>();
    let (result_tx, result_rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("jkr-levelshots".to_owned())
        .spawn(move || {
            while let Ok(mut map) = request_rx.recv() {
                // Scrolling queues many maps; only the latest is still wanted.
                while let Ok(newer) = request_rx.try_recv() {
                    map = newer;
                }
                let decoded = decode_levelshot(&vfs, &map).map(Arc::from);
                if result_tx.send((map, decoded)).is_err() {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        crate::log::progress(format_args!("levelshot decoder not started: {error}"));
    }
    (request_tx, result_rx)
}

/// `map`'s levelshot as [`LEVELSHOT_SIZE`] RGBA, if it ships one.
pub(crate) fn decode_levelshot(vfs: &VirtualFileSystem, map: &str) -> Option<Vec<u8>> {
    EXTENSIONS.iter().find_map(|extension| {
        let path = format!("levelshots/{map}.{extension}");
        let asset = vfs.read(&path).ok().flatten()?;
        let image = crate::gpu_texture::decode_image(&asset.bytes, &path).ok()?;
        let [width, height] = LEVELSHOT_SIZE;
        let scaled = image::imageops::resize(
            &image.into_rgba8(),
            width,
            height,
            image::imageops::FilterType::Triangle,
        );
        Some(scaled.into_raw())
    })
}
