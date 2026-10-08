//! The movement keys (`cg_movementKeys`, TaystJK's `DF_DrawMovementKeys`, after EternalJK's
//! and jaPRO's): which of jump, forward, crouch, left, back and right are held, as keys in
//! the 640 by 480 screen's lower middle (style 1), with attack and alternate attack beside
//! them (2), only the held ones small round the crosshair (3) or larger below it (4); with
//! `cg_movementKeysWalk`, the walk key too. `cg_movementKeysX`/`Y` move them and
//! `cg_movementKeysSize` scales them. The player's own keys come from its last command; a
//! followed player's or a demo's are read from the player state as TaystJK reads them
//! (`DF_SetClientCmd`). The mods draw their own key pictures, which are not in the game's
//! data; these keys are drawn plainly.

use super::*;
use jkr_protocol::UserCommand;
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign, TextOverflow};

/// `BUTTON_ATTACK`, `BUTTON_WALKING`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_WALKING: u16 = 16;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `PMF_DUCKED`, `PMF_JUMP_HELD`, `PMF_FOLLOW`; `EF_FIRING`, `EF_ALT_FIRING`.
const PMF_DUCKED: u16 = 1;
const PMF_JUMP_HELD: u16 = 2;
const PMF_FOLLOW: u16 = 4_096;
const EF_FIRING: u32 = 1 << 9;
const EF_ALT_FIRING: u32 = 1 << 10;
/// `ENTITYNUM_NONE`: no ground under the player.
const ENTITYNUM_NONE: u16 = 1_023;
/// `BOTH_WALK1` .. `BOTH_WALKBACK_DUAL`, `BOTH_WALKBACK1`, `BOTH_WALKBACK2`
/// (`CG_IsWalkingAnim`); `BOTH_ROLL_F` .. `BOTH_ROLL_R` (`CG_InRollAnim`).
const WALKS: [u16; 8] = [1_102, 1_103, 1_104, 1_105, 1_106, 1_107, 1_134, 1_135];
const ROLLS: std::ops::RangeInclusive<u16> = 1_167..=1_170;
/// The keys' first text id.
pub(super) const FIRST_TEXT: u32 = 340;
const LABELS: [&str; 9] = ["J", "W", "C", "A", "S", "D", "M1", "M2", "Sh"];

/// The keys in [`LABELS`] order.
#[derive(Clone, Copy, Default, PartialEq)]
struct Held([bool; 9]);

impl Held {
    /// A command's keys.
    fn from_command(command: &UserCommand) -> Self {
        Self([
            command.up_move > 0,
            command.forward_move > 0,
            command.up_move < 0,
            command.right_move < 0,
            command.forward_move < 0,
            command.right_move > 0,
            command.buttons & BUTTON_ATTACK != 0,
            command.buttons & BUTTON_ALT_ATTACK != 0,
            command.buttons & BUTTON_WALKING != 0,
        ])
    }

    /// A player's keys read from its state (`DF_SetClientCmd`): the way it moves, a jump
    /// while it rises faster in the air or holds jump, a crouch while ducked or rolling,
    /// its firing, and a walk animation for the walk key.
    fn from_player(player: &PlayerState, last_rise: f32) -> Self {
        let [x, y, z] = player.velocity();
        let direction = if x.hypot(y) < 9.0 {
            -1
        } else {
            i32::from(player.movement_direction())
        };
        // `DF_DirToCmd`: 0 forward, then a quarter turn left for every two.
        let (forward, right) = match direction {
            0 => (1, 0),
            1 => (1, -1),
            2 => (0, -1),
            3 => (-1, -1),
            4 => (-1, 0),
            5 => (-1, 1),
            6 => (0, 1),
            7 => (1, 1),
            _ => (0, 0),
        };
        let flags = player.movement_flags();
        let airborne = player.ground_entity_num() == ENTITYNUM_NONE;
        let jump = (airborne && z > 8.0 && z > last_rise) || flags & PMF_JUMP_HELD != 0;
        let crouch = !jump && (flags & PMF_DUCKED != 0 || ROLLS.contains(&player.leg_animation()));
        let firing = player.entity_flags();
        Self([
            jump,
            forward > 0,
            crouch,
            right < 0,
            forward < 0,
            right > 0,
            firing & EF_FIRING != 0 && firing & EF_ALT_FIRING == 0,
            firing & EF_ALT_FIRING != 0,
            WALKS.contains(&player.leg_animation()),
        ])
    }
}

/// This frame's keys and how they are shown.
#[derive(Default)]
pub(crate) struct MovementKeys {
    style: i64,
    walk: bool,
    offset: [f32; 2],
    size: f32,
    held: Held,
    /// The followed player's last upward speed (`cg.lastZSpeed`).
    last_rise: f32,
}

impl MovementKeys {
    /// Read the options and the keys: `command` is this client's own last command while it
    /// plays (not following, not a demo).
    pub(crate) fn update(
        &mut self,
        console: Option<&crate::console::ViewerConsole>,
        player: &PlayerState,
        command: Option<&UserCommand>,
    ) {
        let integer = |name, fallback| {
            console
                .and_then(|console| console.integer_cvar(name))
                .unwrap_or(fallback)
        };
        let scalar = |name, fallback| crate::cgame_options::scalar(console, name, fallback);
        self.style = integer("cg_movementKeys", 0);
        if self.style == 0 {
            return;
        }
        self.walk = integer("cg_movementKeysWalk", 0) != 0;
        self.offset = [
            scalar("cg_movementKeysX", 0.0),
            scalar("cg_movementKeysY", 0.0),
        ];
        self.size = scalar("cg_movementKeysSize", 1.0);
        let following = player.movement_flags() & PMF_FOLLOW != 0;
        self.held = match command.filter(|_| !following) {
            Some(command) => Held::from_command(command),
            None => Held::from_player(player, self.last_rise),
        };
        self.last_rise = player.velocity()[2];
    }

    /// A key's label, by text id.
    pub(super) fn label(index: usize) -> &'static str {
        LABELS.get(index).copied().unwrap_or("")
    }

    /// Draw them in `DF_DrawMovementKeys`'s places, keeping the keys square on any screen.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2], accent: Color) {
        let [w, h] = viewport;
        let s = h / 480.0;
        let [dx, dy] = self.offset;
        // Each key's cell (column, row) in a layout, and whether released keys show.
        let (edge, x, y, cells, released): (f32, f32, f32, [Option<[f32; 2]>; 9], bool) =
            match self.style {
                1 | 2 => {
                    let edge = 16.0 * self.size;
                    let left = match (self.style, self.walk) {
                        (1, true) => 1.0,
                        (1, false) => 1.5,
                        (_, true) => 1.5,
                        (_, false) => 2.0,
                    };
                    let attacks = self.style == 2;
                    (
                        edge,
                        320.0 + dx - edge * left,
                        432.0 + dy - edge,
                        [
                            Some([0.0, 0.0]),
                            Some([1.0, 0.0]),
                            Some([2.0, 0.0]),
                            Some([0.0, 1.0]),
                            Some([1.0, 1.0]),
                            Some([2.0, 1.0]),
                            attacks.then_some([3.0, 0.0]),
                            attacks.then_some([3.0, 1.0]),
                            self.walk.then_some([-1.0, 1.0]),
                        ],
                        true,
                    )
                }
                3 | 4 => {
                    let (edge, x, y) = if self.style == 3 {
                        let edge = 6.0 * self.size;
                        (edge, 320.0 - edge * 1.5, 240.0 - edge * 1.5)
                    } else {
                        let edge = 12.0 * self.size;
                        (edge, 320.0 + dx - edge * 1.5, 432.0 + dy - edge * 1.5)
                    };
                    (
                        edge,
                        x,
                        y,
                        [
                            Some([0.0, 0.0]),
                            Some([1.0, 0.0]),
                            Some([2.0, 0.0]),
                            Some([0.0, 1.0]),
                            Some([1.0, 2.0]),
                            Some([2.0, 1.0]),
                            Some([0.0, 2.0]),
                            Some([2.0, 2.0]),
                            self.walk.then_some([-1.0, 2.0]),
                        ],
                        false,
                    )
                }
                _ => return,
            };
        let labels = edge >= 10.0;
        for (index, cell) in cells.into_iter().enumerate() {
            let Some([column, row]) = cell else {
                continue;
            };
            let held = self.held.0[index];
            if !held && !released {
                continue;
            }
            // Square on screen: the 640-wide layout is kept round the centre.
            let rect = Rect::new(
                w * 0.5 + (x + column * edge - 320.0) * s + s,
                (y + row * edge) * s + s,
                (edge - 2.0) * s,
                (edge - 2.0) * s,
            );
            let fill = if held {
                accent
            } else {
                Color::new(0.0, 0.0, 0.0, 0.35)
            };
            let _ = list.push(DrawCommand::RoundedRect {
                rect,
                radius: 2.0 * s,
                color: fill,
            });
            if !held {
                let _ = list.push(DrawCommand::Border {
                    rect,
                    radius: 2.0 * s,
                    width: s,
                    color: Color::new(1.0, 1.0, 1.0, 0.3),
                });
            }
            if labels {
                let _ = list.push(DrawCommand::Text {
                    rect,
                    text: TextId(FIRST_TEXT + index as u32),
                    size: (edge - 4.0) * 0.75 * s,
                    color: if held {
                        Color::new(0.0, 0.0, 0.0, 0.9)
                    } else {
                        Color::new(1.0, 1.0, 1.0, 0.55)
                    },
                    align: TextAlign::Center,
                    overflow: TextOverflow::Clip,
                    weight: FontWeight::Regular,
                    letter_spacing: 0.0,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_holds_its_keys() {
        let command = UserCommand {
            forward_move: 127,
            right_move: -127,
            up_move: 127,
            buttons: BUTTON_ALT_ATTACK | BUTTON_WALKING,
            ..UserCommand::default()
        };
        assert_eq!(
            Held::from_command(&command).0,
            [true, true, false, true, false, false, false, true, true]
        );
    }
}
