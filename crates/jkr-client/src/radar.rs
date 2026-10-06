//! The radar of team games and vehicles (`CG_DrawRadar`, `cg_draw.c:3243-3796`): every mark
//! it draws in a frame, placed in its box of `2 × RADAR_RADIUS` units on the 640×480 screen,
//! with the viewer at the centre looking up.
//!
//! Its entities are the ones `CG_AddRadarEnt` collects while the packet entities are
//! added, in snapshot order: teammates in a team game (`CG_Player`), vehicles (`ET_NPC` of
//! `CLASS_VEHICLE`), and whatever is flagged `EF_RADAROBJECT` (`CG_General`, `CG_Missile`,
//! `CG_Mover`). They are drawn last to first, so the first collected ends on top.
//!
//! Left out: asteroid warnings (`ET_MOVER` in a vehicle: no stock map has an asteroid
//! field) and the rocket-lock alarms, which are sounds.

use crate::LegacyClientInfo;
use jkr_protocol::{EntityState, GameState};

/// `RADAR_RADIUS`: the radar box is twice this, in 640×480 screen units.
pub const RADAR_RADIUS: f32 = 60.0;
/// `cg_radarRange`.
const RADAR_RANGE: f32 = 2_500.0;
const ET_GENERAL: u8 = 0;
const ET_PLAYER: u8 = 1;
const ET_MISSILE: u8 = 3;
const ET_MOVER: u8 = 6;
const ET_NPC: u8 = 13;
/// `CLASS_VEHICLE` in `class_t`.
const CLASS_VEHICLE: u8 = 53;
/// `EF_RADAROBJECT`.
const EF_RADAROBJECT: u32 = 1 << 2;
const MAX_CLIENTS: u16 = 32;
const MAX_ICONS: i32 = 64;
const GT_TEAM: i32 = 6;
const TEAM_SPECTATOR: u8 = 3;
const CS_PLAYERS: usize = 1_131;
/// `CS_SIEGE_OBJECTIVES` (`CS_SIEGE_STATE + 1`).
const CS_SIEGE_OBJECTIVES: usize = 37 + 256 + 1;
const WHITE: [f32; 4] = [1.0; 4];
/// `g_color_table[COLOR_GREEN]`: teammates are always green.
const TEAM_COLOR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
/// `g_color_table[COLOR_RED]`.
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// One entity as the radar reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RadarEntity {
    pub number: u16,
    /// `eType`.
    pub kind: u8,
    /// `eFlags`.
    pub flags: u32,
    /// `NPC_class`.
    pub npc_class: u8,
    /// `speed`; a vehicle's own speed, a mover's size.
    pub speed: f32,
    /// `m_iVehicleNum`: a vehicle's driver, client number + 1.
    pub vehicle: u16,
    /// `brokenLimbs`: an objective's team.
    pub objective_team: u8,
    /// `frame`: an objective's number.
    pub objective: i32,
    /// `genericenemyindex`: its `CS_ICONS` icon.
    pub icon: i32,
    /// `time2`: when it was last hurt (its icon pulses for five seconds).
    pub pain_time: i32,
    pub owner: u16,
    /// `lerpOrigin`.
    pub origin: [f32; 3],
    /// `lerpAngles[YAW]`, in degrees.
    pub yaw: f32,
}

impl RadarEntity {
    /// An entity's radar fields, at its presented origin and yaw.
    pub fn from_state(state: &EntityState, origin: [f32; 3], yaw: f32) -> Self {
        Self {
            number: state.number(),
            kind: state.entity_type(),
            flags: state.e_flags(),
            npc_class: state.npc_class(),
            speed: state.speed(),
            vehicle: state.vehicle_entity_num(),
            objective_team: state.broken_limbs(),
            objective: state.raw_field(83).unwrap_or(0) as i32,
            icon: state.raw_field(18).unwrap_or(0) as i32,
            pain_time: state.raw_field(61).unwrap_or(0) as i32,
            owner: state.raw_field(40).unwrap_or(0) as u16,
            origin,
            yaw,
        }
    }
}

/// The viewer the radar is drawn for (`cg.predictedPlayerState`).
#[derive(Clone, Copy, Debug)]
pub struct RadarViewer {
    pub client: u16,
    pub origin: [f32; 3],
    /// `viewangles[PITCH]` and `[YAW]`, in degrees.
    pub pitch: f32,
    pub yaw: f32,
    /// `persistant[PERS_TEAM]`.
    pub team: u8,
    pub health: i32,
    /// `PMF_FOLLOW`: watching someone else.
    pub following: bool,
    /// `m_iVehicleNum`: riding a vehicle.
    pub in_vehicle: bool,
}

/// What a mark shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadarIcon {
    /// `mAutomapPlayerIcon` (`gfx/menus/radar/arrow_w`), turned by `angle`.
    Arrow,
    /// `cgs.gameIcons[index]`: the `CS_ICONS` shader at this index.
    Game(i32),
    /// `siegeItemShader` (`gfx/menus/radar/goalitem`).
    GoalItem,
    /// `mAutomapRocketIcon` (`gfx/menus/radar/rocket`).
    Rocket,
    /// The `radarIcon` of the vehicle this entity is.
    Vehicle(u16),
}

/// One drawn mark, in radar-box units from its top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadarMark {
    pub icon: RadarIcon,
    /// The picture's top-left corner, or its centre when `angle` is set.
    pub x: f32,
    pub y: f32,
    pub size: f32,
    /// `CG_DrawRotatePic2`'s angle in degrees, for arrows; `None` for plain pictures.
    pub angle: Option<f32>,
    pub color: [f32; 4],
}

/// The marks of the latest frame, kept between frames.
#[derive(Debug, Default)]
pub struct Radar {
    entities: Vec<RadarEntity>,
    marks: Vec<RadarMark>,
}

impl Radar {
    /// Whether `CG_DrawUpperRight` draws a radar for `viewer` at all: in a team game or a
    /// vehicle, and, inside `CG_DrawRadar`, only for a living player in the game.
    pub fn shown(viewer: &RadarViewer, game: &GameState) -> bool {
        let gametype = game
            .config_string(0)
            .and_then(|info| LegacyClientInfo::new(info).integer("g_gametype"))
            .unwrap_or(0);
        (gametype >= GT_TEAM || viewer.in_vehicle)
            && viewer.health > 0
            && !viewer.following
            && viewer.team != TEAM_SPECTATOR
            && client_team(game, viewer.client).is_some()
    }

    /// `CG_AddRadarEnt`'s choices among `entities`, in snapshot order, then the marks.
    /// `voice_time` is each client's `vChatTime` (zero when it never spoke).
    pub fn build(
        &mut self,
        viewer: &RadarViewer,
        game: &GameState,
        time: i32,
        entities: impl IntoIterator<Item = RadarEntity>,
        voice_time: impl Fn(u16) -> i32,
    ) -> &[RadarMark] {
        self.entities.clear();
        self.marks.clear();
        let team_game = game
            .config_string(0)
            .and_then(|info| LegacyClientInfo::new(info).integer("g_gametype"))
            .unwrap_or(0)
            >= GT_TEAM;
        for entity in entities {
            if collected(&entity, viewer, game, team_game) {
                self.entities.push(entity);
            }
        }
        for index in (0..self.entities.len()).rev() {
            let entity = self.entities[index];
            if let Some(mark) = self.mark(&entity, viewer, game, time, &voice_time) {
                self.marks.push(mark);
            }
        }
        // The viewer's own arrow, white and straight up, at the centre.
        self.marks.push(RadarMark {
            icon: RadarIcon::Arrow,
            x: RADAR_RADIUS,
            y: RADAR_RADIUS,
            size: 16.0 * RADAR_RADIUS / 128.0,
            angle: Some(0.0),
            color: WHITE,
        });
        &self.marks
    }

    /// One radar entity's mark (`CG_DrawRadar`'s loop body).
    fn mark(
        &self,
        entity: &RadarEntity,
        viewer: &RadarViewer,
        game: &GameState,
        time: i32,
        voice_time: &impl Fn(u16) -> i32,
    ) -> Option<RadarMark> {
        let away = [
            viewer.origin[0] - entity.origin[0],
            viewer.origin[1] - entity.origin[1],
        ];
        let mut distance = (away[0] * away[0] + away[1] * away[1]).sqrt();
        let to_player = if distance > 0.0 {
            [away[0] / distance, away[1] / distance]
        } else {
            [0.0, 0.0]
        };
        let moving_vehicle =
            entity.kind == ET_NPC && entity.npc_class == CLASS_VEHICLE && entity.speed > 0.0;
        if distance > RADAR_RANGE * 0.8 {
            if entity.flags & EF_RADAROBJECT != 0 || moving_vehicle {
                distance = RADAR_RANGE * 0.8;
            } else {
                return None;
            }
        }
        let distance = distance / RADAR_RANGE * RADAR_RADIUS;
        let look = flat_forward(viewer.pitch, viewer.yaw);
        let angle = look[0].atan2(look[1]) - to_player[0].atan2(to_player[1]);
        let (sin, cos) = (angle.sin(), angle.cos());
        let (x, y) = (RADAR_RADIUS + sin * distance, RADAR_RADIUS + cos * distance);
        let rise = entity.origin[2] - viewer.origin[2];
        match entity.kind {
            ET_NPC => {
                if !moving_vehicle {
                    return None;
                }
                let size = 9.0 * height_scale(rise, 4_096.0, 0.5);
                let color = driver_color(game, viewer, entity.vehicle);
                Some(RadarMark {
                    icon: RadarIcon::Vehicle(entity.number),
                    x: x - 4.0,
                    y: y - 4.0,
                    size,
                    angle: None,
                    color,
                })
            }
            // Asteroids only (left out, see the module documentation).
            ET_MOVER => None,
            ET_MISSILE => {
                let owner = self.owner(entity.owner)?;
                if entity.owner <= MAX_CLIENTS || owner.npc_class != CLASS_VEHICLE {
                    return None;
                }
                let color = if owner.vehicle <= MAX_CLIENTS {
                    driver_color(game, viewer, owner.vehicle)
                } else {
                    WHITE
                };
                Some(RadarMark {
                    icon: RadarIcon::Rocket,
                    x: x - 4.0,
                    y: y - 4.0,
                    size: 3.0 * height_scale(rise, 1_024.0, 0.5),
                    angle: None,
                    color,
                })
            }
            ET_PLAYER => {
                client_team(game, entity.number)?;
                let mut color = TEAM_COLOR;
                let mut base = 16.0;
                // A teammate's arrow swells and whitens after a voice command.
                let spoke = voice_time(entity.number);
                if spoke != 0 && spoke.wrapping_add(2_000) > time {
                    let fade = spoke.wrapping_add(2_000).wrapping_sub(time) as f32 / 3_000.0;
                    base += 4.0 * fade;
                    for channel in 0..3 {
                        color[channel] += (1.0 - color[channel]) * fade;
                    }
                }
                let scale = if rise > 0.0 {
                    1.0 + (rise / 1_024.0).min(1.0)
                } else {
                    height_scale(rise, 1_024.0, 0.5)
                };
                Some(RadarMark {
                    icon: RadarIcon::Arrow,
                    x,
                    y,
                    size: base * scale * RADAR_RADIUS / 128.0,
                    angle: Some((360.0 - entity.yaw) + viewer.yaw),
                    color,
                })
            }
            _ => {
                let size = 9.0 * height_scale(rise, 1_024.0, 0.5);
                let (icon, mut color) = if entity.objective_team != 0 {
                    // An objective's mark goes once it is complete.
                    if objective_complete(game, entity.objective_team, entity.objective) {
                        return None;
                    }
                    if entity.icon != 0 && entity.icon < MAX_ICONS {
                        (RadarIcon::Game(entity.icon), WHITE)
                    } else if entity.objective_team == viewer.team {
                        // `VectorCopy` sets no alpha (stock leaves it unset); opaque here.
                        (RadarIcon::GoalItem, RED)
                    } else {
                        (RadarIcon::GoalItem, TEAM_COLOR)
                    }
                } else if entity.icon != 0 {
                    (RadarIcon::Game(entity.icon), WHITE)
                } else {
                    (RadarIcon::GoalItem, WHITE)
                };
                if (entity.pain_time != 0 && time.wrapping_sub(entity.pain_time) < 5_000)
                    || entity.pain_time == -1
                {
                    color[3] = pulse(time);
                }
                Some(RadarMark {
                    icon,
                    x: x - 4.0,
                    y: y - 4.0,
                    size,
                    angle: None,
                    color,
                })
            }
        }
    }

    /// The collected entity numbered `number`, the way `cg_entities[owner]` is read.
    fn owner(&self, number: u16) -> Option<&RadarEntity> {
        self.entities.iter().find(|entity| entity.number == number)
    }
}

/// Whether `CG_AddPacketEntities` hands `entity` to `CG_AddRadarEnt`.
fn collected(
    entity: &RadarEntity,
    viewer: &RadarViewer,
    game: &GameState,
    team_game: bool,
) -> bool {
    match entity.kind {
        // `CG_Player`: teammates in a team game; NPCs only as vehicles.
        ET_PLAYER => {
            team_game
                && entity.number != viewer.client
                && client_team(game, entity.number) == Some(viewer.team)
        }
        ET_NPC => entity.npc_class == CLASS_VEHICLE,
        ET_GENERAL | ET_MISSILE | ET_MOVER => entity.flags & EF_RADAROBJECT != 0,
        _ => false,
    }
}

/// `AngleVectors`' forward with its height dropped and made unit length again.
fn flat_forward(pitch: f32, yaw: f32) -> [f32; 2] {
    let (pitch, yaw) = (pitch.to_radians(), yaw.to_radians());
    let forward = [pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin()];
    let length = (forward[0] * forward[0] + forward[1] * forward[1]).sqrt();
    if length > 0.0 {
        [forward[0] / length, forward[1] / length]
    } else {
        [0.0, 0.0]
    }
}

/// The size a mark takes for `rise` units above (or below) the viewer: up to `limit`
/// larger or smaller, reached at `span / 2` units.
fn height_scale(rise: f32, span: f32, limit: f32) -> f32 {
    if rise > 0.0 {
        1.0 + (rise / span).min(limit)
    } else if rise < 0.0 {
        1.0 - (-rise / span).min(limit)
    } else {
        1.0
    }
}

/// The alpha of a hurt entity's mark: down and up every 200 ms.
fn pulse(time: i32) -> f32 {
    let phase = time.rem_euclid(200) as f32 / 200.0;
    if (time / 200) & 1 != 0 {
        0.1 + 0.9 * phase
    } else {
        1.0 - 0.9 * phase
    }
}

/// Green for a vehicle (or its rocket) driven by a teammate, red for an enemy's, white
/// with no player driving.
fn driver_color(game: &GameState, viewer: &RadarViewer, vehicle: u16) -> [f32; 4] {
    let Some(driver) = vehicle.checked_sub(1) else {
        return WHITE;
    };
    let viewer_team = client_team(game, viewer.client);
    match client_team(game, driver) {
        Some(team) if Some(team) == viewer_team => TEAM_COLOR,
        Some(_) => RED,
        None => WHITE,
    }
}

/// A client's team from its `CS_PLAYERS` string; `None` while it has none (`infoValid`).
fn client_team(game: &GameState, client: u16) -> Option<u8> {
    let info = game.config_string(CS_PLAYERS + usize::from(client))?;
    if info.is_empty() {
        return None;
    }
    let team = LegacyClientInfo::new(info).integer("t").unwrap_or(0);
    u8::try_from(team).ok()
}

/// `team<side>_objective<number>` as `CG_ParseSiegeObjectiveStatus` sets it from
/// `CS_SIEGE_OBJECTIVES`: each `-` starts the next objective, `1` after it completes it,
/// and `|` moves on to team two.
fn objective_complete(game: &GameState, side: u8, objective: i32) -> bool {
    let Some(status) = game.config_string(CS_SIEGE_OBJECTIVES) else {
        return false;
    };
    let (mut team, mut number) = (1u8, 0i32);
    let mut bytes = status.iter().peekable();
    while let Some(&byte) = bytes.next() {
        match byte {
            b'|' => {
                team = 2;
                number = 0;
            }
            b'-' => {
                number += 1;
                let done = bytes.next() == Some(&b'1');
                if team == side && number == objective {
                    return done;
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(objectives: &[u8]) -> GameState {
        let mut game = GameState::empty_local(0);
        let mut set = |index: usize, value: &[u8]| {
            game.replace_config_string(index, value.to_vec())
                .expect("a valid config string");
        };
        set(0, b"\\g_gametype\\7");
        set(CS_PLAYERS, b"\\n\\viewer\\t\\1");
        set(CS_PLAYERS + 1, b"\\n\\mate\\t\\1");
        set(CS_PLAYERS + 2, b"\\n\\enemy\\t\\2");
        set(CS_SIEGE_OBJECTIVES, objectives);
        game
    }

    const VIEWER: RadarViewer = RadarViewer {
        client: 0,
        origin: [0.0; 3],
        pitch: 0.0,
        yaw: 90.0,
        team: 1,
        health: 100,
        following: false,
        in_vehicle: false,
    };

    fn player(number: u16, origin: [f32; 3]) -> RadarEntity {
        RadarEntity {
            number,
            kind: ET_PLAYER,
            origin,
            yaw: 90.0,
            ..RadarEntity::default()
        }
    }

    #[test]
    fn a_teammate_ahead_sits_above_the_centre() {
        let game = game(b"");
        let mut radar = Radar::default();
        // Looking along +Y; the teammate 1,250 units ahead, the enemy beside him.
        let marks = radar
            .build(
                &VIEWER,
                &game,
                10_000,
                [
                    player(1, [0.0, 1_250.0, 0.0]),
                    player(2, [10.0, 1_250.0, 0.0]),
                ],
                |_| 0,
            )
            .to_vec();
        assert_eq!(marks.len(), 2, "the teammate and the viewer's own arrow");
        let mate = marks[0];
        assert_eq!(mate.icon, RadarIcon::Arrow);
        assert!((mate.x - RADAR_RADIUS).abs() < 1e-3);
        assert!((mate.y - (RADAR_RADIUS - 30.0)).abs() < 1e-3);
        assert_eq!(mate.angle, Some(360.0));
        assert_eq!(mate.color, TEAM_COLOR);
        assert!((mate.size - 16.0 * RADAR_RADIUS / 128.0).abs() < 1e-5);
        assert_eq!(marks[1].angle, Some(0.0));
    }

    #[test]
    fn far_marks_drop_unless_they_are_objects_or_moving_vehicles() {
        let game = game(b"");
        let mut radar = Radar::default();
        let far = [3_000.0, 0.0, 0.0];
        let object = RadarEntity {
            number: 40,
            kind: ET_GENERAL,
            flags: EF_RADAROBJECT,
            origin: far,
            ..RadarEntity::default()
        };
        let parked = RadarEntity {
            number: 41,
            kind: ET_NPC,
            npc_class: CLASS_VEHICLE,
            origin: far,
            ..RadarEntity::default()
        };
        let marks = radar
            .build(&VIEWER, &game, 0, [player(1, far), object, parked], |_| 0)
            .to_vec();
        // The object is held at the edge (0.8 of the range); the teammate and the
        // parked vehicle are gone.
        assert_eq!(marks.len(), 2);
        assert_eq!(marks[0].icon, RadarIcon::GoalItem);
        let distance = 0.8 * RADAR_RADIUS;
        let centre = [
            marks[0].x + 4.0 - RADAR_RADIUS,
            marks[0].y + 4.0 - RADAR_RADIUS,
        ];
        assert!(((centre[0].powi(2) + centre[1].powi(2)).sqrt() - distance).abs() < 1e-3);
    }

    #[test]
    fn objectives_show_their_team_until_complete() {
        // Team one's objective 2 is done.
        let game = game(b"t1-0-1|t2-0");
        let mut radar = Radar::default();
        let objective = |number: u16, team: u8, objective: i32| RadarEntity {
            number,
            kind: ET_GENERAL,
            flags: EF_RADAROBJECT,
            objective_team: team,
            objective,
            origin: [100.0, 0.0, 0.0],
            ..RadarEntity::default()
        };
        let marks = radar
            .build(
                &VIEWER,
                &game,
                0,
                [
                    objective(40, 1, 1),
                    objective(41, 1, 2),
                    objective(42, 2, 1),
                ],
                |_| 0,
            )
            .to_vec();
        // Drawn last to first: team two's, then team one's first; the second is gone.
        let colors: Vec<_> = marks
            .iter()
            .filter(|mark| mark.icon == RadarIcon::GoalItem)
            .map(|mark| mark.color)
            .collect();
        assert_eq!(colors, [TEAM_COLOR, [1.0, 0.0, 0.0, 1.0]]);
    }

    #[test]
    fn a_hurt_object_pulses_and_a_speaker_swells() {
        let game = game(b"");
        let mut radar = Radar::default();
        let hurt = RadarEntity {
            number: 40,
            kind: ET_GENERAL,
            flags: EF_RADAROBJECT,
            icon: 5,
            pain_time: 9_000,
            origin: [100.0, 0.0, 0.0],
            ..RadarEntity::default()
        };
        let marks = radar
            .build(
                &VIEWER,
                &game,
                10_250,
                [hurt, player(1, [0.0, 100.0, 0.0])],
                |client| {
                    if client == 1 { 9_500 } else { 0 }
                },
            )
            .to_vec();
        // 10,250 ms: an odd 200 ms step, a quarter in: 0.1 + 0.9 × 0.25.
        let object = marks
            .iter()
            .find(|mark| mark.icon == RadarIcon::Game(5))
            .unwrap();
        assert!((object.color[3] - 0.325).abs() < 1e-5);
        // Spoke 750 ms ago: (2,000 - 750) / 3,000 of the way to white and 4 units larger.
        let mate = marks[0];
        let fade = 1_250.0 / 3_000.0;
        assert!((mate.color[0] - fade).abs() < 1e-5);
        assert!((mate.size - (16.0 + 4.0 * fade) * RADAR_RADIUS / 128.0).abs() < 1e-4);
    }

    #[test]
    fn the_radar_shows_for_a_living_player_in_a_team_game() {
        let game = game(b"");
        assert!(Radar::shown(&VIEWER, &game));
        let dead = RadarViewer {
            health: 0,
            ..VIEWER
        };
        assert!(!Radar::shown(&dead, &game));
        let watching = RadarViewer {
            following: true,
            ..VIEWER
        };
        assert!(!Radar::shown(&watching, &game));
    }
}
