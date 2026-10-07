//! Kicks landing on this server ([`jkr_game_jka::saber_kick`]): at a player's turn, before
//! its saber is posed for the frame, a player whose legs play a kicking animation traces
//! its kick from its skeleton as the frame before left it (the Ghoul2 clock), through the
//! models as `d_saberKickTweak` asks, and what the kick strikes takes the punch, the blow
//! (`G_Damage`'s usual routes, [`NativeGame::shot_hurt`]) and the toss. NPCs that kick are
//! not run here: no stock map has one that does.

use super::*;
use jkr_game_jka::damage::DamageRequest;
use jkr_game_jka::saber_kick::{
    AfterBlow, Draws, KICK_BOX, Kicker, Landing, Side, Struck, Target, can_be_enemy, kick_trace,
    land, toss,
};

/// `PMF_TIME_KNOCKBACK` in the wire `pm_flags` (field 38).
const PMF_TIME_KNOCKBACK: u32 = 64;
const PS_PM_FLAGS: usize = 38;
/// `DAMAGE_NO_KNOCKBACK`, `MOD_MELEE`, `CHAN_AUTO`.
const DAMAGE_NO_KNOCKBACK: u32 = 4;
const MOD_MELEE: u32 = 2;
const CHAN_AUTO: u32 = 0;
/// `CLASS_VEHICLE`: a vehicle struck is not tossed.
const CLASS_VEHICLE: i32 = 53;
/// How long a kick's target credits its fall to the kicker (`otherKillerTime`).
const OTHER_KILLER_MS: i32 = 10_000;

impl NativeGame {
    /// `G_KickSomeMofos` for player `client` at `level_time` (`w_saber.c:8149-8152`): its
    /// kick traced when the animation lands one, and landed.
    pub(super) fn kick_frame(&mut self, client: usize, level_time: i32) {
        let tweak = self.cvars.integer(b"d_saberKickTweak") != 0;
        let odd_frame = self.level_frames & 1 == 1;
        // The players' skeletons answer at the Ghoul2 clock, a frame behind the level's.
        let ghoul2_time = if self.previous_frame_time == 0 {
            level_time
        } else {
            self.previous_frame_time
        };
        let mut rng = self.deaths.rng;
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let legs = peer.state.leg_animation();
        if !jkr_game_jka::saber_rules::kicking(legs) {
            return;
        }
        let animation_length = peer
            .movement
            .animation_lengths()
            .and_then(|lengths| lengths.length_ms(legs))
            .unwrap_or(0);
        let origin = peer.state.origin();
        let yaw = peer.state.view_angles()[1];
        let kicker = Kicker {
            legs_anim: legs,
            torso_anim: peer.state.torso_animation(),
            animation_length,
            legs_timer: peer.state.legs_timer(),
            yaw,
            origin,
            current_origin: origin,
            maxs: peer.movement.box_bounds().1,
            odd_frame,
            bolts: true,
        };
        // `!self->ghoul2`: no kicks before the model is posed.
        let Some(skeleton) = peer.skeleton.as_mut() else {
            return;
        };
        let kick = kick_trace(&kicker, &mut rng, |bolt| {
            skeleton
                .skeleton
                .bolt_point(&skeleton.models, bolt.name(), yaw, origin, ghoul2_time)
                .ok()
                .flatten()
                .unwrap_or(origin)
        });
        let mut memory = peer.kick;
        self.deaths.rng = rng;
        let Some(kick) = kick else { return };
        let trace = self.shot_sweep(
            client,
            kick.start,
            (KICK_BOX.map(|size| -size), KICK_BOX),
            kick.end,
            jkr_game_jka::map_turret_world::MASK_SHOT,
            tweak,
            level_time,
        );
        let struck = Struck {
            fraction: trace.fraction,
            start_solid: trace.start_solid,
            all_solid: trace.all_solid,
            entity: trace.entity_number,
            end: trace.end_position,
        };
        let mut world = KickWorld {
            game: self,
            kicker: client,
            origin,
            level_time,
            tweak,
        };
        let _ = land(
            &kick,
            struck,
            &mut memory,
            &kicker,
            level_time,
            tweak,
            &mut world,
        );
        if let Some(peer) = self.peer_mut(client) {
            peer.kick = memory;
        }
    }

    /// A client of the game as `G_CanBeEnemy` reads it: a player or an NPC.
    fn kick_side(&self, number: u16) -> Option<Side> {
        if let Some(peer) = self.peer(usize::from(number)) {
            return Some(Side {
                number,
                duel: peer
                    .state
                    .duel_in_progress()
                    .then(|| peer.state.duel_index()),
                team: i32::from(peer.session.team),
            });
        }
        self.npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)
            .map(|npc| Side {
                number,
                duel: None,
                team: npc.session_team,
            })
    }
}

/// The server as a kick's landing sees it.
struct KickWorld<'a> {
    game: &'a mut NativeGame,
    kicker: usize,
    /// The kicker's `r.currentOrigin`, where its sound plays.
    origin: [f32; 3],
    level_time: i32,
    tweak: bool,
}

impl Draws for KickWorld<'_> {
    fn irand(&mut self, low: i32, high: i32) -> i32 {
        self.game.deaths.rng.irand(low, high)
    }

    fn flrand(&mut self, low: f32, high: f32) -> f32 {
        self.game.deaths.rng.flrand(low, high)
    }
}

impl Landing for KickWorld<'_> {
    fn sound(&mut self, name: &str) {
        self.game
            .general_sound(name.as_bytes(), self.origin, CHAN_AUTO, self.level_time);
    }

    fn target(&mut self, number: u16) -> Target {
        let shot = self.game.shot_target(number);
        Target {
            in_use: true,
            takes_damage: shot.takes_damage,
            client: shot.client,
        }
    }

    /// The blow through the shot's routes. Whatever `G_Damage` refuses before it
    /// normalizes the direction — a duel's outsider, a rider inside its vehicle — is no
    /// enemy to toss or has no body to kick, so the answer only matters where it is yes.
    fn damage(&mut self, number: u16, direction: [f32; 3], point: [f32; 3], amount: i32) -> bool {
        // A player struck credits its next fall to the kicker for ten seconds; an NPC's
        // credit is not kept.
        let (kicker, level_time) = (self.kicker as u16, self.level_time);
        if let Some(peer) = self.game.peer_mut(usize::from(number)) {
            peer.wounds.other_killer = jkr_game_jka::damage::OtherKiller {
                number: kicker,
                time: level_time + OTHER_KILLER_MS,
                debounce_time: level_time + OTHER_KILLER_MS,
            };
        }
        let request = DamageRequest {
            level_time: self.level_time,
            attacker: self.game.attacker_for(self.kicker as u16),
            direction: Some(direction),
            point: Some(point),
            damage: amount,
            flags: DAMAGE_NO_KNOCKBACK,
            means: MOD_MELEE,
        };
        let _ = self.game.shot_hurt(self.kicker, number, request, false);
        true
    }

    fn after_blow(&mut self, number: u16) -> Option<AfterBlow> {
        let (Some(me), Some(other)) = (
            self.game.kick_side(self.kicker as u16),
            self.game.kick_side(number),
        ) else {
            return None;
        };
        let (gametype, friendly_fire) = (
            self.game.gametype,
            self.game.cvars.integer(b"g_friendlyFire") != 0,
        );
        let enemy = can_be_enemy(me, other, gametype, friendly_fire);
        let flags = |state: &PlayerState| state.raw_field(PS_PM_FLAGS).unwrap_or(0);
        if let Some(peer) = self.game.peer(usize::from(number)) {
            return Some(AfterBlow {
                health: peer.health,
                knocked_back: flags(&peer.state) & PMF_TIME_KNOCKBACK != 0,
                enemy,
            });
        }
        let npc = self
            .game
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)?;
        Some(AfterBlow {
            health: npc.health,
            knocked_back: flags(&npc.player) & PMF_TIME_KNOCKBACK != 0,
            enemy,
        })
    }

    fn toss(&mut self, number: u16, direction: [f32; 3], strength: f32) {
        let (level_time, tweak) = (self.level_time, self.tweak);
        if let Some(peer) = self.game.peer_mut(usize::from(number)) {
            let health = peer.health;
            toss(
                &mut peer.state,
                &mut peer.knockdown,
                health,
                direction,
                strength,
                level_time,
                tweak,
            );
            peer.movement = peer.movement.reseeded(&peer.state);
            return;
        }
        if let Some(npc) = self
            .game
            .npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == number)
            .filter(|npc| npc.definition.entity_class != CLASS_VEHICLE)
        {
            let health = npc.health;
            toss(
                &mut npc.player,
                &mut npc.mind.knockdown,
                health,
                direction,
                strength,
                level_time,
                tweak,
            );
        }
    }
}
