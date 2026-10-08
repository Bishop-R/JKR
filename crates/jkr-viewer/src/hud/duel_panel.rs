//! The duel spectator panel (`CG_DrawSpectator`, `cg_draw.c`): while a free spectator
//! watches a duel, the duellists' portraits at the bottom left and right (a power duel's
//! third above the right one), their health bars when the server shows duel healths
//! (`g_showDuelHealths`, duels only; `CG_DrawDuelistHealth`), and in a duel each one's
//! score out of the fraglimit under its portrait. The names are the spectator line above.

use super::portrait::Portrait;
use jkr_protocol::{GameState, PlayerState};
use jkr_ui::{Color, DrawCommand, DrawList, Rect, TextAlign, TextId, TextOverflow};

/// `PMF_FOLLOW`, `TEAM_SPECTATOR`, `CS_CLIENT_DUELISTS`, `CS_CLIENT_DUELHEALTHS`.
const PMF_FOLLOW: u16 = 4_096;
const TEAM_SPECTATOR: u8 = 3;
const CS_CLIENT_DUELISTS: usize = 30;
const CS_CLIENT_DUELHEALTHS: usize = 31;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
/// The portraits' size and the first score's text id, on the 640 by 480 screen.
const SIZE: f32 = 64.0;
pub(super) const SCORE_TEXT: u32 = 334;

/// This frame's panel.
pub(crate) struct DuelPanel {
    active: bool,
    power: bool,
    duelists: [i32; 3],
    healths: [i32; 2],
    show_healths: bool,
    scores: [String; 2],
    portraits: [Portrait; 3],
}

impl Default for DuelPanel {
    fn default() -> Self {
        let cell = |index| jkr_ui::TextureId(crate::ui_renderer::ICON_CELLS + index);
        Self {
            active: false,
            power: false,
            duelists: [-1; 3],
            healths: [0; 2],
            show_healths: false,
            scores: [String::with_capacity(16), String::with_capacity(16)],
            // The HUD's atlas cells past its pictures (`icon_assets::COUNT`).
            portraits: [
                Portrait::at(cell(124)),
                Portrait::at(cell(125)),
                Portrait::at(cell(126)),
            ],
        }
    }
}

/// `CG_DrawDuelistHealth` through `CG_DrawHealthBarRough` (64 by 8 at `x`, `y`): a red
/// filling for the health left, a darker red for the health lost, a white shell.
fn health_bar(list: &mut DrawList, x: f32, y: f32, s: f32, health: i32) {
    // `MAX_HEALTH_FOR_IFACE`.
    let ratio = (health as f32 / 100.0).clamp(0.0, 1.0);
    let red = ratio * 0.2 + 0.5;
    let filled = (SIZE * ratio - 1.0).max(0.0);
    for (left, width, red) in [(1.0, filled, red), (filled, SIZE - filled, red * 0.5)] {
        let _ = list.push(DrawCommand::SolidRect {
            rect: Rect::new(x + left * s, y + s, width * s, 5.0 * s),
            color: Color::new(red, 0.0, 0.0, 0.7),
        });
    }
    let _ = list.push(DrawCommand::Border {
        rect: Rect::new(x, y, SIZE * s, 8.0 * s),
        radius: 0.0,
        width: s,
        color: Color::new(1.0, 1.0, 1.0, 1.0),
    });
}

/// Numbers split by `|` (`SetDuelistHealthsFromConfigString` and the duellists' list).
fn numbers<const N: usize>(game: &GameState, index: usize, missing: i32) -> [i32; N] {
    let mut values = [missing; N];
    if let Some(text) = game
        .config_string(index)
        .and_then(|text| std::str::from_utf8(text).ok())
    {
        for (value, part) in values.iter_mut().zip(text.split('|')) {
            *value = jkr_game_jka::userinfo::atoi(part.as_bytes());
        }
    }
    values
}

impl DuelPanel {
    /// The panel for `player` on this server; `scores` are the last scores the server
    /// sent (`cgs.clientinfo[].score`).
    pub(crate) fn update(
        &mut self,
        game: &GameState,
        player: &PlayerState,
        scores: &[jkr_client::ScoreEntry],
    ) {
        let info = game.config_string(0).unwrap_or_default();
        let integer = |key: &str| jkr_client::LegacyClientInfo::new(info).integer(key);
        let gametype = integer("g_gametype").unwrap_or(0) as i32;
        self.duelists = numbers(game, CS_CLIENT_DUELISTS, -1);
        self.active = matches!(gametype, GT_DUEL | GT_POWERDUEL)
            && player.team() == TEAM_SPECTATOR
            && player.movement_flags() & PMF_FOLLOW == 0
            && self.duelists[0] >= 0
            && self.duelists[1] >= 0;
        self.power = gametype == GT_POWERDUEL;
        self.show_healths = gametype == GT_DUEL && integer("g_showDuelHealths").unwrap_or(0) >= 1;
        self.healths = numbers(game, CS_CLIENT_DUELHEALTHS, 0);
        let fraglimit = integer("fraglimit").unwrap_or(0);
        for (text, duelist) in self.scores.iter_mut().zip(self.duelists) {
            use std::fmt::Write as _;
            text.clear();
            let score = scores
                .iter()
                .find(|entry| i32::from(entry.client_num) == duelist)
                .map_or(0, |entry| entry.score);
            let _ = write!(text, "{score}/{fraglimit}");
        }
    }

    /// A score line, by `SCORE_TEXT` order.
    pub(super) fn score(&self, index: usize) -> &str {
        self.scores.get(index).map_or("", String::as_str)
    }

    /// Load the duellists' portraits when they change.
    pub(crate) fn update_portraits(
        &mut self,
        game: &GameState,
        vfs: &jkr_vfs::VirtualFileSystem,
        shaders: &jkr_shader::ShaderCatalog,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        for (portrait, duelist) in self.portraits.iter_mut().zip(self.duelists) {
            let client = (self.active && duelist >= 0).then_some(duelist as u16);
            portrait.update(game, client, vfs, shaders, renderer, queue);
        }
    }

    /// Draw it, the left items from the left edge and the right ones from the right.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2]) {
        if !self.active {
            return;
        }
        let [w, h] = viewport;
        let s = h / 480.0;
        let (left, right) = (10.0 * s, w - (SIZE + 10.0) * s);
        let top = 480.0 - SIZE * 1.5;
        let size = SIZE * s;
        self.portraits[0].emit_at(list, Rect::new(left, top * s, size, size));
        self.portraits[1].emit_at(list, Rect::new(right, top * s, size, size));
        if self.power && self.duelists[2] >= 0 {
            let third = 480.0 - SIZE * 2.8;
            self.portraits[2].emit_at(list, Rect::new(right, third * s, size, size));
        }
        if self.show_healths {
            for (x, health) in [(left, self.healths[0]), (right, self.healths[1])] {
                health_bar(list, x, (top - 12.0) * s, s, health);
            }
        }
        if !self.power {
            for (index, x) in [left, right].into_iter().enumerate() {
                let _ = list.push(DrawCommand::Text {
                    rect: Rect::new(x - 16.0 * s, (top + SIZE) * s, size + 32.0 * s, 16.0 * s),
                    text: TextId(SCORE_TEXT + index as u32),
                    size: 12.0 * s,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                    align: TextAlign::Center,
                    overflow: TextOverflow::Clip,
                    weight: jkr_ui::FontWeight::Regular,
                    letter_spacing: 0.0,
                });
            }
        }
    }
}
