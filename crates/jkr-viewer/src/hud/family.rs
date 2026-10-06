//! Player-facing HUD policy; retained values and geometry, never parallel game state.
use super::*;
use jkr_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use jkr_ui::{DrawCommand, FontWeight, Rect, TextAlign, TextOverflow};
use std::fmt::Write;

/// Register only options with consumers in this module or the existing widget emitter.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    for (name, value, help) in [
        (
            "cg_drawScore",
            2,
            "Local score: 0 hidden, 1 personal, 2 includes team scores",
        ),
        ("cg_drawSnapshot", 0, "Snapshot time and message number"),
        ("cg_drawUpperRight", 1, "Show upper-right match information"),
        ("cg_drawVote", 1, "Show vote information"),
        ("cg_drawTimerCountdown", 0, "Count down a timed match"),
        ("cg_drawTimerMsec", 0, "Show timer milliseconds"),
        (
            "cg_drawInventory",
            1,
            "Show owned inventory and the inventory selector",
        ),
        ("cg_drawCrosshairNamesColours", 1, "Colored target names"),
        ("cg_drawTeamOverlayWeapons", 0, "Show teammate weapons"),
        ("cg_killfeed", 0, "Show the obituary feed"),
        (
            "cg_killfeedAlignment",
            0,
            "Obituaries: 0 right, 1 left, 2 center",
        ),
        ("cg_killfeedReverse", 0, "Show attacker before victim"),
    ] {
        cvars.register(CvarDefinition::new(
            name,
            value as i64,
            CvarFlags::ARCHIVE,
            help,
        ))?;
    }
    for (name, value, help) in [
        ("cg_drawScoreX", 0.0, "Personal score horizontal offset"),
        ("cg_drawScoreY", 0.0, "Personal score upward offset"),
        ("cg_drawCrosshairNamesOpacity", 1.0, "Target name opacity"),
        (
            "cg_drawTeamOverlayX",
            640.0,
            "Team overlay right edge in virtual units",
        ),
        (
            "cg_drawTeamOverlayY",
            0.0,
            "Team overlay top; zero preserves HUD layout",
        ),
        ("cg_drawTeamOverlayScale", 1.0, "Team overlay scale"),
        ("cg_gunX", 0.0, "View weapon forward offset"),
        ("cg_gunY", 0.0, "View weapon left offset"),
        ("cg_gunZ", 0.0, "View weapon upward offset"),
        (
            "cg_killfeedX",
            0.0,
            "Obituary horizontal offset in virtual units",
        ),
        (
            "cg_killfeedY",
            0.0,
            "Obituary vertical offset in virtual units",
        ),
        (
            "cg_killfeedTextSize",
            0.8,
            "Obituary text scale; zero uses default",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    Ok(())
}

/// Allocation-free integer lookup using a canonical lowercase name.
pub(crate) fn integer(console: Option<&ViewerConsole>, name: &str, fallback: i64) -> i64 {
    console
        .and_then(|c| c.integer_cvar(name))
        .unwrap_or(fallback)
}

/// The existing FPS cvar is case insensitive; do not register a duplicate.
pub(crate) fn fps(console: Option<&ViewerConsole>) -> bool {
    integer(console, "cg_drawupperright", 1) != 0
        && console
            .and_then(|c| c.bool_cvar("cg_drawfps"))
            .unwrap_or(false)
}

/// Cached presentation policy for widget emission.
#[derive(Clone, Copy)]
pub(super) struct Policy {
    /// Personal/team score mode.
    pub(super) score: i64,
    /// Virtual-coordinate score offsets.
    pub(super) score_offset: [f32; 2],
    /// Upper-right group visibility.
    pub(super) upper: bool,
    /// Snapshot diagnostic visibility.
    pub(super) snapshot: bool,
    /// Vote visibility.
    pub(super) vote: bool,
    /// Inventory selector visibility.
    pub(super) inventory: bool,
    /// Match clock direction and precision.
    pub(super) timer: [bool; 2],
    /// Whether names retain colors.
    pub(super) name_colors: bool,
    /// Name alpha multiplier.
    pub(super) name_alpha: f32,
    /// Team overlay right/top coordinates and size multiplier.
    pub(super) team: [f32; 3],
    /// Include weapons in teammate gear text.
    pub(super) team_weapons: bool,
    /// Obituary feed visibility.
    pub(super) kills: bool,
    /// Obituary horizontal alignment.
    pub(super) kill_align: i64,
    /// Attacker-first obituary phrasing.
    pub(super) kill_reverse: bool,
    /// Obituary virtual offsets and text multiplier, sampled before drawing.
    pub(super) kill_geometry: [f32; 3],
}

impl Default for Policy {
    fn default() -> Self {
        Self::read(None)
    }
}

impl Policy {
    /// Read live cvars once before projecting the HUD.
    pub(super) fn read(console: Option<&ViewerConsole>) -> Self {
        let i = |name, fallback| integer(console, name, fallback);
        let f = |name, fallback| crate::cgame_options::scalar(console, name, fallback);
        Self {
            score: i("cg_drawscore", 2),
            score_offset: [f("cg_drawscorex", 0.0), f("cg_drawscorey", 0.0)],
            upper: i("cg_drawupperright", 1) != 0,
            snapshot: i("cg_drawsnapshot", 0) != 0,
            vote: i("cg_drawvote", 1) != 0,
            inventory: i("cg_drawinventory", 1) != 0,
            timer: [
                i("cg_drawtimercountdown", 0) != 0,
                i("cg_drawtimermsec", 0) != 0,
            ],
            name_colors: i("cg_drawcrosshairnamescolours", 1) != 0,
            name_alpha: f("cg_drawcrosshairnamesopacity", 1.0).clamp(0.0, 1.0),
            team: [
                f("cg_drawteamoverlayx", 640.0),
                f("cg_drawteamoverlayy", 0.0),
                f("cg_drawteamoverlayscale", 1.0).clamp(0.1, 4.0),
            ],
            team_weapons: i("cg_drawteamoverlayweapons", 0) != 0,
            kills: i("cg_killfeed", 0) != 0,
            kill_align: i("cg_killfeedalignment", 0),
            kill_reverse: i("cg_killfeedreverse", 0) != 0,
            kill_geometry: [
                f("cg_killfeedx", 0.0),
                f("cg_killfeedy", 0.0),
                match f("cg_killfeedtextsize", 0.8) {
                    0.0 => 1.0,
                    value => (value / 0.8).clamp(0.1, 8.0),
                },
            ],
        }
    }

    /// Filter existing widgets without rebuilding their retained layout document.
    pub(super) fn visible(self, binding: Option<&str>) -> bool {
        match binding {
            Some("vote_panel") => self.vote,
            Some("kill_feed") => self.kills,
            Some("match_timer" | "team_rows") => self.upper,
            _ => true,
        }
    }

    /// Position the team group in the viewport, retaining hero row sizing.
    pub(super) fn team_rect(self, rect: Rect, viewport: [f32; 2]) -> Rect {
        Rect::new(
            rect.x + (self.team[0] - 640.0) * viewport[0] / 640.0,
            if self.team[1] == 0.0 {
                rect.y
            } else {
                self.team[1] * viewport[1] / 480.0
            },
            rect.width,
            rect.height,
        )
    }
}

impl HudOverlay {
    /// Update score and snapshot labels in their preallocated storage.
    pub(crate) fn update_family(
        &mut self,
        snapshot: &jkr_protocol::Snapshot,
        game: &GameState,
        console: Option<&ViewerConsole>,
    ) {
        self.family = Policy::read(console);
        self.icons.update(snapshot, game, console);
        self.targeting.update(console, &snapshot.player);
        self.inventory_bits = if snapshot.player.health() > 0
            && snapshot.player.movement_type() != 4
            && snapshot.player.stats[1] != 0
        {
            snapshot.player.stats[2] & !(1 << 7)
        } else {
            0
        };
        self.score_text.clear();
        let _ = write!(
            self.score_text,
            "SCORE {}",
            snapshot.player.persistent[0] as i32
        );
        if self.family.score > 1 && matches!(snapshot.player.team(), 1 | 2) {
            for index in [6, 7] {
                let value = config_number(game, index);
                let _ = write!(self.score_text, " / {value}");
            }
        }
        self.snapshot_text.clear();
        let _ = write!(
            self.snapshot_text,
            "time:{} snap:{}",
            snapshot.server_time, snapshot.message_sequence
        );
    }

    /// Optional right-side readouts follow the leader block; explicit team
    /// overlay coordinates remain the player's choice.
    pub(super) fn upper_right_stack(&self) -> [f32; 3] {
        let bottom = self.enemy_info.bottom();
        let snapshot = if bottom > 0.0 { bottom + 16.0 } else { 50.0 };
        let after_snapshot = snapshot + if self.family.snapshot { 28.0 } else { 0.0 };
        let inventory = 100.0_f32.max(after_snapshot);
        let items = if self.family.upper && self.family.inventory {
            (self.inventory_bits & 0x0ffe).count_ones()
        } else {
            0
        };
        let team = if bottom == 0.0 {
            0.0
        } else if items > 0 {
            inventory + items as f32 * 26.0 + 12.0
        } else {
            after_snapshot
        };
        [snapshot, inventory, team]
    }

    /// Append independent score and diagnostic text without a backplate.
    pub(super) fn emit_family(&mut self, viewport: [f32; 2]) {
        self.targeting
            .emit(&mut self.draw_list, viewport, self.theme);
        self.enemy_info
            .emit(&mut self.draw_list, viewport, self.theme);
        let s = crate::ui_scale::height_scale(viewport[1]);
        let [snapshot_top, inventory_top, _] = self.upper_right_stack();
        if self.family.upper && self.family.inventory {
            let mut row = 0;
            for tag in 1..12 {
                if self.inventory_bits & (1 << tag) == 0 {
                    continue;
                }
                let _ = self.draw_list.push(DrawCommand::Text {
                    rect: Rect::new(
                        viewport[0] - 330.0 * s,
                        (inventory_top + row as f32 * 26.0) * s,
                        290.0 * s,
                        25.0 * s,
                    ),
                    text: TextId(1100 + tag),
                    size: 20.0 * s,
                    color: self.theme.muted,
                    align: TextAlign::End,
                    overflow: TextOverflow::Clip,
                    weight: FontWeight::Regular,
                    letter_spacing: 0.0,
                });
                row += 1;
            }
        }
        for (enabled, id, y, offset) in [
            (
                self.family.score != 0,
                315,
                viewport[1] - 160.0 * s,
                self.family.score_offset,
            ),
            (
                self.family.upper && self.family.snapshot,
                316,
                snapshot_top * s,
                [0.0; 2],
            ),
        ] {
            if !enabled {
                continue;
            }
            let _ = self.draw_list.push(DrawCommand::Text {
                rect: Rect::new(
                    viewport[0] - 480.0 * s + offset[0] * viewport[0] / 640.0,
                    y - offset[1] * viewport[1] / 480.0,
                    440.0 * s,
                    28.0 * s,
                ),
                text: TextId(id),
                size: 22.0 * s,
                color: self.theme.foreground,
                align: TextAlign::End,
                overflow: TextOverflow::Clip,
                weight: FontWeight::Semibold,
                letter_spacing: 0.0,
            });
        }
    }
}

fn config_number(game: &GameState, index: usize) -> i32 {
    game.config_string(index)
        .and_then(|s| std::str::from_utf8(s).ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Retain powerup text while hiding the optional teammate weapon label.
pub(super) fn gear(text: &mut String, entry: TeamInfo, weapons: bool) {
    options::format_team_gear(text, entry);
    if !weapons {
        let end = text.find(" · ").map_or(text.len(), |i| i + " · ".len());
        text.drain(..end);
    }
}

/// Change participant order without discarding the obituary's cause.
pub(super) fn obituary(
    text: &mut String,
    victim: &str,
    attacker: &str,
    phrase: &str,
    reverse: bool,
) {
    text.clear();
    if reverse {
        let _ = write!(text, "{attacker} → {victim} ({phrase})");
    } else {
        let _ = write!(text, "{victim} {phrase} {attacker}");
    }
}

/// Reformat the retained match clock, including optional countdown and milliseconds.
pub(super) fn timer(text: &mut String, game: &GameState, time: i32, policy: Policy) {
    let elapsed = time.saturating_sub(config_number(game, 21)).max(0);
    let mut limit: i32 = 0;
    if let Some(bytes) = game.config_string(0) {
        let mut fields = bytes.split(|b| *b == b'\\').skip(1);
        while let (Some(key), Some(value)) = (fields.next(), fields.next()) {
            if key == b"timelimit" {
                limit = std::str::from_utf8(value)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                break;
            }
        }
    }
    let limit = limit.saturating_mul(60_000);
    let time = if policy.timer[0] && limit > 0 && elapsed <= limit {
        limit - elapsed
    } else {
        elapsed
    };
    text.clear();
    let _ = write!(text, "{}:{:02}", time / 60_000, time / 1000 % 60);
    if policy.timer[1] {
        let _ = write!(text, ".{:03}", time % 1000);
    }
}

/// Copy display text while optionally removing Quake color escapes, without allocation.
pub(super) fn name(text: &mut String, value: &str, colors: bool) {
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if !colors && c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else {
            text.push(c);
        }
    }
}
