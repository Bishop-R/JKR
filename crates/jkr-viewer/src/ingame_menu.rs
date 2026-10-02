//! Retained in-game menu: the pages reached with Escape during a match,
//! drawn in the main menu's hero style over the live world, or, with
//! `ui_menuStyle classic`, as the retail bar and pop-ups ([`classic`]).

use crate::menu::art::ArtSet;
use crate::menu::style::MenuStyle;
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use jkr_protocol::{GameState, InfoString};
use jkr_ui::{AbstractAction, DrawList, InputEvent, UiEventKind};
use std::fmt::Write as _;

mod about;
mod callvote;
pub(crate) mod classic;
mod classic_actions;
mod classic_view;
pub(crate) mod shot;
mod siege;
pub(crate) mod siege_data;
mod view;
pub(crate) use callvote::Action as CallVoteAction;

const VOTE_SCROLL_TOKEN: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Page {
    Main,
    /// Camera and sun controls over the live world.
    Shot,
    Team,
    /// Map/theme constrained Siege class selection.
    Siege,
    /// Host, map, game type and limits (`ingame_about` in the stock UI).
    About,
    /// Disconnect or quit, with the choice as the confirmation (classic:
    /// the retail exit pop-up, confirmed on the next two pages).
    Leave,
    /// Classic only: the retail vote pop-up (Yes, No).
    Vote,
    /// Classic only: "Go to Main Menu?" after the exit pop-up's Main Menu.
    ConfirmLeave,
    /// Classic only: "Quit Program?" after the exit pop-up's Quit Program.
    ConfirmQuit,
    CallVote,
    VoteMap,
    VoteGameType,
    VoteKick,
    VoteClientKick,
    VoteWarmup,
    VoteTimeLimit,
    VoteFragLimit,
}

pub(crate) struct View<'a> {
    pub(crate) page: Page,
    pub(crate) selected_row: usize,
    pub(crate) team: u8,
    pub(crate) team_game: bool,
    /// The server runs Siege, where retail swaps two bar buttons.
    pub(crate) siege: bool,
    pub(crate) red_players: usize,
    pub(crate) blue_players: usize,
    pub(crate) vote_active: bool,
    /// Keeps the struct open for per-page data borrowed from the frame.
    pub(crate) _frame: std::marker::PhantomData<&'a ()>,
}

/// The per-entry hint of `view`'s page: only the main page carries hints.
fn hint_for(view: &View<'_>) -> impl Fn(usize) -> &'static str {
    let (page, vote_active) = (view.page, view.vote_active);
    move |row| match page {
        Page::Main => main_hint(row, vote_active),
        _ => "",
    }
}

/// One-line description under the selected entry of the main page.
fn main_hint(row: usize, vote_active: bool) -> &'static str {
    match row {
        0 => "Back to the match",
        1 => "Pick a side or spectate",
        2 => "Find another server; joining leaves this one",
        3 => "Name, model, saber, colours and Force",
        4 => "Host, map, game type and limits",
        5 => "Video, audio, HUD, game and network",
        6 => "Key bindings",
        7 => "Map, game type, kick, limits",
        8 | 9 if vote_active => "Cast your vote on the current call",
        8 if !vote_active => "Camera framing and smooth sunlight for recording",
        10 if vote_active => "Camera framing and smooth sunlight for recording",
        _ => "Disconnect or quit",
    }
}

/// Fixed-storage retained UI state shared by every in-game page.
pub(crate) struct InGameMenu {
    canvas: MenuCanvas,
    kicker: String,
    rows: [String; 24],
    enabled: [bool; 24],
    row_count: usize,
    callvote: callvote::State,
    about: about::State,
    active_page: Page,
    siege: siege::State,
    pub(crate) shot: shot::Panel,
    /// Layout family (`ui_menuStyle`).
    style: MenuStyle,
    /// Retail artwork the classic layout can draw.
    art: ArtSet,
}

impl InGameMenu {
    pub(crate) fn new() -> Self {
        Self {
            canvas: MenuCanvas::new(),
            kicker: String::with_capacity(48),
            rows: std::array::from_fn(|_| String::with_capacity(96)),
            enabled: [true; 24],
            row_count: 0,
            callvote: callvote::State::new(),
            about: about::State::new(),
            active_page: Page::Main,
            siege: siege::State::default(),
            shot: shot::Panel::default(),
            style: MenuStyle::Modern,
            art: ArtSet::default(),
        }
    }

    /// Follow the player's `ui_menuStyle`, with the retail artwork `art`
    /// the classic layout can draw.
    pub(crate) fn set_style(&mut self, style: MenuStyle, art: ArtSet) {
        self.style = style;
        self.art = art;
    }

    /// Whether the classic (retail bar and pop-ups) layout is in use.
    pub(crate) fn is_classic(&self) -> bool {
        self.style == MenuStyle::Classic
    }

    pub(crate) fn append(
        &mut self,
        view: View<'_>,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        if view.page == Page::Shot {
            self.active_page = Page::Shot;
            self.shot.build(&mut self.canvas, viewport);
            self.canvas.append_text(vertices, font, viewport);
            return;
        }
        self.prepare_rows(&view);
        self.active_page = view.page;
        let rows = view::Rows {
            labels: &self.rows[..self.row_count],
            enabled: &self.enabled[..self.row_count],
            scroll: self.callvote.scroll_metrics(view.page),
            info: info_lines(&self.about, view.page),
        };
        if self.style == MenuStyle::Classic {
            classic_view::build(&mut self.canvas, &view, rows, self.art, viewport);
        } else {
            view::build(
                &mut self.canvas,
                &view,
                &self.kicker,
                rows,
                hint_for(&view),
                viewport,
            );
        }
        self.canvas.append_text(vertices, font, viewport);
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        self.canvas.draw_list()
    }
    pub(crate) fn navigate(&mut self, action: AbstractAction) -> Option<usize> {
        if self.active_page.is_vote_page() {
            return None;
        }
        self.canvas.action(action).map(usize::from)
    }
    pub(crate) fn pointer(&mut self, event: InputEvent) -> Option<(UiEventKind, usize)> {
        let wheel = match event {
            InputEvent::PointerWheel { delta, .. } => Some(delta),
            _ => None,
        };
        let event = self.canvas.pointer(event);
        if let Some(delta) = wheel
            && self.active_page.is_vote_page()
        {
            self.callvote
                .scroll(self.active_page, -delta.y.signum() as isize * 3);
            return None;
        }
        let event = event?;
        if event.kind == UiEventKind::Drag && event.token == Some(VOTE_SCROLL_TOKEN) {
            if let (Some(point), Some(track)) =
                (event.position, self.canvas.rect_for(VOTE_SCROLL_TOKEN))
            {
                let ratio = ((point.y - track.y) / track.height).clamp(0.0, 1.0);
                self.callvote.scroll_to(self.active_page, ratio);
            }
            return None;
        }
        Some((event.kind, usize::from(event.token?)))
    }
    pub(crate) fn activation_allowed(&self, row: usize) -> bool {
        self.enabled.get(row).copied().unwrap_or(false)
    }

    /// Rebuild the server-info page from the live game state.
    pub(crate) fn refresh_about(&mut self, game_state: Option<&GameState>, address: Option<&str>) {
        self.about.refresh(game_state, address);
    }

    pub(crate) fn refresh_callvote(
        &mut self,
        game_state: Option<&GameState>,
        vfs: Option<&jkr_vfs::VirtualFileSystem>,
    ) {
        self.callvote.refresh(game_state, vfs);
    }

    pub(crate) fn callvote_action(&mut self, page: Page, row: usize) -> CallVoteAction {
        self.callvote.activate(page, row)
    }

    pub(crate) fn row_count(&self, page: Page, team_game: bool, vote_active: bool) -> usize {
        if let Some(count) = classic::row_count(page, team_game).filter(|_| self.is_classic()) {
            count
        } else if page == Page::Siege {
            self.siege.row_count()
        } else if page.is_vote_page() {
            self.callvote.row_count(page)
        } else {
            row_count(page, team_game, vote_active)
        }
    }

    fn prepare_rows(&mut self, view: &View<'_>) {
        self.kicker.clear();
        self.kicker.push_str(match view.team {
            1 => "JEDI ACADEMY   /   RED TEAM",
            2 => "JEDI ACADEMY   /   BLUE TEAM",
            3 => "JEDI ACADEMY   /   SPECTATING",
            _ => "JEDI ACADEMY   /   PLAYING",
        });
        for row in &mut self.rows {
            row.clear();
        }
        self.enabled = [true; 24];
        let classic_rows = if self.style == MenuStyle::Classic {
            classic::prepare(view, &mut self.rows, &mut self.enabled)
        } else {
            None
        };
        self.row_count = if let Some(count) = classic_rows {
            count
        } else if view.page == Page::Siege {
            self.siege.prepare(&mut self.rows)
        } else if view.page.is_vote_page() {
            self.callvote.prepare_rows(view.page, &mut self.rows)
        } else {
            self.prepare_standard_rows(view)
        };
        for row in 0..self.row_count {
            let current = current_team_row(view, row);
            self.enabled[row] &= !current;
            if current && self.style != MenuStyle::Classic {
                self.rows[row].push_str("  /  current");
            }
        }
    }

    fn prepare_standard_rows(&mut self, view: &View<'_>) -> usize {
        match view.page {
            Page::Main => {
                let mut count = 0;
                for value in [
                    "Resume",
                    "Join / change team",
                    "Server browser",
                    "Player profile",
                    "Server info",
                    "Settings",
                    "Controls",
                ] {
                    self.rows[count].push_str(value);
                    count += 1;
                }
                self.rows[count].push_str("Call vote");
                count += 1;
                if view.vote_active {
                    self.rows[count].push_str("Vote yes");
                    self.rows[count + 1].push_str("Vote no");
                    count += 2;
                }
                self.rows[count].push_str("Shot controls");
                count += 1;
                self.rows[count].push_str("Leave");
                count + 1
            }
            Page::About => {
                self.rows[0].push_str("Back");
                1
            }
            Page::Leave => {
                self.rows[0].push_str("Disconnect  /  back to the main menu");
                self.rows[1].push_str("Quit JKR");
                self.rows[2].push_str("Back");
                3
            }
            Page::Team => {
                if view.team_game {
                    let _ = write!(
                        self.rows[0],
                        "Auto-join  /  {} red / {} blue",
                        view.red_players, view.blue_players
                    );
                    let _ = write!(self.rows[1], "Red team  /  {} players", view.red_players);
                    let _ = write!(self.rows[2], "Blue team  /  {} players", view.blue_players);
                    self.rows[3].push_str("Spectate");
                    self.rows[4].push_str("Back");
                    5
                } else {
                    self.rows[0].push_str("Join game");
                    self.rows[1].push_str("Spectate");
                    self.rows[2].push_str("Back");
                    3
                }
            }
            _ => 0,
        }
    }
}

impl Page {
    pub(crate) const fn is_vote_page(self) -> bool {
        matches!(
            self,
            Self::CallVote
                | Self::VoteMap
                | Self::VoteGameType
                | Self::VoteKick
                | Self::VoteClientKick
                | Self::VoteWarmup
                | Self::VoteTimeLimit
                | Self::VoteFragLimit
        )
    }
}

pub(crate) fn weapon_name(weapon: u8) -> &'static str {
    match weapon {
        0 => "Unarmed",
        1 => "Stun Baton",
        2 => "Melee",
        3 => "Lightsaber",
        4 => "Blaster Pistol",
        5 => "E-11 Blaster Rifle",
        6 => "Disruptor Rifle",
        7 => "Bowcaster",
        8 => "Heavy Repeater",
        9 => "DEMP2",
        10 => "Flechette",
        11 => "Rocket Launcher",
        12 => "Thermal Detonator",
        13 => "Trip Mine",
        14 => "Det Pack",
        15 => "Concussion Rifle",
        16 => "Bryar Pistol (legacy)",
        17 => "Emplaced Gun",
        18 => "Turret",
        _ => "Unknown weapon",
    }
}

pub(crate) fn row_count(page: Page, team_game: bool, vote_active: bool) -> usize {
    match page {
        Page::Main => {
            if vote_active {
                12
            } else {
                10
            }
        }
        Page::Team if team_game => 5,
        Page::Team => 3,
        Page::About => 1,
        Page::Leave => 3,
        _ => 0,
    }
}

/// Read-only lines above the entries of `page` (the server-info page).
fn info_lines(about: &about::State, page: Page) -> &[String] {
    match page {
        Page::About => about.lines(),
        _ => &[],
    }
}

/// Whether `row` of the join page is the team the player is already on.
fn current_team_row(view: &View<'_>, row: usize) -> bool {
    if view.page != Page::Team {
        return false;
    }
    if view.team_game {
        matches!((row, view.team), (1, 1) | (2, 2) | (3, 3))
    } else {
        matches!((row, view.team), (0, 0..=2) | (1, 3))
    }
}

/// Count populated red/blue clientinfo records for the join modal.
pub(crate) fn team_sizes(game_state: &GameState) -> [usize; 2] {
    let mut sizes = [0; 2];
    for client in 0..32 {
        let Some(bytes) = game_state.config_string(1_131 + client) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let Ok(info) = InfoString::parse(text) else {
            continue;
        };
        match info.get_i32("t") {
            Some(1) => sizes[0] += 1,
            Some(2) => sizes[1] += 1,
            _ => {}
        }
    }
    sizes
}
