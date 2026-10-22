//! Fighter brackets (`CG_DrawBracketedEntities`, `CG_BracketEntity`, `cg_draw.c:5427-5658`):
//! while this client pilots a fighter, every other vehicle and every entity the server
//! brackets (`EF2_BRACKET_ENTITY`) gets four corner marks, sized by its radius over its
//! distance — red for an enemy, green for a teammate, white for a vehicle nobody rides —
//! once it is 2,000 units off and in sight, or at any distance under the crosshair or the
//! rocket lock. With `cg_drawVehLeadIndicator` (on by default) a moving enemy vehicle also
//! shows where to aim, when the fighter's first weapon fires an unguided shot gravity does
//! not pull: a dotted line from it to where that shot would meet it, and a mark there.
//! Stock's mark is a picture the game's data lacks (`gfx/menus/radar/lead`, so stock draws
//! the default image tinted red); here it is a red ring.

use super::*;
use glam::Vec3;
use jkr_protocol::Snapshot;
use jkr_ui::{Color, DrawCommand, DrawList, Rect};

/// `ET_GENERAL`, `ET_NPC`, `CLASS_VEHICLE`, `EF2_BRACKET_ENTITY`.
const ET_GENERAL: u8 = 0;
const ET_NPC: u8 = 13;
const CLASS_VEHICLE: u8 = 53;
const EF2_BRACKET_ENTITY: u32 = 1 << 6;
/// Protocol-26 entity fields `teamowner`, `g2radius`, `eFlags2`.
const TEAM_OWNER: usize = 21;
const G2_RADIUS: usize = 38;
const FLAGS2: usize = 96;
/// `ps.rocketLockIndex`.
const PS_ROCKET_LOCK_INDEX: usize = 24;
const GT_TEAM: i32 = 6;
/// `CS_PLAYERS`, `MAX_CLIENTS`.
const CS_PLAYERS: usize = 1_131;
const MAX_CLIENTS: u16 = 32;
/// Closer than this, only a target under the crosshair or the lock is bracketed.
const NEAR: f32 = 2_000.0;
/// Room for the brackets of one frame.
const CAPACITY: usize = 64;
const RED: Color = Color::new(1.0, 0.0, 0.0, 1.0);

/// One bracketed entity, in pixels: its centre, its size, its colour, and the lead
/// point for a moving enemy vehicle.
#[derive(Clone, Copy)]
struct Bracket {
    center: [f32; 2],
    size: f32,
    color: Color,
    lead: Option<[f32; 2]>,
}

/// The fighter this client pilots.
#[derive(Clone, Copy)]
pub(crate) struct Fighter {
    /// Its entity number, which is never bracketed.
    pub(crate) number: u16,
    /// Where it is (`cg.predictedVehicleState.origin`).
    pub(crate) origin: Vec3,
    /// Its first weapon's shot speed, when that shot can be led (`bIsProjectile`, no
    /// `bHasGravity`, no `fHoming`).
    pub(crate) shot_speed: Option<f32>,
}

/// What one frame's brackets read.
pub(crate) struct BracketInputs<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) game: &'a GameState,
    pub(crate) camera: identification::Camera,
    pub(crate) fighter: Option<Fighter>,
    /// The entity under the crosshair (`cg.crosshairClientNum`).
    pub(crate) crosshair: Option<u16>,
    /// `cg_drawVehLeadIndicator`.
    pub(crate) lead: bool,
}

/// This frame's brackets.
pub(crate) struct Brackets {
    list: Vec<Bracket>,
}

impl Default for Brackets {
    fn default() -> Self {
        Self {
            list: Vec::with_capacity(CAPACITY),
        }
    }
}

/// The team (`t`) in a player's info, if it has one.
fn client_team(game: &GameState, client: u16) -> Option<i64> {
    let info = game
        .config_string(CS_PLAYERS + usize::from(client))
        .filter(|info| !info.is_empty())?;
    Some(jkr_game_jka::userinfo::atoi(jkr_protocol::info_value(info, b"t").unwrap_or(b"0")) as i64)
}

impl Brackets {
    /// The brackets for this frame: `origin` gives an entity's drawn origin
    /// (`lerpOrigin`), `clear` whether nothing opaque stands between two points.
    pub(crate) fn update(
        &mut self,
        input: BracketInputs<'_>,
        origin: &dyn Fn(u16) -> Option<Vec3>,
        clear: &mut dyn FnMut(Vec3, Vec3) -> bool,
    ) {
        self.list.clear();
        let Some(fighter) = input.fighter else {
            return;
        };
        let player = &input.snapshot.player;
        let lock = player.raw_field(PS_ROCKET_LOCK_INDEX).unwrap_or(1_023) as u16;
        let gametype = input
            .game
            .config_string(0)
            .and_then(|info| jkr_client::LegacyClientInfo::new(info).integer("g_gametype"))
            .unwrap_or(0);
        let own_team = client_team(input.game, player.client_num());
        let eye = input.camera.eye;
        for state in &input.snapshot.entities {
            if self.list.len() == CAPACITY {
                break;
            }
            let number = state.number();
            let vehicle = state.entity_type() == ET_NPC && state.npc_class() == CLASS_VEHICLE;
            let flagged = state.entity_type() == ET_GENERAL
                && state.raw_field(FLAGS2).unwrap_or(0) & EF2_BRACKET_ENTITY != 0;
            if !(vehicle && number != fighter.number) && !flagged {
                continue;
            }
            let Some(position) = origin(number) else {
                continue;
            };
            let distance = position.distance(eye);
            let targeted = input.crosshair == Some(number) || lock == number;
            if !targeted && (distance < NEAR || !clear(eye, position)) {
                continue;
            }
            let Some(center) = input.camera.project_ahead(position) else {
                continue;
            };
            // The pilot's team, else the entity's own team, says friend or foe.
            let pilot = state.vehicle_entity_num();
            let owner = state.raw_field(TEAM_OWNER).unwrap_or(0) as i64;
            let (enemy, color) = if pilot != 0
                && pilot - 1 < MAX_CLIENTS
                && let Some(team) = client_team(input.game, pilot - 1)
            {
                if gametype < GT_TEAM || Some(team) != own_team {
                    (true, RED)
                } else {
                    (false, Color::new(0.0, 1.0, 0.0, 1.0))
                }
            } else if owner != 0 {
                if gametype < GT_TEAM || owner != i64::from(player.team()) {
                    (true, RED)
                } else {
                    (false, Color::new(0.0, 1.0, 0.0, 1.0))
                }
            } else {
                (false, Color::new(1.0, 1.0, 1.0, 1.0))
            };
            let radius = match state.raw_field(G2_RADIUS).unwrap_or(0) {
                0 => 64.0,
                radius => radius as f32,
            };
            let size = if distance <= 1.0 {
                radius * 400.0
            } else {
                radius * 400.0 / distance
            }
            .max(1.0);
            let velocity = Vec3::from_array(state.trajectory_delta());
            let lead = (input.lead && enemy && vehicle && velocity != Vec3::ZERO)
                .then_some(fighter.shot_speed)
                .flatten()
                .and_then(|speed| {
                    // How long the shot takes to reach it, and where it will be then.
                    let eta = position.distance(fighter.origin) / speed;
                    input.camera.project_ahead(position + velocity * eta)
                });
            self.list.push(Bracket {
                center,
                size,
                color,
                lead,
            });
        }
    }

    /// Draw them: each corner a line a tenth of the size long (half a unit at least) and a
    /// unit wide, the lead's dotted line in ten half-transparent dots and its ring.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2]) {
        let s = viewport[1] / 480.0;
        for bracket in &self.list {
            let size = bracket.size * s;
            let length = (bracket.size * 0.1).max(0.5) * s;
            let width = s;
            let [cx, cy] = bracket.center;
            let (x, y) = (cx - size * 0.5, cy - size * 0.5);
            for [rx, ry, rw, rh] in [
                [x, y, length, width],
                [x, y, width, length],
                [x + size - length, y, length, width],
                [x + size - width, y, width, length],
                [x, y + size - width, length, width],
                [x, y + size - length, width, length],
                [x + size - length, y + size - width, length, width],
                [x + size - width, y + size - length, width, length],
            ] {
                let _ = list.push(DrawCommand::SolidRect {
                    rect: Rect::new(rx, ry, rw, rh),
                    color: bracket.color,
                });
            }
            let Some([lx, ly]) = bracket.lead else {
                continue;
            };
            for dot in 0..10 {
                let t = dot as f32 / 10.0;
                let _ = list.push(DrawCommand::SolidRect {
                    rect: Rect::new(
                        cx + (lx - cx) * t - 0.5 * s,
                        cy + (ly - cy) * t - 0.5 * s,
                        s,
                        s,
                    ),
                    color: Color::new(1.0, 0.0, 0.0, 0.5),
                });
            }
            let _ = list.push(DrawCommand::Border {
                rect: Rect::new(lx - 8.0 * s, ly - 8.0 * s, 16.0 * s, 16.0 * s),
                radius: 8.0 * s,
                width: 1.5 * s,
                color: RED,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState};

    fn entity(number: u16, kind: u8, fields: &[(usize, u32)]) -> EntityState {
        let mut state = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(8, u32::from(kind));
        for &(index, value) in fields {
            state.set_raw_field(index, value);
        }
        state
    }

    #[test]
    fn a_fighter_brackets_far_vehicles_in_sight_and_leads_moving_enemies() {
        let vehicle = |number, pilot: u32| {
            entity(
                number,
                ET_NPC,
                &[
                    (88, u32::from(CLASS_VEHICLE)),
                    (93, pilot),
                    (7, 100.0_f32.to_bits()),
                ],
            )
        };
        let entities = vec![
            vehicle(40, 0),                                          // our own fighter
            vehicle(41, 2),                                          // far, ridden by client 1
            vehicle(42, 0),                                          // near
            entity(43, ET_GENERAL, &[(FLAGS2, EF2_BRACKET_ENTITY)]), // flagged
            vehicle(44, 0),                                          // far but hidden
        ];
        let at = |number: u16| match number {
            40 => Some(Vec3::ZERO),
            41 => Some(Vec3::new(3_000.0, 0.0, 0.0)),
            42 => Some(Vec3::new(500.0, 0.0, 0.0)),
            43 => Some(Vec3::new(2_500.0, 300.0, 0.0)),
            44 => Some(Vec3::new(3_000.0, -500.0, 0.0)),
            _ => None,
        };
        let mut game = jkr_protocol::GameState::empty_local(0);
        game.replace_config_string(0, b"\\g_gametype\\0".to_vec())
            .unwrap();
        game.replace_config_string(CS_PLAYERS + 1, b"\\n\\pilot\\t\\0".to_vec())
            .unwrap();
        let snapshot = Snapshot {
            message_sequence: 0,
            reliable_acknowledge: 0,
            server_commands: Vec::new(),
            server_time: 0,
            delta_from: None,
            flags: 0,
            area_mask: Vec::new(),
            player: PlayerState::zero(),
            vehicle_player: None,
            entities,
            consumed_bits: 0,
        };
        let camera = identification::Camera {
            eye: Vec3::ZERO,
            target: Vec3::X,
            up: Vec3::Z,
            fov: 90.0,
            viewport: [640.0, 480.0],
        };
        let mut brackets = Brackets::default();
        let fighter = Fighter {
            number: 40,
            origin: Vec3::ZERO,
            shot_speed: Some(1_000.0),
        };
        brackets.update(
            BracketInputs {
                snapshot: &snapshot,
                game: &game,
                camera,
                fighter: Some(fighter),
                crosshair: None,
                lead: true,
            },
            &at,
            &mut |_, to: Vec3| to.y > -100.0,
        );
        let colors: Vec<_> = brackets
            .list
            .iter()
            .map(|b| (b.color, b.lead.is_some()))
            .collect();
        assert_eq!(
            colors,
            [(RED, true), (Color::new(1.0, 1.0, 1.0, 1.0), false)]
        );
        // Radius 64 at 3,000 units.
        assert!((brackets.list[0].size - 64.0 * 400.0 / 3_000.0).abs() < 1e-3);
        // Not in a fighter, nothing.
        brackets.update(
            BracketInputs {
                snapshot: &snapshot,
                game: &game,
                camera,
                fighter: None,
                crosshair: None,
                lead: true,
            },
            &at,
            &mut |_, _| true,
        );
        assert!(brackets.list.is_empty());
    }
}
