//! jaPRO's lead indicator (`cg_leadIndicator`; EternalJK `cg_draw.c:11182-11304`, with
//! `CG_GetBulletSpeed`/`CG_GetBulletGravity` and `CG_DottedLine`): for the projectile weapon
//! in hand, where to aim at each moving player in sight for the shot to meet them — ten red
//! dots from where the player is drawn towards that point and a small red square up and
//! left of it. A value of 1 allows for the client's ping, any other value is a ping of its
//! own in milliseconds. A jaPRO server can forbid it (`restricts` bit 2).

use super::*;
use jkr_bsp::{Aabb, Bsp, TraceScratch};
use jkr_ui::{Color, DrawCommand, DrawList, Rect};

/// The serverinfo's `restricts` bit that forbids the indicator (`RESTRICT_LEAD`).
const RESTRICT_LEAD: i32 = 1 << 2;
/// The `jcinfo` bit that makes the disruptor a projectile (`JAPRO_CINFO_PROJSNIPER`).
const JAPRO_CINFO_PROJSNIPER: i32 = 1 << 29;
/// What a falling shot is reckoned to drop by: `0.5 * 800 * eta²`.
const GRAVITY: f32 = 800.0;
/// `CG_DottedLine`'s dots, each a unit of the 480-line screen.
const DOTS: usize = 10;
/// `EF_DEAD`.
const EF_DEAD: u32 = 1 << 1;

/// One player's line, in pixels: where it is drawn, and where to aim; the square only
/// where the aim point is on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mark {
    from: [f32; 2],
    to: [f32; 2],
    square: bool,
}

/// This frame's lines and the size of a unit of the 480-line screen.
#[derive(Default)]
pub(super) struct State {
    marks: Vec<Mark>,
    unit: f32,
}

/// What one frame of the indicator reads.
pub(crate) struct LeadInputs<'a> {
    pub(crate) snapshot: &'a jkr_protocol::Snapshot,
    pub(crate) game: &'a GameState,
    /// Where entities are drawn (`cent->lerpOrigin`).
    pub(crate) world: Option<&'a jkr_runtime::World>,
    pub(crate) camera: identification::Camera,
    pub(crate) time: i64,
    pub(crate) bsp: &'a Bsp,
    pub(crate) scratch: &'a mut TraceScratch,
    /// `cg_leadIndicator`.
    pub(crate) setting: f32,
    /// The newest snapshot's ping (`cg.snap->ping`).
    pub(crate) ping: i32,
    /// Whether the command holds the attack and the alternate attack.
    pub(crate) buttons: (bool, bool),
    /// Where the local player is (`cg.predictedPlayerState.origin`).
    pub(crate) origin: glam::Vec3,
    /// The 2D screen is off (`cg_draw2D 0`), or the scores or an intermission cover it.
    pub(crate) hidden: bool,
}

/// `CG_GetBulletSpeed` and `CG_GetBulletGravity`: a weapon's shot speed and whether it
/// falls, for its primary or alternate fire; `None` for one that has no projectile.
pub(crate) fn shot(weapon: u8, alternate: bool, projectile_sniper: bool) -> Option<(f32, bool)> {
    let (speed, gravity) = match weapon {
        // WP_BLASTER: its gravity follows the disruptor's case it falls through to.
        5 => (2_300.0, projectile_sniper),
        // WP_DISRUPTOR, a projectile only on such a jaPRO server.
        6 => (if projectile_sniper { 10_000.0 } else { 0.0 }, projectile_sniper),
        // WP_BOWCASTER.
        7 => (1_300.0, false),
        // WP_BRYAR_PISTOL, WP_BRYAR_OLD.
        4 | 16 => (1_600.0, false),
        // WP_REPEATER.
        8 if alternate => (1_100.0, true),
        8 => (1_600.0, false),
        // WP_DEMP2.
        9 => (if alternate { 0.0 } else { 1_800.0 }, false),
        // WP_FLECHETTE.
        10 if alternate => (1_050.0, true),
        10 => (3_500.0, false),
        // WP_ROCKET_LAUNCHER.
        11 => (if alternate { 450.0 } else { 900.0 }, false),
        // WP_THERMAL, WP_TRIP_MINE.
        12 | 13 => (550.0, true),
        // WP_CONCUSSION.
        15 => (if alternate { 0.0 } else { 3_000.0 }, false),
        _ => (0.0, false),
    };
    (speed > 0.0).then_some((speed, gravity))
}

/// Where to aim at a player drawn at `target`, moving at `velocity`, from `origin`: the
/// shot's flight time over the distance, the player's velocity grown by the ping (in
/// seconds), and a falling shot's drop added above.
pub(crate) fn lead_point(
    origin: glam::Vec3,
    target: glam::Vec3,
    velocity: glam::Vec3,
    (speed, gravity): (f32, bool),
    ping: f32,
) -> glam::Vec3 {
    let eta = (target - origin).length() / speed;
    let mut start = target;
    if gravity {
        start.z += 0.5 * GRAVITY * eta * eta;
    }
    start + (velocity + velocity * ping) * eta
}

impl HudOverlay {
    /// One frame of the indicator (`CG_LeadIndicator`).
    pub(crate) fn update_lead_indicator(&mut self, input: LeadInputs<'_>) {
        let state = &mut self.lead_indicator;
        state.marks.clear();
        state.unit = input.camera.viewport[1] / 480.0;
        if input.setting == 0.0 || input.hidden {
            return;
        }
        let info = jkr_client::LegacyClientInfo::new(input.game.config_string(0).unwrap_or(b""));
        let player = &input.snapshot.player;
        if info.integer("restricts").unwrap_or(0) & RESTRICT_LEAD != 0
            || player.entity_flags() & EF_DEAD != 0
        {
            return;
        }
        let (primary, alternate) = input.buttons;
        let sniper = info.integer("jcinfo").unwrap_or(0) & JAPRO_CINFO_PROJSNIPER != 0;
        // Alternate fire unless only the primary is held, as jaPRO reckons it.
        let Some(shot) = shot(player.weapon(), !(primary && !alternate), sniper) else {
            return;
        };
        let ping = if input.setting as i32 == 1 {
            input.ping as f32 * 0.001
        } else {
            input.setting * 0.001
        }
        .max(0.0);
        let local = player.client_num();
        for entity in &input.snapshot.entities {
            let number = entity.number();
            if number >= 32
                || number == local
                || entity.entity_type() != 1
                || entity.e_flags() & EF_DEAD != 0
            {
                continue;
            }
            let velocity = glam::Vec3::from_array(entity.trajectory_delta());
            if velocity == glam::Vec3::ZERO && !shot.1 {
                continue;
            }
            let Some(drawn) = input
                .world
                .and_then(|world| world.entity(jkr_runtime::EntityId::new(u64::from(number) + 1)))
            else {
                continue;
            };
            let target = glam::Vec3::from_array(drawn.sample(input.time).translation);
            // In sight: nothing of the world between (a player in the way still counts).
            let trace = input.bsp.trace_box_with(
                input.scratch,
                input.origin.to_array(),
                target.to_array(),
                Aabb::POINT,
                1,
            );
            if trace.fraction < 1.0 {
                continue;
            }
            let lead = lead_point(input.origin, target, velocity, shot, ping);
            let (Some(from), Some(to)) = (
                input.camera.project(target),
                input.camera.project_ahead(lead),
            ) else {
                continue;
            };
            let viewport = input.camera.viewport;
            let square =
                (0.0..=viewport[0]).contains(&to[0]) && (0.0..=viewport[1]).contains(&to[1]);
            state.marks.push(Mark { from, to, square });
        }
    }
}

impl State {
    /// The dots, half seen, and the square.
    pub(super) fn emit(&self, list: &mut DrawList) {
        let unit = self.unit;
        for mark in &self.marks {
            for dot in 0..DOTS {
                let along = dot as f32 / DOTS as f32;
                let x = mark.from[0] + (mark.to[0] - mark.from[0]) * along - unit * 0.5;
                let y = mark.from[1] + (mark.to[1] - mark.from[1]) * along - unit * 0.5;
                let _ = list.push(DrawCommand::SolidRect {
                    rect: Rect::new(x, y, unit, unit),
                    color: Color::new(1.0, 0.0, 0.0, 0.5),
                });
            }
            if mark.square {
                let _ = list.push(DrawCommand::SolidRect {
                    rect: Rect::new(
                        mark.to[0] - 8.0 * unit,
                        mark.to[1] - 8.0 * unit,
                        4.0 * unit,
                        4.0 * unit,
                    ),
                    color: Color::new(1.0, 0.0, 0.0, 1.0),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn shot_speeds_and_drops_are_japros() {
        assert_eq!(shot(5, false, false), Some((2_300.0, false)));
        assert_eq!(shot(5, false, true), Some((2_300.0, true)));
        assert_eq!(shot(6, false, false), None);
        assert_eq!(shot(6, true, true), Some((10_000.0, true)));
        assert_eq!(shot(8, true, false), Some((1_100.0, true)));
        assert_eq!(shot(9, true, false), None);
        assert_eq!(shot(12, false, false), Some((550.0, true)));
        assert_eq!(shot(3, false, false), None);
    }

    #[test]
    fn the_lead_runs_ahead_of_a_moving_player_and_above_for_a_falling_shot() {
        let (origin, target) = (Vec3::ZERO, Vec3::new(1_600.0, 0.0, 0.0));
        let velocity = Vec3::new(0.0, 300.0, 0.0);
        // A second's flight at 1600: three hundred units across.
        let lead = lead_point(origin, target, velocity, (1_600.0, false), 0.0);
        assert!((lead - Vec3::new(1_600.0, 300.0, 0.0)).length() < 1e-3);
        // A tenth of a second's ping grows the velocity by a tenth.
        let lead = lead_point(origin, target, velocity, (1_600.0, false), 0.1);
        assert!((lead.y - 330.0).abs() < 1e-3);
        // Falling, the aim rises by half of 800 in that second.
        let lead = lead_point(origin, target, Vec3::ZERO, (1_600.0, true), 0.0);
        assert!((lead.z - 400.0).abs() < 1e-3);
    }
}
