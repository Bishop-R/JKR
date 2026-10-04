//! Native main-menu and server-browser input/presentation on the shared UI path.

pub(crate) mod address_view;
mod controller;
mod pointer;

mod browser_details;
pub(crate) mod browser_filters;
pub(crate) mod browser_table;
pub(crate) mod browser_view;
pub(crate) mod create_game;
pub(crate) mod create_game_catalog;
mod create_game_pointer;
mod create_game_view;
mod hosting;
mod levelshot;
pub(crate) mod main_view;
mod map_picker;
mod map_picker_view;
pub(crate) mod network_view;

use super::{TextVertex, UiFont};
use crate::client_state::{ClientPhase, ClientState};
use crate::console::ViewerConsole;
use crate::keybind_editor::{EditorResult, KeybindEditor};
use crate::menu_backdrop::{Sample, Shot, Stage};
use crate::menu_widgets::MenuCanvas;
use crate::player_menu::{PlayerMenu, PlayerMenuResult, ReturnTarget};
use crate::server_browser::{RefreshPoll, ServerBrowser, SortColumn};
use crate::settings::{SettingsMenu, SettingsResult};
use jkr_ui::{AbstractAction, DrawList, InputEvent};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// One top-level entry: the label and the one-line hint shown while selected.
pub(crate) struct MainItem {
    pub(crate) label: &'static str,
    pub(crate) hint: &'static str,
}

const MAIN_ITEMS: [MainItem; 5] = [
    MainItem {
        label: "Play",
        hint: "Browse and join servers",
    },
    MainItem {
        label: "Create game",
        hint: "Host a match with bots on this machine",
    },
    MainItem {
        label: "Player",
        hint: "Name, model, saber and Force",
    },
    MainItem {
        label: "Settings",
        hint: "Video, audio, controls and keys",
    },
    MainItem {
        label: "Quit",
        hint: "Exit to desktop",
    },
];

/// Hand the loaded map and mounted assets to the menu layer.
pub(crate) fn attach_world(
    menu: &mut Option<ClientMenu>,
    vfs: std::sync::Arc<jkr_vfs::VirtualFileSystem>,
    bsp: &jkr_bsp::Bsp,
    console: Option<&ViewerConsole>,
) {
    if let Some(menu) = menu {
        menu.attach_catalogue(vfs);
        menu.backdrop = crate::menu_backdrop::Backdrop::from_bsp(bsp);
        if let Some(console) = console {
            menu.player.prime(console);
        }
    }
}

/// Feed decoded menu images to the UI atlas while their screen is up: the
/// player screen's model icons, Create game's map preview.
pub(crate) fn upload_menu_images(
    menu: &mut Option<ClientMenu>,
    renderer: &crate::ui_renderer::ShapeRenderer,
    queue: &crate::frame_queue::FrameQueue,
) {
    let Some(menu) = menu else { return };
    match menu.state.phase() {
        ClientPhase::Player => menu.player.upload_icons(renderer, queue),
        ClientPhase::CreateGame => {
            menu.create_game
                .service_levelshots(|rgba| renderer.upload_levelshot(queue, rgba));
        }
        _ => {}
    }
}

/// Side effects requested by menu input and executed by the application.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum MenuAction {
    None,
    Connect(String),
    CancelJoin,
    /// Start a local server with these settings and join it.
    HostGame(crate::connection::local_server::HostSettings),
    Quit,
    ReturnToGameMenu,
}

/// Persistent menu selection, browser data, and high-level client phase.
pub(crate) struct ClientMenu {
    state: ClientState,
    browser: ServerBrowser,
    browser_focus: u16,
    /// Last clicked browser row and when, for double-click joining.
    browser_last_click: Option<(u16, std::time::Instant)>,
    main_selection: usize,
    settings: SettingsMenu,
    /// Where the settings screen (and the key-bindings editor it hosts)
    /// returns when closed: the main menu, or the game menu that opened it.
    settings_return: ReturnTarget,
    /// Where the server browser returns when closed without joining.
    browser_return: ReturnTarget,
    keybinds: KeybindEditor,
    player: PlayerMenu,
    /// The Create game screen and the server it runs.
    create_game: create_game::CreateGameMenu,
    ui: MenuCanvas,
    filter_editing: bool,
    password_target: Option<String>,
    password: String,
    address_editing: bool,
    address_input: String,
    address_error: String,
    backdrop: Option<crate::menu_backdrop::Backdrop>,
    /// Map of the server being joined, as the browser row advertised it.
    destination_map: Option<String>,
    /// The preview of that map can be shown: the gate may open.
    destination_ready: bool,
    /// Last gate request logged, so the log only records changes.
    gate_logged: bool,
}

/// Backdrop shot each client phase is presented over. A connect stays on
/// the browser shot: the gate there is what opens onto the server.
fn shot_for(phase: &ClientPhase, player: &PlayerMenu) -> Shot {
    match phase {
        ClientPhase::Browser
        | ClientPhase::Connecting(_)
        | ClientPhase::ConnectionError
        | ClientPhase::CreateGame => Shot::Browser,
        ClientPhase::Settings | ClientPhase::Keybinds => Shot::Settings,
        ClientPhase::Player => player.shot(),
        _ => Shot::Main,
    }
}

/// Whether `phase` is a connect in progress, during which the backdrop's
/// gate opens.
fn connecting(phase: &ClientPhase) -> bool {
    matches!(phase, ClientPhase::Connecting(_))
}

impl ClientMenu {
    pub(crate) fn new(open_main_menu: bool, master: String) -> Self {
        Self {
            state: ClientState::new(open_main_menu),
            browser: ServerBrowser::new(master),
            browser_focus: 1_000,
            browser_last_click: None,
            main_selection: 0,
            settings: SettingsMenu::new(),
            settings_return: ReturnTarget::MainMenu,
            browser_return: ReturnTarget::MainMenu,
            keybinds: KeybindEditor::new(),
            player: PlayerMenu::new(),
            create_game: create_game::CreateGameMenu::new(),
            ui: MenuCanvas::new(),
            filter_editing: false,
            password_target: None,
            destination_map: None,
            destination_ready: false,
            gate_logged: false,
            password: String::with_capacity(64),
            address_editing: false,
            address_input: String::with_capacity(256),
            address_error: String::with_capacity(96),
            backdrop: None,
        }
    }

    /// Advance the live-map backdrop toward the current screen's shot and
    /// return the camera for this frame, once a map is loaded.
    pub(crate) fn drive_backdrop(&mut self, millis: u64) -> Option<Sample> {
        let shot = shot_for(self.state.phase(), &self.player);
        let gate = connecting(self.state.phase()) && self.destination_ready;
        if gate != self.gate_logged {
            self.gate_logged = gate;
            crate::log::progress(format_args!(
                "gate wanted={gate} at {millis} ms (phase {:?}, destination ready={})",
                self.state.phase(),
                self.destination_ready
            ));
        }
        self.backdrop.as_mut().map(|backdrop| {
            backdrop.set_gate(gate);
            backdrop.drive(shot, millis)
        })
    }

    /// Whether the joined server's world is built and waiting behind the
    /// gate; the glide through the gate holds until it is.
    pub(crate) fn set_world_ready(&mut self, ready: bool) {
        if let Some(backdrop) = &mut self.backdrop {
            backdrop.set_world_ready(ready);
        }
    }

    /// How far the backdrop's gate prop is open: only during a connect, so
    /// the gate of a map joined for play stands where the server has it.
    pub(crate) fn gate_open(&self, millis: u64) -> f32 {
        match &self.backdrop {
            Some(backdrop) if connecting(self.state.phase()) => backdrop.gate_open(millis),
            _ => 0.0,
        }
    }

    /// Whether a finished join may cut to the server world: the camera has
    /// flown through the gate, or there is no gate flight on this map.
    pub(crate) fn gate_crossed(&self, millis: u64) -> bool {
        match &self.backdrop {
            Some(backdrop) if connecting(self.state.phase()) => backdrop.gate_crossed(millis),
            _ => true,
        }
    }

    /// Progress of the glide through the gate during a connect (diagnostics).
    pub(crate) fn passage_progress(&self, millis: u64) -> f32 {
        match &self.backdrop {
            Some(backdrop) if connecting(self.state.phase()) => backdrop.passage_progress(millis),
            _ => 0.0,
        }
    }

    /// A cue the backdrop's gate passed while opening, with where its dust
    /// falls from.
    pub(crate) fn take_gate_cue(&mut self) -> Option<(crate::world_props::GateCue, [[f32; 3]; 2])> {
        self.backdrop.as_mut()?.take_gate_cue()
    }

    /// The menu world is back after a game: its gate is shut and the glide
    /// through it over, whatever state the join left them in.
    pub(crate) fn reset_gate(&mut self) {
        self.destination_ready = false;
        if let Some(backdrop) = &mut self.backdrop {
            backdrop.reset_gate();
        }
    }

    /// Whether the joined server's map is still loading behind this menu.
    /// Whether a join is in progress (connecting or loading the map).
    pub(crate) fn is_connecting(&self) -> bool {
        connecting(self.state.phase())
    }

    /// The map the browser row of the server being joined advertised
    /// (`mp/duel1`), if the join came from a row.
    pub(crate) fn destination_map(&self) -> Option<&str> {
        self.destination_map
            .as_deref()
            .filter(|map| !map.is_empty())
    }

    /// Whether the world beyond the gate is ready to be shown.
    pub(crate) fn set_destination_ready(&mut self, ready: bool) {
        self.destination_ready = ready;
    }

    /// The player model that stands on the backdrop's stage: only while a
    /// route with a stage is active (flying to, parked on, or flying back
    /// from the Player screen).
    pub(crate) fn stage_model(&self) -> Option<(Stage, &str)> {
        let stage = self.backdrop.as_ref()?.stage()?;
        Some((stage, self.player.stage_model()))
    }

    /// The sabers in the stage model's hands.
    pub(crate) fn stage_sabers(&self) -> crate::player_menu::StageSabers<'_> {
        let open = matches!(self.state.phase(), ClientPhase::Player);
        self.player.stage_sabers(open)
    }

    /// Where the thrown saber floats on this map's saber shot.
    pub(crate) fn saber_focus(&self) -> Option<crate::menu_backdrop::Focus> {
        self.backdrop.as_ref()?.focus(Shot::Saber)
    }

    /// Opacity of the current screen: 1 while the backdrop camera is parked
    /// on its shot, fading in as the flight arrives, 0 while it is away.
    fn screen_reveal(&self) -> f32 {
        if self.opened_from_game() {
            // Over a live match the backdrop camera is not flying anywhere.
            return 1.0;
        }
        let shot = shot_for(self.state.phase(), &self.player);
        self.backdrop
            .as_ref()
            .map_or(1.0, |backdrop| backdrop.reveal(shot))
    }

    /// Apply the player's accent colour to every screen this menu draws.
    pub(crate) fn set_accent(&mut self, accent: jkr_ui::Color) {
        self.ui.set_accent(accent);
    }

    /// Whether the settings screen wants the window's monitor facts.
    pub(crate) fn wants_monitor_modes(&self) -> bool {
        self.settings.wants_monitor_modes()
    }

    /// Hand the settings screen the window's monitor facts.
    pub(crate) fn set_monitor_modes(
        &mut self,
        modes: crate::settings::MonitorModes,
        console: &ViewerConsole,
    ) {
        self.settings.set_monitor_modes(modes, console);
    }

    /// Start the master-server fetch now, ahead of the browser being
    /// opened, so its rows are waiting when "Play" is pressed.
    pub(crate) fn prefetch_servers(&mut self) {
        self.browser.refresh();
    }

    /// Evidence hook: open the server browser the way "Play" would, minus
    /// the master-server refresh (evidence runs stay off the network).
    pub(crate) fn open_browser(&mut self) {
        self.browser_return = ReturnTarget::MainMenu;
        self.state.open_browser();
    }

    /// Open the server browser over a live match, returning to the game
    /// menu when closed; a join from it leaves the current server. The
    /// caller fetches rows with [`Self::refresh_servers_if_stale`].
    pub(crate) fn open_browser_from_game(&mut self, console: &ViewerConsole) {
        if let Ok(master) = console.master_server() {
            self.browser.set_master(master);
        }
        self.browser_return = ReturnTarget::InGame;
        self.state.open_browser();
        self.filter_editing = false;
        self.browser_focus = 1_000 + self.browser.selected() as u16;
    }

    /// Start a master-server fetch unless fresh rows are already on show.
    pub(crate) fn refresh_servers_if_stale(&mut self) {
        if self.browser.is_stale() {
            self.refresh();
        }
    }

    /// Close the browser toward wherever it was opened from.
    pub(super) fn close_browser(&mut self) -> MenuAction {
        match self.browser_return {
            ReturnTarget::MainMenu => {
                self.state.main_menu();
                MenuAction::None
            }
            ReturnTarget::InGame => {
                self.state.entered_game();
                MenuAction::ReturnToGameMenu
            }
        }
    }

    /// Open the settings screen on `tab`, returning to `target` when closed.
    pub(crate) fn open_settings_from(
        &mut self,
        console: &ViewerConsole,
        target: ReturnTarget,
        tab: usize,
    ) {
        self.settings.open_tab(console, tab);
        self.settings_return = target;
        self.state.open_settings();
    }

    /// Close the settings screen toward wherever it was opened from.
    pub(super) fn close_settings(&mut self) -> MenuAction {
        match self.settings_return {
            ReturnTarget::MainMenu => {
                self.state.main_menu();
                MenuAction::None
            }
            ReturnTarget::InGame => {
                self.state.entered_game();
                MenuAction::ReturnToGameMenu
            }
        }
    }

    /// Whether the screen on show was opened from the game menu, over a
    /// live match rather than the menu map.
    fn opened_from_game(&self) -> bool {
        match self.state.phase() {
            ClientPhase::Settings | ClientPhase::Keybinds => {
                self.settings_return == ReturnTarget::InGame
            }
            ClientPhase::Player => self.player.return_target() == ReturnTarget::InGame,
            ClientPhase::Browser => self.browser_return == ReturnTarget::InGame,
            _ => false,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.state.is_overlay_visible()
    }

    pub(crate) fn poll(&mut self) {
        if matches!(self.state.phase(), ClientPhase::Player) || !self.player.is_resolved() {
            self.player.poll();
        }
        self.poll_local_server();
        let browser_visible = matches!(self.state.phase(), ClientPhase::Browser);
        if browser_visible {
            self.browser.tick_details();
        }
        match self.browser.poll() {
            RefreshPoll::Idle | RefreshPoll::Pending => {}
            RefreshPoll::Complete if browser_visible && self.browser.entries().is_empty() => {
                self.state
                    .set_status("No responding protocol-26 servers found.");
            }
            RefreshPoll::Complete if browser_visible => self.state.set_status(format!(
                "{} responding servers. Enter joins; R refreshes.",
                self.browser.entries().len()
            )),
            RefreshPoll::Failed(error) if browser_visible => {
                self.state.set_status(format!("Refresh failed: {error}"));
            }
            RefreshPoll::Complete | RefreshPoll::Failed(_) => {}
        }
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        if !self.is_visible() || event.state != ElementState::Pressed || event.repeat {
            return MenuAction::None;
        }
        if self.address_editing {
            let PhysicalKey::Code(key) = event.physical_key else {
                return MenuAction::None;
            };
            match key {
                KeyCode::Escape => {
                    self.address_editing = false;
                    self.address_error.clear();
                }
                KeyCode::Backspace => {
                    self.address_input.pop();
                    self.address_error.clear();
                }
                KeyCode::Enter | KeyCode::NumpadEnter => return self.submit_address(),
                _ => {
                    if let Some(text) = event
                        .text
                        .as_deref()
                        .filter(|text| text.chars().all(|character| !character.is_control()))
                    {
                        let remaining = 255_usize.saturating_sub(self.address_input.len());
                        self.address_input.extend(text.chars().take(remaining));
                        self.address_error.clear();
                    }
                }
            }
            return MenuAction::None;
        }
        if let Some(address) = self.password_target.clone() {
            let PhysicalKey::Code(key) = event.physical_key else {
                return MenuAction::None;
            };
            match key {
                KeyCode::Escape => {
                    self.password_target = None;
                    self.password.clear();
                }
                KeyCode::Backspace => {
                    self.password.pop();
                }
                KeyCode::Enter | KeyCode::NumpadEnter if !self.password.is_empty() => {
                    console.set_cvar("password", &self.password);
                    self.password_target = None;
                    self.state.connecting(address.clone());
                    return MenuAction::Connect(address);
                }
                _ => {
                    if let Some(text) = event
                        .text
                        .as_deref()
                        .filter(|text| text.chars().all(|character| !character.is_control()))
                    {
                        self.password.push_str(text);
                    }
                }
            }
            return MenuAction::None;
        }
        if matches!(self.state.phase(), ClientPhase::Browser) && self.filter_editing {
            if let Some(text) = event
                .text
                .as_deref()
                .filter(|text| text.chars().all(|character| !character.is_control()))
            {
                self.browser.push_filter(text);
                return MenuAction::None;
            }
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return MenuAction::None;
        };
        match self.state.phase() {
            ClientPhase::MainMenu => match key {
                KeyCode::ArrowUp | KeyCode::KeyW => {
                    self.navigate_main(AbstractAction::Previous);
                    MenuAction::None
                }
                KeyCode::ArrowDown | KeyCode::KeyS | KeyCode::Tab => {
                    self.navigate_main(AbstractAction::Next);
                    MenuAction::None
                }
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                    self.activate_main(console)
                }
                _ => MenuAction::None,
            },
            ClientPhase::Browser => match key {
                KeyCode::ArrowUp | KeyCode::KeyW => {
                    self.browser.move_selection(-1);
                    self.browser_focus = 1_000 + self.browser.selected() as u16;
                    MenuAction::None
                }
                KeyCode::ArrowDown | KeyCode::KeyS => {
                    self.browser.move_selection(1);
                    self.browser_focus = 1_000 + self.browser.selected() as u16;
                    MenuAction::None
                }
                KeyCode::PageUp | KeyCode::PageDown => {
                    self.browser
                        .page_selection(if key == KeyCode::PageUp { -1 } else { 1 });
                    self.browser_focus = 1_000 + self.browser.selected() as u16;
                    MenuAction::None
                }
                KeyCode::Home | KeyCode::End => {
                    self.browser.select_end(key == KeyCode::End);
                    self.browser_focus = 1_000 + self.browser.selected() as u16;
                    MenuAction::None
                }
                KeyCode::Tab => {
                    self.browser
                        .set_favorites_only(!self.browser.favorites_only());
                    self.browser_focus = 1_000 + self.browser.selected() as u16;
                    MenuAction::None
                }
                KeyCode::KeyC => {
                    self.open_address_entry();
                    MenuAction::None
                }
                KeyCode::KeyR => {
                    self.refresh();
                    MenuAction::None
                }
                KeyCode::KeyF => {
                    self.browser.toggle_selected_favorite();
                    MenuAction::None
                }
                KeyCode::Slash => {
                    self.filter_editing = true;
                    MenuAction::None
                }
                KeyCode::Backspace if self.filter_editing => {
                    self.browser.pop_filter();
                    MenuAction::None
                }
                KeyCode::Digit1 => {
                    self.browser.sort_by(SortColumn::Name);
                    MenuAction::None
                }
                KeyCode::Digit2 => {
                    self.browser.sort_by(SortColumn::Map);
                    MenuAction::None
                }
                KeyCode::Digit3 => {
                    self.browser.sort_by(SortColumn::Players);
                    MenuAction::None
                }
                KeyCode::Digit4 => {
                    self.browser.sort_by(SortColumn::Ping);
                    MenuAction::None
                }
                KeyCode::Digit5 => {
                    self.browser.sort_by(SortColumn::Gametype);
                    MenuAction::None
                }
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                    self.activate_browser_focus()
                }
                KeyCode::Escape => {
                    if self.filter_editing {
                        self.filter_editing = false;
                        MenuAction::None
                    } else {
                        self.close_browser()
                    }
                }
                _ => MenuAction::None,
            },
            ClientPhase::Settings => match self.settings.handle_key(event, console) {
                SettingsResult::Back => self.close_settings(),
                SettingsResult::OpenKeybinds => {
                    self.keybinds.open(console);
                    self.state.open_keybinds();
                    MenuAction::None
                }
                SettingsResult::None => MenuAction::None,
            },
            ClientPhase::Keybinds => {
                if matches!(self.keybinds.handle_key(event, console), EditorResult::Back) {
                    self.settings.open(console);
                    self.state.open_settings();
                }
                MenuAction::None
            }
            ClientPhase::Player => match self.player.handle_key(event, console) {
                PlayerMenuResult::None => MenuAction::None,
                PlayerMenuResult::Back(ReturnTarget::MainMenu) => {
                    self.state.main_menu();
                    MenuAction::None
                }
                PlayerMenuResult::Back(ReturnTarget::InGame) => {
                    self.state.entered_game();
                    MenuAction::ReturnToGameMenu
                }
            },
            ClientPhase::CreateGame => {
                let result = self.create_game.key(key, event.text.as_deref(), console);
                self.create_game_result(result)
            }
            ClientPhase::Connecting(_) if key == KeyCode::Escape => self.cancel_join(),
            ClientPhase::ConnectionError
                if matches!(
                    key,
                    KeyCode::Escape | KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space
                ) =>
            {
                self.state.open_browser();
                MenuAction::None
            }
            ClientPhase::Connecting(_) | ClientPhase::ConnectionError | ClientPhase::InGame => {
                MenuAction::None
            }
        }
    }

    pub(crate) fn draw_list(&self) -> Option<&DrawList> {
        match self.state.phase() {
            ClientPhase::MainMenu
            | ClientPhase::Browser
            | ClientPhase::Connecting(_)
            | ClientPhase::ConnectionError => Some(self.ui.draw_list()),
            ClientPhase::Settings => Some(self.settings.draw_list()),
            ClientPhase::Keybinds => Some(self.keybinds.draw_list()),
            ClientPhase::Player => Some(self.player.draw_list()),
            ClientPhase::CreateGame => Some(self.create_game.draw_list()),
            ClientPhase::InGame => None,
        }
    }

    /// Selection metadata consumed by the shared backdrop/highlight shader.
    pub(crate) fn visual_selection(&self) -> (f32, f32) {
        match self.state.phase() {
            ClientPhase::MainMenu => (1.0, self.main_selection as f32),
            ClientPhase::Browser => (2.0, self.browser.selected() as f32),
            ClientPhase::Settings => {
                let (selected, root) = self.settings.visual_selection();
                (if root { 3.0 } else { 4.0 }, selected as f32)
            }
            ClientPhase::Keybinds => (5.0, self.keybinds.visual_selection() as f32),
            ClientPhase::Player => (7.0, self.player.visual_selection() as f32),
            ClientPhase::CreateGame => (8.0, self.create_game.visual_selection() as f32),
            ClientPhase::Connecting(_) => (6.0, 0.0),
            ClientPhase::ConnectionError => (6.0, 0.0),
            ClientPhase::InGame => (0.0, 0.0),
        }
    }

    pub(crate) fn handle_mouse_binding(
        &mut self,
        button: winit::event::MouseButton,
        console: &mut ViewerConsole,
    ) -> bool {
        matches!(self.state.phase(), ClientPhase::Keybinds)
            && self.keybinds.capture_mouse(button, console)
    }

    pub(crate) fn joined(&mut self) {
        self.state.entered_game();
    }

    pub(crate) fn state_connecting(&mut self, address: String) {
        // A join leaves the current server; a cancelled or failed one lands
        // in the browser, which then returns to the main menu.
        self.browser_return = ReturnTarget::MainMenu;
        self.state.connecting(address);
    }

    /// Show map preparation using the existing cancellable connection notice.
    pub(crate) fn state_loading(&mut self, map: &str) {
        self.state_connecting(map.to_owned());
        self.state.set_status(format!("Loading {map}..."));
    }

    pub(crate) fn join_failed(&mut self, error: impl Into<String>) {
        self.state.connection_failed(error);
    }

    pub(crate) fn return_to_main_menu(&mut self) {
        self.state.main_menu();
        self.main_selection = 0;
    }

    pub(crate) fn attach_catalogue(&mut self, vfs: std::sync::Arc<jkr_vfs::VirtualFileSystem>) {
        self.create_game.attach_vfs(std::sync::Arc::clone(&vfs));
        self.player.attach_catalogue(vfs);
    }

    pub(crate) fn open_player(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        self.player.open(console, target);
        self.state.open_player();
    }
}
