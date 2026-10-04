//! Modern player screen: a hero form over the live map, with the `model`
//! cvar's character standing on the backdrop's stage. Character and saber
//! changes write their cvars the moment they are made, so the stage model
//! swaps instantly and there is nothing to apply or revert. The Force page
//! is the exception: it edits a draft that only its Apply action writes
//! (see `force`).

mod controller;
mod force;
mod force_icons;
mod force_view;
mod grid;
mod icons;
mod numeric;
mod pointer;
mod rows;
mod saber;
mod saber_view;
mod team_filter;
mod view;

use crate::console::ViewerConsole;
use crate::menu_widgets::MenuCanvas;
use jkr_client::{LegacyAssetCatalog, LegacyAssetCatalogLoader};
use jkr_ui::DrawList;
use jkr_vfs::VirtualFileSystem;
pub(crate) use saber::StageSabers;
use std::sync::Arc;
use team_filter::TeamSkin;

/// Tab labels, in page order.
const PAGE_TABS: [&str; 3] = ["CHARACTER", "SABER", "FORCE"];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProfilePage {
    #[default]
    Character,
    Saber,
    Force,
}

impl ProfilePage {
    const ALL: [Self; 3] = [Self::Character, Self::Saber, Self::Force];

    fn index(self) -> usize {
        Self::ALL.iter().position(|page| *page == self).unwrap_or(0)
    }
}

/// Screen to restore when the player selector closes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReturnTarget {
    MainMenu,
    InGame,
}

/// Result of one keyboard or pointer event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlayerMenuResult {
    None,
    Back(ReturnTarget),
}

/// Which catalogue entry the `model` cvar currently names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Choice {
    Character(usize),
    Species(usize),
}

/// The userinfo values this screen owns (sabers and Force live in their
/// own drafts).
#[derive(Clone, Debug, Eq, PartialEq)]
struct Draft {
    name: String,
    model: String,
    rgb: [u8; 3],
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            name: String::with_capacity(32),
            model: "kyle/default".to_owned(),
            rgb: [255; 3],
        }
    }
}

/// Fixed-storage controller and retained draw list for the player screen.
pub(crate) struct PlayerMenu {
    canvas: MenuCanvas,
    loader: Option<LegacyAssetCatalogLoader>,
    /// Where the model icons come from once the catalogue is known.
    icon_vfs: Option<Arc<VirtualFileSystem>>,
    icons: icons::IconLoader,
    /// The Force page's power icons and side emblems.
    force_icons: icons::IconLoader,
    /// Skin set the grid lists (retail's Team Color chooser).
    team: TeamSkin,
    /// Catalogue indices (characters first, then species) the grid shows,
    /// rebuilt when the team or the catalogue changes.
    tiles: Vec<usize>,
    /// First visible tile row of the model grid.
    grid_scroll: usize,
    /// Largest `grid_scroll` the last frame's layout allowed.
    grid_max_scroll: usize,
    /// Scroll the grid to the current model on the next frame (set when the
    /// choice changes without the pointer, so wheel browsing is not undone).
    grid_follow: bool,
    draft: Draft,
    choice: Option<Choice>,
    /// Selected head, torso, legs and skin colour of the current species.
    variants: [usize; 4],
    /// Keyboard/pointer selection: a row index on the current page.
    selected: usize,
    name_editing: bool,
    numeric: Option<crate::menu_widgets::numeric::NumericEdit>,
    name_before_edit: String,
    return_target: ReturnTarget,
    resolved_catalogue: bool,
    page: ProfilePage,
    saber: saber::SaberMenu,
    force: force::ForceMenu,
}

impl PlayerMenu {
    pub(crate) fn new() -> Self {
        Self {
            canvas: MenuCanvas::new(),
            loader: None,
            icon_vfs: None,
            icons: icons::IconLoader::new(),
            force_icons: icons::IconLoader::new(),
            team: TeamSkin::default(),
            tiles: Vec::with_capacity(icons::MAX_ICONS),
            grid_scroll: 0,
            grid_max_scroll: 0,
            grid_follow: true,
            draft: Draft::default(),
            choice: None,
            variants: [0; 4],
            selected: 0,
            name_editing: false,
            numeric: None,
            name_before_edit: String::with_capacity(32),
            return_target: ReturnTarget::MainMenu,
            resolved_catalogue: false,
            page: ProfilePage::Character,
            saber: saber::SaberMenu::new(),
            force: force::ForceMenu::new(),
        }
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        self.canvas.draw_list()
    }

    /// The `model` cvar value currently shown, for the live stage model.
    pub(crate) fn stage_model(&self) -> &str {
        &self.draft.model
    }

    /// The sabers the stage model holds: the saber draft as it stands,
    /// thrown out to the backdrop's saber shot while the screen is `open`
    /// on its Saber tab (the tab stays selected after the screen closes;
    /// the saber must not).
    pub(crate) fn stage_sabers(&self, open: bool) -> StageSabers<'_> {
        self.saber
            .stage_sabers(open && self.page == ProfilePage::Saber)
    }

    /// Where the screen returns when closed.
    pub(crate) fn return_target(&self) -> ReturnTarget {
        self.return_target
    }

    /// Backdrop shot behind the current tab.
    pub(crate) fn shot(&self) -> crate::menu_backdrop::Shot {
        match self.page {
            ProfilePage::Saber => crate::menu_backdrop::Shot::Saber,
            _ => crate::menu_backdrop::Shot::Player,
        }
    }

    /// Selected row, for the shared highlight shader.
    pub(crate) fn visual_selection(&self) -> usize {
        self.selected
    }

    /// Move a few decoded model and Force icons into the UI atlas.
    pub(crate) fn upload_icons(
        &mut self,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        self.icons.upload_batch(renderer, queue, 32);
        self.force_icons.upload_batch(renderer, queue, 32);
    }

    /// Start decoding the Force icons once the VFS is known, and the model
    /// icons once the catalogue is too.
    fn request_icons_if_ready(&mut self) {
        if let Some(vfs) = self
            .icon_vfs
            .as_ref()
            .filter(|_| self.force_icons.is_idle())
        {
            self.force_icons
                .request_paths(Arc::clone(vfs), force_icons::requests());
        }
        if !self.icons.is_idle() {
            return;
        }
        let catalog = catalog_of(&self.loader);
        if let (Some(vfs), Some(catalog)) = (&self.icon_vfs, catalog) {
            self.icons.request(Arc::clone(vfs), catalog);
        }
    }

    fn catalog(&self) -> Option<&LegacyAssetCatalog> {
        catalog_of(&self.loader)
    }
}

/// Borrow the loaded catalogue independently of the mutable menu canvas.
fn catalog_of(loader: &Option<LegacyAssetCatalogLoader>) -> Option<&LegacyAssetCatalog> {
    loader
        .as_ref()
        .and_then(LegacyAssetCatalogLoader::catalog)
        .map(Arc::as_ref)
}

impl crate::GpuState {
    pub(crate) fn open_player_menu_from_game(&mut self) {
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
            menu.open_player(console, ReturnTarget::InGame);
            self.game_menu = false;
        }
    }
}
