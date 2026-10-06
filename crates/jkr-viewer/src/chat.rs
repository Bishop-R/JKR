//! Floating conversation overlay. Chat owns presentation/history only; player
//! identity and stock command semantics come from the client adapter.

mod editing;
mod editor;

mod interaction;
mod layout;
mod options;
mod player_actions;
mod social;

mod view;

pub(crate) mod combat;

use crate::game_font::{GameFonts, RetailFont};
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use editor::Editor;
use jkr_client::{ChatRoster, ChatTarget, ServerEventKind, chat_body, chat_display_text};
use jkr_ui::{DrawList, Tween};
use std::collections::VecDeque;
use std::time::Instant;

const HISTORY_LIMIT: usize = 64;
const MAX_VISIBLE: usize = 8;
const HOLD_MS: u64 = 9_000;
const FADE_MS: u64 = 2_000;
const ENTER_MS: u32 = 220;

#[derive(Clone, Copy, PartialEq)]
enum Channel {
    Global,
    Team,
    Whisper,
}

struct ChatLine {
    name: String,
    body: String,
    sender: Option<ChatTarget>,
    channel: Channel,
    received_ms: u64,
    muted: bool,
    wrap: layout::Wrapped,
    y: Option<Tween>,
}

#[derive(Clone, Copy)]
struct PlayerMenu {
    target: ChatTarget,
    anchor_y: f32,
    selected: Option<u16>,
}

/// Bounded incoming history and the currently active unboxed composer.
pub(crate) struct ChatOverlay {
    lines: VecDeque<ChatLine>,
    center: Option<(String, u64)>,
    /// Text ids of this frame's centre-print rows, which retail drew with the
    /// medium font rather than the chat box's small one.
    center_text: (u32, u32),
    combat: combat::Combat,
    input: Option<Editor>,
    modifiers: winit::keyboard::ModifiersState,
    roster: ChatRoster,
    muted: Vec<ChatTarget>,
    friends: social::Friends,
    player_menu: Option<PlayerMenu>,
    scroll: usize,
    unread: usize,
    clock: Instant,
    opened_ms: u64,
    notice: &'static str,
    ui: MenuCanvas,
    visible_targets: [Option<ChatTarget>; MAX_VISIBLE],
    pressed_action: Option<(u16, Option<ChatTarget>)>,
    layout_viewport: [f32; 2],
    scoreboard_layout: bool,
    options: options::Options,
}

impl ChatOverlay {
    /// Borrow an existing coloured roster name for the overhead HUD; never copy it.
    pub(crate) fn player_label(&self, slot: u16) -> &str {
        self.roster
            .target(Some(slot))
            .and_then(|target| self.roster.display_name(target))
            .unwrap_or("")
    }
    pub(crate) fn new() -> Self {
        Self {
            lines: VecDeque::with_capacity(HISTORY_LIMIT),
            center: None,
            center_text: (0, 0),
            combat: combat::Combat::default(),
            input: None,
            modifiers: winit::keyboard::ModifiersState::empty(),
            roster: ChatRoster::default(),
            muted: Vec::with_capacity(HISTORY_LIMIT),
            friends: social::Friends::default(),
            player_menu: None,
            scroll: 0,
            unread: 0,
            clock: Instant::now(),
            opened_ms: 0,
            notice: "",
            ui: MenuCanvas::with_text_capacity(512),
            visible_targets: [None; MAX_VISIBLE],
            pressed_action: None,
            layout_viewport: [0.0; 2],
            scoreboard_layout: false,
            options: options::Options::default(),
        }
    }

    fn millis(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.clock)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    pub(crate) fn update_roster(&mut self, game: &jkr_protocol::GameState) {
        self.roster.update(game);
        self.muted
            .retain(|target| self.roster.name(*target).is_some());
    }

    pub(crate) fn receive(
        &mut self,
        kind: ServerEventKind,
        mut text: String,
        sender: Option<u16>,
        now: Instant,
    ) {
        // Console output never enters conversation history, including direct callers.
        if kind == ServerEventKind::Print {
            return;
        }
        let ms = self.millis(now);
        if kind == ServerEventKind::CenterPrint {
            text.reserve(1024usize.saturating_sub(text.len()));
            self.combat.height = None;
            self.center = Some((text, ms));
            return;
        }
        if kind == ServerEventKind::Chat && self.options.clean != 0 {
            let plain = options::clean_body(&text, 1).to_ascii_lowercase();
            if plain.contains("media - currently playing: ") || plain.contains("hi everybody!") {
                return;
            }
        }
        let target = self.roster.target(sender);
        // Display keeps `^n`; identity, whisper destinations and muting keep
        // using the plain roster name.
        let name = target
            .and_then(|target| self.roster.display_name(target))
            .unwrap_or("")
            .to_owned();
        let display = chat_display_text(&text);
        let (body, private) = if name.is_empty() {
            (display.as_str(), false)
        } else {
            chat_body(&display, &name)
        };
        let body = options::clean_body(body, self.options.clean);
        if self.options.clean != 0
            && self
                .lines
                .back()
                .is_some_and(|line| line.sender == target && line.body == body)
        {
            return;
        }
        let line = ChatLine {
            name,
            body,
            sender: target,
            channel: if private {
                Channel::Whisper
            } else if kind == ServerEventKind::TeamChat {
                Channel::Team
            } else {
                Channel::Global
            },
            received_ms: ms,
            muted: target.is_some_and(|target| self.muted.contains(&target)),
            wrap: layout::Wrapped::default(),
            y: None,
        };
        if self.lines.len() == HISTORY_LIMIT {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
        if self.is_typing() && self.scroll > 0 {
            self.scroll = (self.scroll + 1).min(self.lines.len().saturating_sub(1));
            self.unread = (self.unread + 1).min(HISTORY_LIMIT);
        }
    }

    pub(crate) fn open(&mut self, team: bool) {
        self.input = Some(Editor::new(if team {
            Channel::Team
        } else {
            Channel::Global
        }));
        self.player_menu = None;
        self.pressed_action = None;
        self.scroll = 0;
        self.unread = 0;
        self.notice = "";
        self.opened_ms = self.millis(Instant::now());
    }

    pub(crate) fn is_typing(&self) -> bool {
        self.input.is_some()
    }

    /// Toggle a whisper composer only for a currently occupied roster slot.
    pub(crate) fn whisper_to(&mut self, slot: Option<u16>) -> bool {
        let Some(target) = self.roster.target(slot) else {
            return false;
        };
        if self.is_typing() {
            self.input = None;
            return true;
        }
        self.open(false);
        let input = self.input.as_mut().expect("opened composer");
        input.channel = Channel::Whisper;
        input.recipient = Some(target);
        true
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        self.ui.draw_list()
    }

    pub(crate) fn set_scoreboard_layout(&mut self, enabled: bool) {
        if self.scoreboard_layout != enabled {
            self.scoreboard_layout = enabled;
            for line in &mut self.lines {
                line.y = None;
            }
            self.pressed_action = None;
        }
    }

    /// Lay out and append chat text: the chat box in the small game font and
    /// centre prints in the medium one when `ui_gameFont` has them loaded
    /// (`CG_ChatBox_DrawStrings` paints with `FONT_SMALL`, `CG_DrawCenterString`
    /// with `FONT_MEDIUM`), otherwise in `font`.
    pub(crate) fn append(
        &mut self,
        draw_feed: bool,
        fonts: &mut GameFonts,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        _scale: f32,
    ) {
        let ms = self.millis(Instant::now());
        self.build(
            draw_feed,
            fonts.font(RetailFont::Small).unwrap_or(font),
            viewport,
            ms,
        );
        let (start, end) = self.center_text;
        self.ui.append_text_routed(
            fonts,
            |id, _| {
                Some(if (start..end).contains(&id.0) {
                    RetailFont::Medium
                } else {
                    RetailFont::Small
                })
            },
            vertices,
            font,
            viewport,
        );
    }
}

pub(crate) enum ChatInputResult {
    None,
    Submit(String),
}
