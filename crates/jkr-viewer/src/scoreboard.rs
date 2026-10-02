//! Cached authoritative scoreboard projection rendered through `jkr-ui`.

mod deaths;
mod view;

use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use jkr_client::{ClientSession, ScoreEntry};
use jkr_protocol::{GameState, InfoString};
use jkr_ui::DrawList;

const CS_PLAYERS: usize = 1_131;

/// Hide both chat geometry and text while scores are visible, without clearing chat state.
pub(crate) fn chat_visible(scores: bool, information: bool, history: bool) -> bool {
    !scores && (information || history)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScoreRow {
    pub(super) client_num: u8,
    pub(super) name: String,
    pub(super) team: u8,
    pub(super) score: i32,
    /// Optional session-observed count; no fabricated authoritative plugin statistic.
    pub(super) deaths: Option<i32>,
    pub(super) ping: i32,
    pub(super) time: i32,
}

/// Cached scoreboard data and fixed retained presentation storage.
pub(crate) struct Scoreboard {
    cached_scores: Vec<ScoreEntry>,
    config_signature: u64,
    server_signature: u64,
    team_game: bool,
    map: String,
    mode: String,
    rows: Vec<ScoreRow>,
    ui: MenuCanvas,
    deaths: deaths::Deaths,
}

impl Scoreboard {
    pub(crate) fn new() -> Self {
        Self {
            cached_scores: Vec::new(),
            config_signature: 0,
            server_signature: 0,
            team_game: false,
            map: String::with_capacity(64),
            mode: String::with_capacity(32),
            rows: Vec::with_capacity(32),
            ui: MenuCanvas::new(),
            deaths: deaths::Deaths::default(),
        }
    }
    pub(crate) fn draw_list(&self) -> &DrawList {
        self.ui.draw_list()
    }

    pub(crate) fn append(
        &mut self,
        session: &ClientSession,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        _scale: f32,
    ) {
        self.refresh(session);
        for row in &mut self.rows {
            row.deaths = self.deaths.count(row.client_num);
        }
        let local = session.latest_snapshot().player.client_num();
        view::build(
            &mut self.ui,
            &self.rows,
            view::MatchHeader {
                map: &self.map,
                mode: &self.mode,
                team_scores: session.team_scores(),
                team_game: self.team_game,
                local_client: local,
            },
            viewport,
        );
        self.ui.finish(u16::MAX);
        self.ui
            .append_text_styled(vertices, font, viewport, crate::text::TextStyle::NEUTRAL);
    }

    fn refresh(&mut self, session: &ClientSession) {
        let signature = config_signature(session.game_state(), session.scores());
        let server_signature = session
            .game_state()
            .config_string(0)
            .map_or(0, byte_signature);
        if self.cached_scores == session.scores()
            && self.config_signature == signature
            && self.server_signature == server_signature
        {
            return;
        }
        self.cached_scores.clear();
        self.cached_scores.extend_from_slice(session.scores());
        self.config_signature = signature;
        self.server_signature = server_signature;
        self.team_game = is_team_game(session.game_state());
        self.rows.clear();
        for score in session.scores() {
            let (name, team) = client_identity(session.game_state(), score.client_num);
            self.rows.push(ScoreRow {
                client_num: score.client_num,
                name,
                team,
                score: score.score,
                deaths: None,
                ping: score.ping,
                time: score.time_minutes,
            });
        }
        let server = server_info(session.game_state());
        self.map.clear();
        self.map.push_str(&server.0);
        self.mode.clear();
        self.mode.push_str(server.1);
    }
}

/// Append the current server scoreboard.
pub(crate) fn append_overlay(gpu: &mut crate::GpuState, viewport: [f32; 2], scale: f32) {
    if let Some(session) = &gpu.live_session {
        gpu.scoreboard.append(
            session,
            &mut gpu.text_vertices,
            &gpu.ui_font,
            viewport,
            scale,
        );
    }
}

fn server_info(game_state: &GameState) -> (String, &'static str) {
    let info = game_state
        .config_string(0)
        .and_then(|v| std::str::from_utf8(v).ok())
        .and_then(|v| InfoString::parse(v).ok());
    let Some(info) = info else {
        return ("UNKNOWN MAP".to_owned(), "MATCH");
    };
    let map = info.get("mapname").unwrap_or("UNKNOWN MAP").to_owned();
    let mode = match info.get_i32("g_gametype") {
        Some(0) => "FFA",
        Some(3) => "DUEL",
        Some(4) => "POWER DUEL",
        Some(6) => "TEAM FFA",
        Some(7) => "SIEGE",
        Some(8) => "CTF",
        Some(9) => "CTY",
        _ => "MATCH",
    };
    (map, mode)
}

fn client_identity(game_state: &GameState, client_num: u8) -> (String, u8) {
    game_state
        .config_string(CS_PLAYERS + usize::from(client_num))
        .map_or_else(
            || (format!("Client {client_num}"), 3),
            |bytes| client_identity_from_config(bytes, client_num),
        )
}
fn client_identity_from_config(bytes: &[u8], client_num: u8) -> (String, u8) {
    let info = jkr_client::LegacyClientInfo::new(bytes);
    (
        info.bytes("n")
            .or_else(|| info.bytes("name"))
            .filter(|n| !n.is_empty())
            .map_or_else(
                || format!("Client {client_num}"),
                // Latin-1, not UTF-8: see `jkr_client::decode_legacy`.
                |v| jkr_client::decode_legacy(v).into_owned(),
            ),
        info.integer("t")
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(3),
    )
}

fn is_team_game(game_state: &GameState) -> bool {
    game_state
        .config_string(0)
        .and_then(|v| std::str::from_utf8(v).ok())
        .and_then(|v| InfoString::parse(v).ok())
        .and_then(|v| v.get_i32("g_gametype"))
        .is_some_and(|v| v >= 6)
}
fn config_signature(game_state: &GameState, scores: &[ScoreEntry]) -> u64 {
    scores
        .iter()
        .fold(0xcbf29ce484222325_u64, |mut hash, score| {
            if let Some(bytes) =
                game_state.config_string(CS_PLAYERS + usize::from(score.client_num))
            {
                hash ^= byte_signature(bytes);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            hash
        })
}
fn byte_signature(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
