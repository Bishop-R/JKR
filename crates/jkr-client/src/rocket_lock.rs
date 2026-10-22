//! The rocket launcher's lock as stock's client shows it (`CG_DrawRocketLocking`, OpenJK
//! `codemp/cgame/cg_draw.c:5763-5968`): from the moment the lock was taken, a red wedge a
//! sixteenth of 1.2 s apart (2.4 s in Siege) closes round the target, up to eight, with a
//! tick at each and the lock sound at the eighth; then the lock picture pulses over them.
//! Stock draws it twice a frame — `CG_Draw2DScreenTints` and `CG_Draw2D` both call it — so
//! its pictures land twice, while its sounds start once.
//!
//! Not here: a vehicle weapon's lock, with its own `lockOnTime` and the vehicle tick and lock
//! sounds. The server takes no vehicle lock yet (`jkr_game_jka::vehicle_weapons`).

/// The wedge and the lock picture (`R_RegisterShaderNoMip`).
pub const WEDGE: &str = "gfx/2d/wedge";
pub const LOCK: &str = "gfx/2d/lock";
/// The sounds, started at the target's origin.
pub const TICK_SOUND: &str = "sound/weapons/rocket/tick.wav";
pub const LOCK_SOUND: &str = "sound/weapons/rocket/lock.wav";
/// How many times a frame stock draws the lock.
pub const DRAWS: usize = 2;
/// `ENTITYNUM_NONE`, `ENTITYNUM_WORLD`, `GT_TEAM`, `TEAM_SPECTATOR`, `ET_NPC`,
/// `CLASS_VEHICLE`.
const ENTITY_NONE: u16 = 1_023;
const ENTITY_WORLD: u16 = 1_022;
const GT_TEAM: i32 = 6;
const TEAM_SPECTATOR: u8 = 3;
const ET_NPC: u8 = 13;
const CLASS_VEHICLE: u8 = 53;
/// The wedges a full lock shows.
const STEPS: i32 = 8;

/// What one frame's lock reads of the player's state and its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LockInput {
    /// `ps.rocketLockIndex`, `ps.rocketLockTime`.
    pub index: u16,
    pub time: f32,
    /// `cg.time`.
    pub now: i32,
    /// A Siege game, whose lock takes twice as long.
    pub siege: bool,
    /// The viewer spectates.
    pub spectator: bool,
    /// In a team game, the target is a teammate, or a vehicle a teammate pilots: no lock is
    /// shown on it (`pmove` cannot tell teams apart).
    pub teammate: bool,
}

/// One frame's lock: how many wedges, whether it holds, and the sound starting now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lock {
    pub steps: u8,
    pub locked: bool,
    pub sound: Option<&'static str>,
}

/// `CG_DrawRocketLocking`'s statics: the wedges last shown (`oldDif`) and the count the lock
/// had while it was taken (`lastvalidlockdif`), kept while the lock waits (`time` -1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LockState {
    shown: i32,
    last_valid: i32,
}

impl LockState {
    /// One frame of the lock; `on_screen` says whether the target is in front of the eye
    /// (`CG_WorldCoordToScreenCoord`). `None` shows nothing.
    pub fn frame(&mut self, input: &LockInput, on_screen: bool) -> Option<Lock> {
        // The callers' test, then the function's own.
        if input.index == ENTITY_NONE || input.now as f32 - input.time <= 0.0 {
            return None;
        }
        let interval = if input.siege { 2_400.0f32 } else { 1_200.0 } / 16.0;
        let mut steps = ((input.now as f32 - input.time) / interval) as i32;
        if input.time == 0.0 || input.spectator || input.teammate {
            return None;
        }
        if input.time != -1.0 {
            self.last_valid = steps;
        } else {
            steps = self.last_valid;
        }
        if !on_screen {
            return None;
        }
        if steps < 0 {
            self.shown = 0;
            return None;
        }
        let steps = steps.min(STEPS);
        let sound = (self.shown != steps).then_some(if steps == STEPS {
            LOCK_SOUND
        } else {
            TICK_SOUND
        });
        self.shown = steps;
        Some(Lock {
            steps: steps as u8,
            locked: steps == STEPS,
            sound,
        })
    }
}

/// Whether player `own` spectates, as its client info says (`cgs.clientinfo[ps.clientNum]`):
/// one following a player sees that player's lock.
pub fn spectating(game: &jkr_protocol::GameState, own: u16) -> bool {
    own < 32 && crate::radar::client_team(game, own) == Some(TEAM_SPECTATOR)
}

/// Whether the lock on `target` is hidden from player `own` as a teammate's
/// (`cg_draw.c:5822-5869`): in a team game, a player of its team, or a vehicle a player of
/// its team pilots. An NPC's client info has no team (`CG_CreateNPCClient` zeroes it), nor
/// a client's without its info.
pub fn teammate(
    game: &jkr_protocol::GameState,
    snapshot: &jkr_protocol::Snapshot,
    own: u16,
    target: u16,
) -> bool {
    let team_game = game
        .config_string(0)
        .and_then(|info| crate::LegacyClientInfo::new(info).integer("g_gametype"))
        .unwrap_or(0)
        >= GT_TEAM;
    let team_of = |client: u16| {
        if client < 32 {
            crate::radar::client_team(game, client).unwrap_or(0)
        } else {
            0
        }
    };
    let own_team = team_of(own);
    if team_of(target) == own_team {
        return team_game;
    }
    team_game
        && snapshot
            .entities
            .iter()
            .find(|entity| entity.number() == target)
            .is_some_and(|entity| {
                entity.entity_type() == ET_NPC
                    && entity.npc_class() == CLASS_VEHICLE
                    && entity.owner() < ENTITY_WORLD
                    && team_of(entity.owner()) == own_team
            })
}

/// The lock's edge in 640x480 units for a target `distance` units from the eye: 38 close by,
/// 6 from 1024 on.
pub fn edge(distance: f32) -> f32 {
    let far = (distance / 1_024.0).clamp(0.0, 1.0);
    (1.0 - far) * (1.0 - far) * 32.0 + 6.0
}

/// Wedge `step`'s alpha (`0.1f * i + 0.2f`): each wedge a tenth more solid than the last.
pub fn wedge_alpha(step: u8) -> f32 {
    0.1 * f32::from(step) + 0.2
}

/// The lock picture's grey at `now` (`sin(cg.time * 0.05) * 0.5 + 0.5`).
pub fn pulse(now: i32) -> f32 {
    (now as f32 * 0.05).sin() * 0.5 + 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(time: f32, now: i32) -> LockInput {
        LockInput {
            index: 40,
            time,
            now,
            siege: false,
            spectator: false,
            teammate: false,
        }
    }

    #[test]
    fn the_wedges_close_in_with_a_tick_each_then_the_lock_holds() {
        let mut lock = LockState::default();
        // 75 ms a wedge outside Siege.
        let first = lock.frame(&input(1_000.0, 1_080), true).unwrap();
        assert_eq!((first.steps, first.sound), (1, Some(TICK_SOUND)));
        // The second draw of the frame starts no sound.
        assert_eq!(
            lock.frame(&input(1_000.0, 1_080), true).unwrap().sound,
            None
        );
        let full = lock.frame(&input(1_000.0, 1_600), true).unwrap();
        assert_eq!(
            (full.steps, full.locked, full.sound),
            (8, true, Some(LOCK_SOUND))
        );
        // While the lock waits (`-1`), the last count stands.
        let waiting = lock.frame(&input(-1.0, 1_700), true).unwrap();
        assert_eq!((waiting.steps, waiting.sound), (8, None));
        // In Siege it takes twice as long.
        let mut siege = LockState::default();
        let half = siege
            .frame(
                &LockInput {
                    siege: true,
                    ..input(1_000.0, 1_600)
                },
                true,
            )
            .unwrap();
        assert_eq!(half.steps, 4);
    }

    #[test]
    fn no_lock_shows_off_screen_on_a_teammate_or_for_a_spectator() {
        let mut lock = LockState::default();
        assert_eq!(lock.frame(&input(1_000.0, 1_300), false), None);
        let teammate = LockInput {
            teammate: true,
            ..input(1_000.0, 1_300)
        };
        assert_eq!(lock.frame(&teammate, true), None);
        let spectator = LockInput {
            spectator: true,
            ..input(1_000.0, 1_300)
        };
        assert_eq!(lock.frame(&spectator, true), None);
        assert_eq!(lock.frame(&input(0.0, 1_300), true), None);
        assert_eq!(edge(0.0), 38.0);
        assert_eq!(edge(2_000.0), 6.0);
    }
}
