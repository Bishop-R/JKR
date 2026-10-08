//! What the game reads out of a map's entity dictionaries.
//!
//! The reference spawns every dictionary through `G_SpawnEntitiesFromString`
//! (`g_spawn.c`). This module starts with the entities a player can be placed at;
//! the rest of the spawn table follows with the systems that need it.
use jkr_entity::Entity;

/// Which players a spawn point is for, by the classnames of `g_spawn.c:522-533`
/// and `663-666`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpawnKind {
    /// `info_player_deathmatch`, and `info_player_start`, which the reference
    /// turns into one.
    Deathmatch,
    /// `info_player_duel`, `info_player_duel1`, `info_player_duel2`.
    Duel(u8),
    /// `info_player_intermission`, with `_red` and `_blue`: the view between maps
    /// and the place spectators start from.
    Intermission(Option<Team>),
    /// `info_player_start_red` / `_blue`, `team_CTF_*player` (first spawn of a
    /// team game) and `team_CTF_*spawn` (every later one).
    Team { team: Team, initial: bool },
    /// `info_player_siegeteam1` / `2`.
    Siege(u8),
}

/// The two sides of a team game.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Team {
    /// `TEAM_RED`.
    Red,
    /// `TEAM_BLUE`.
    Blue,
}

/// A place a player can be put, as the map's author placed it.
///
/// The origin is the entity's; the reference lifts and drops a player from there
/// when it actually spawns one (`ClientSpawn`), which is not done here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnPoint {
    /// Who may be placed here.
    pub kind: SpawnKind,
    /// The entity's `origin`.
    pub origin: [f32; 3],
    /// Pitch, yaw, roll in degrees.
    pub angles: [f32; 3],
}

/// The spawn points of a map, in the order of its entity lump.
pub fn spawn_points(entities: &[Entity]) -> Vec<SpawnPoint> {
    entities
        .iter()
        .filter_map(|entity| {
            let kind = match entity.classname()? {
                "info_player_deathmatch" | "info_player_start" => SpawnKind::Deathmatch,
                "info_player_duel" => SpawnKind::Duel(0),
                "info_player_duel1" => SpawnKind::Duel(1),
                "info_player_duel2" => SpawnKind::Duel(2),
                "info_player_intermission" => SpawnKind::Intermission(None),
                "info_player_intermission_red" => SpawnKind::Intermission(Some(Team::Red)),
                "info_player_intermission_blue" => SpawnKind::Intermission(Some(Team::Blue)),
                "info_player_start_red" | "team_CTF_redplayer" => SpawnKind::Team {
                    team: Team::Red,
                    initial: true,
                },
                "info_player_start_blue" | "team_CTF_blueplayer" => SpawnKind::Team {
                    team: Team::Blue,
                    initial: true,
                },
                "team_CTF_redspawn" => SpawnKind::Team {
                    team: Team::Red,
                    initial: false,
                },
                "team_CTF_bluespawn" => SpawnKind::Team {
                    team: Team::Blue,
                    initial: false,
                },
                "info_player_siegeteam1" => SpawnKind::Siege(1),
                "info_player_siegeteam2" => SpawnKind::Siege(2),
                _ => return None,
            };
            // A malformed vector reads as the reference's sscanf leaves it: zero.
            let origin = entity.vector("origin").ok().flatten().unwrap_or_default();
            // `angle` is a yaw (`F_ANGLEHACK`, g_spawn.c:845-850) and, coming later in
            // the field table than nothing else does, loses to an explicit `angles`.
            let mut angles = entity.vector("angles").ok().flatten().unwrap_or_else(|| {
                [
                    0.0,
                    entity.number("angle").ok().flatten().unwrap_or_default(),
                    0.0,
                ]
            });
            // `FindIntermissionPoint`: "if it has a target, look towards it" — an entity
            // the level keeps (`G_PickTarget`; an `info_null` is gone by then).
            if matches!(kind, SpawnKind::Intermission(_))
                && let Some(target) = intermission_target(entities, entity)
            {
                let direction = std::array::from_fn(|axis| target[axis] - origin[axis]);
                let (pitch, yaw) = crate::damage::vector_to_angles(direction);
                angles = [pitch, yaw, 0.0];
            }
            Some(SpawnPoint {
                kind,
                origin,
                angles,
            })
        })
        .collect()
}

/// `G_PickTarget(ent->target)` for an intermission point: where the first entity the
/// level keeps under that name stands (the reference draws among several with `rand`;
/// no stock map names two). `None` without a target, or with none kept.
fn intermission_target(entities: &[Entity], point: &Entity) -> Option<[f32; 3]> {
    let name = point.get("target").filter(|name| !name.is_empty())?;
    entities
        .iter()
        .filter(|entity| {
            entity
                .get("targetname")
                .is_some_and(|own| own.eq_ignore_ascii_case(name))
        })
        .find(|entity| {
            matches!(
                crate::spawn_table::support(entity),
                Some(
                    crate::spawn_table::Support::Ported
                        | crate::spawn_table::Support::Point
                        | crate::spawn_table::Support::Unported
                )
            )
        })
        .map(|entity| entity.vector("origin").ok().flatten().unwrap_or_default())
}

/// Where a client that is not playing looks at the map from: the neutral
/// intermission point if the map has one, else its first spawn point of any kind.
pub fn spectator_start(points: &[SpawnPoint]) -> Option<&SpawnPoint> {
    points
        .iter()
        .find(|point| point.kind == SpawnKind::Intermission(None))
        .or_else(|| points.first())
}

/// `SelectRandomFurthestSpawnPoint` (OpenJK `codemp/game/g_client.c:684-800`, through
/// `SelectSpawnPoint`): among the deathmatch points nobody stands on (`SpotWouldTelefrag`:
/// `occupied`), sorted by their distance from `avoid` — where the player died, or
/// spectated — furthest first (a point no further than one listed goes after it), one of
/// the further half is drawn with the game's generator. With every point taken, the
/// first deathmatch point of the map, whoever stands there. Returns where the player is
/// placed — nine units above the point — and how it faces.
pub fn select_spawn_point(
    rng: &mut crate::player_death::Rng,
    points: &[SpawnPoint],
    occupied: impl Fn(&SpawnPoint) -> bool,
    avoid: [f32; 3],
) -> Option<([f32; 3], [f32; 3])> {
    furthest_free(rng, points, SpawnKind::Deathmatch, &occupied, avoid)
        .or_else(|| first_deathmatch(points))
}

/// `SelectDuelSpawnPoint` (`g_client.c:803-894`) for a duel's `kind` of point
/// (`info_player_duel`, or a power duel's `duel1` for the lone and `duel2` for the pair):
/// the same choice among the free ones, then among the free deathmatch points, then the
/// first deathmatch point.
pub fn select_duel_spawn_point(
    rng: &mut crate::player_death::Rng,
    points: &[SpawnPoint],
    kind: SpawnKind,
    occupied: impl Fn(&SpawnPoint) -> bool,
    avoid: [f32; 3],
) -> Option<([f32; 3], [f32; 3])> {
    furthest_free(rng, points, kind, &occupied, avoid)
        .or_else(|| furthest_free(rng, points, SpawnKind::Deathmatch, &occupied, avoid))
        .or_else(|| first_deathmatch(points))
}

/// A random one of the furthest half of `kind`'s points nobody stands on, from `avoid`,
/// raised nine units; `None` where every one is taken.
fn furthest_free(
    rng: &mut crate::player_death::Rng,
    points: &[SpawnPoint],
    kind: SpawnKind,
    occupied: &dyn Fn(&SpawnPoint) -> bool,
    avoid: [f32; 3],
) -> Option<([f32; 3], [f32; 3])> {
    const MAX_SPAWN_POINTS: usize = 128;
    let mut listed: Vec<(f32, &SpawnPoint)> = Vec::with_capacity(MAX_SPAWN_POINTS);
    for point in points.iter().filter(|point| point.kind == kind) {
        if occupied(point) {
            continue;
        }
        let distance = (0..3)
            .map(|axis| (point.origin[axis] - avoid[axis]).powi(2))
            .sum::<f32>()
            .sqrt();
        match listed.iter().position(|(listed, _)| distance > *listed) {
            Some(at) => {
                listed.truncate(MAX_SPAWN_POINTS - 1);
                listed.insert(at, (distance, point));
            }
            None if listed.len() < MAX_SPAWN_POINTS => listed.push((distance, point)),
            None => {}
        }
    }
    if listed.is_empty() {
        return None;
    }
    // `rnd = Q_flrand(0.0f, 1.0f) * (numSpots / 2)`: the float product truncated.
    let rnd = (rng.flrand(0.0, 1.0) * (listed.len() / 2) as f32) as usize;
    let chosen = listed[rnd].1;
    Some((
        [chosen.origin[0], chosen.origin[1], chosen.origin[2] + 9.0],
        chosen.angles,
    ))
}

/// Every point taken: the first deathmatch point, raised nine units.
fn first_deathmatch(points: &[SpawnPoint]) -> Option<([f32; 3], [f32; 3])> {
    let chosen = points
        .iter()
        .find(|point| point.kind == SpawnKind::Deathmatch)?;
    Some((
        [chosen.origin[0], chosen.origin[1], chosen.origin[2] + 9.0],
        chosen.angles,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entities(text: &str) -> Vec<Entity> {
        jkr_entity::parse_entity_lump(text.as_bytes()).unwrap()
    }

    /// `FindIntermissionPoint`: a camera with a target the level keeps looks at it — the
    /// stock maps' `info_notnull` "v1" — and one whose target is gone (an `info_null`,
    /// freed at spawn) keeps its own angles.
    #[test]
    fn an_intermission_camera_looks_at_its_target() {
        let aimed = entities(
            "{\n\"classname\" \"info_player_intermission\"\n\"origin\" \"0 0 0\"\n\"target\" \"v1\"\n}\n\
             {\n\"classname\" \"info_notnull\"\n\"targetname\" \"v1\"\n\"origin\" \"100 100 0\"\n}",
        );
        let points = spawn_points(&aimed);
        assert!((points[0].angles[1] - 45.0).abs() < 0.001);
        assert_eq!(points[0].angles[0], 0.0);
        let gone = entities(
            "{\n\"classname\" \"info_player_intermission\"\n\"origin\" \"0 0 0\"\n\"angle\" \"90\"\n\"target\" \"t1\"\n}\n\
             {\n\"classname\" \"info_null\"\n\"targetname\" \"t1\"\n\"origin\" \"100 0 0\"\n}",
        );
        assert_eq!(spawn_points(&gone)[0].angles, [0.0, 90.0, 0.0]);
    }
}
