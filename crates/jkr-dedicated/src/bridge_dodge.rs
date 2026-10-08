//! `Jedi_DodgeEvasion` for a player a shot strikes ([`jkr_game_jka::force_dodge`]): the
//! decision, then the dodge on the player — the pose held 300 ms, a 100 ms speed burst, and
//! the Force speed sound or, with `g_forceDodge 2`, `ForceSpeed(self, 500)`.

use super::*;
use jkr_game_jka::force_dodge::{Dodger, SPEED_SOUND, SPEED_SOUND_CHANNEL, dodge_evasion, take};
use jkr_game_jka::force_powers::{FP_SEE, FP_SPEED};

/// `ENTITYNUM_NONE`.
const ENTITY_NONE: u16 = 1_023;

impl NativeGame {
    /// Whether the player at `number` dodges a shot striking it at `hit_location` (`HL_*`),
    /// the dodge taken on it. Anything but a client in the game never dodges.
    pub(super) fn shot_dodge(&mut self, number: u16, hit_location: i32, level_time: i32) -> bool {
        let mode = self.integer_cvar(b"g_forceDodge", 1);
        let (gametype, force_rules) = (self.gametype, self.force_rules);
        let Some(peer) = self.peer(usize::from(number)) else {
            return false;
        };
        if !peer.begun || !peer.playing() {
            return false;
        }
        let dodger = Dodger {
            alive: peer.health > 0,
            active_powers: peer.state.force_powers_active(),
            on_ground: peer.state.ground_entity_num() != ENTITY_NONE,
            weapon_time: peer.state.weapon_time(),
            hand_extend: u32::from(peer.state.force_hand_extend()),
            sight_level: i32::from(peer.force.levels[FP_SEE]),
            speed_level: i32::from(peer.force.levels[FP_SPEED]),
            // Only read with `g_forceDodge 2`; it asks nothing of the world.
            speed_usable: mode == 2
                && jkr_game_jka::force_powers::usable(
                    &peer.state,
                    &peer.force,
                    peer.health,
                    FP_SPEED,
                    level_time,
                    gametype,
                    force_rules,
                ),
        };
        let rng = &mut self.deaths.rng;
        let Some(dodge) = dodge_evasion(&dodger, mode, hit_location, &mut |low, high| {
            rng.irand(low, high)
        }) else {
            return false;
        };
        let Some(peer) = self.peer_mut(usize::from(number)) else {
            return false;
        };
        take(
            dodge,
            &mut peer.state,
            &mut peer.knockdown.hand_extend_time,
            level_time,
        );
        peer.movement = peer.movement.reseeded(&peer.state);
        let origin = peer.state.origin();
        if dodge.by_speed {
            let _ =
                self.with_force_frame(usize::from(number), level_time, |state, force, frame| {
                    jkr_game_jka::force_powers::speed(state, force, 500, frame)
                });
            if let Some(peer) = self.peer_mut(usize::from(number)) {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        } else {
            self.general_sound(SPEED_SOUND, origin, SPEED_SOUND_CHANNEL, level_time);
        }
        true
    }

    /// `G_GetHitLocation` for `point` on the player at `number`: its linked box grown by a
    /// unit, its yaw `0`, as the reference leaves a player's `r.currentAngles`.
    pub(super) fn player_hit_location(&self, number: u16, point: [f32; 3]) -> i32 {
        let Some(peer) = self.peer(usize::from(number)) else {
            return jkr_game_jka::force_dodge::HL_NONE;
        };
        let (bottom, top) = peer.movement.box_bounds();
        let origin = peer.state.origin();
        let bounds = (
            std::array::from_fn(|axis| origin[axis] + bottom[axis] - 1.0),
            std::array::from_fn(|axis| origin[axis] + top[axis] + 1.0),
        );
        jkr_game_jka::damage::hit_location(0.0, bounds, point) as i32
    }
}
