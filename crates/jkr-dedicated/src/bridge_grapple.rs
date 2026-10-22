//! `g_debugMelee`'s grapple on this server ([`jkr_game_jka::debug_melee`]): at a client's
//! turn in the frame (`WP_SaberPositionUpdate`, `w_saber.c:8149-8390`) — a player's after
//! its kick, the NPCs' just before they think — a reaching hand (`BOTH_KYLE_GRAB`) closes
//! on the player or NPC that the trace from its body to its right hand meets
//! (`G_GrabSomeMofos`), and a grapple runs: the grabber's blows, the held one kept in
//! front of it, the two facing each other, the grapple broken off.

use super::*;
use jkr_game_jka::damage::DamageRequest;
use jkr_game_jka::debug_melee::{
    BOTH_DEADFLOP1, BOTH_KYLE_GRAB, BOTH_KYLE_MISS, Grapple, HELD, REACH, beat, grapple_move,
    hold_distance, holds, pretty_close, toss,
};
use jkr_game_jka::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, animate,
};

/// The grab's box, 8 units wide, swept from the body to the right hand.
const HAND: [f32; 3] = [4.0; 3];
/// A player's `clipmask` (`MASK_PLAYERSOLID`).
const MASK_PLAYERSOLID: u32 = 0x1111;
const MOD_MELEE: u32 = 2;
/// `EV_GENERAL_SOUND`, the wire field its channel goes in (`saberEntityNum`), `CHAN_AUTO`.
const EV_GENERAL_SOUND: u32 = 76;
const ES_SABER_ENTITY: usize = 37;
const CHAN_AUTO: u32 = 0;
const WP_SABER: u8 = 3;
/// `ENTITYNUM_WORLD`.
const ENTITY_WORLD: u16 = 1_022;
/// Wire fields: the animations and their timers, the weapon's time, the hand's pose and
/// the saber's.
const PS_WEAPON_TIME: usize = 10;
const PS_LEGS_ANIM: usize = 13;
const PS_TORSO_ANIM: usize = 15;
const PS_TORSO_TIMER: usize = 20;
const PS_LEGS_TIMER: usize = 21;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_SABER_HOLSTERED: usize = 81;
const HANDEXTEND_KNOCKDOWN: u32 = 8;

/// A client in a grapple as one beat reads it.
#[derive(Clone, Copy, Debug)]
struct Body {
    origin: [f32; 3],
    view_angles: [f32; 3],
    torso: u16,
    torso_timer: i32,
    legs: u16,
    legs_timer: i32,
    health: i32,
    grapple: Grapple,
    /// `pers.cmd.forwardmove`.
    forward_move: i8,
}

impl Body {
    /// Still well short of the end of a grapple pose: more than 100 ms of it left.
    fn far_from_done(&self) -> bool {
        grapple_move(self.torso) != 0 && self.torso_timer > 100
            || grapple_move(self.legs) != 0 && self.legs_timer > 100
    }
}

/// A client's state as a beat changes it.
struct Client<'a> {
    state: &'a mut PlayerState,
    grapple: &'a mut Grapple,
    knockdown: &'a mut jkr_game_jka::knockdown::Knockdown,
    lengths: Option<&'a dyn AnimationLengths>,
}

impl Client<'_> {
    /// `G_SetAnim(SETANIM_BOTH, animation, OVERRIDE | HOLD)`; whether the torso took it.
    fn animate(&mut self, animation: u16) -> bool {
        if let Some(lengths) = self.lengths {
            animate(
                self.state,
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                lengths,
                [1.0; 2],
            );
        }
        self.state.torso_animation() == animation
    }

    /// The weapon busy as long as the torso's animation runs.
    fn busy_with_torso(&mut self) {
        let timer = self.state.torso_timer();
        self.state.set_raw_field(PS_WEAPON_TIME, timer as u32);
    }

    /// `saberHolstered = 2` on a lit saber in hand; whether it went out.
    fn saber_out(&mut self) -> bool {
        let lit = self.state.weapon() == WP_SABER && self.state.saber_holstered() == 0;
        if lit {
            self.state.set_raw_field(PS_SABER_HOLSTERED, 2);
        }
        lit
    }
}

impl NativeGame {
    /// `WP_SaberPositionUpdate`'s grapple (`w_saber.c:8153-8390`) for client `number`, a
    /// player or an NPC that is not kicking: a reaching hand grabs, a grapple beats on.
    pub(super) fn grapple_frame(&mut self, number: u16, level_time: i32) {
        let Some(me) = self.grapple_body(number) else {
            return;
        };
        if me.torso == BOTH_KYLE_GRAB {
            self.grab(number, &me, level_time);
        } else if me.grapple.state != 0 {
            self.grapple_beat(number, &me, level_time);
        }
    }

    /// The NPCs' grapples ([`Self::grapple_frame`]), in the roster's order, before they
    /// think: an NPC's `WP_SaberPositionUpdate` comes before its think
    /// (`g_main.c:3342-3346`), after every player's.
    pub(super) fn npc_grapples(&mut self, level_time: i32) {
        for at in 0..self.npcs.roster.actors.len() {
            let Some(npc) = self.npcs.roster.actors.get(at) else {
                break;
            };
            let busy =
                npc.player.torso_animation() == BOTH_KYLE_GRAB || npc.mind.grapple.state != 0;
            if busy && !jkr_game_jka::saber_rules::kicking(npc.player.leg_animation()) {
                let number = npc.number;
                self.grapple_frame(number, level_time);
            }
        }
    }

    /// `G_GrabSomeMofos` (`w_saber.c:7976-8089`): the trace from the body to the right hand
    /// (`MASK_SHOT`, through the models) meets a living enemy level with it that is in no
    /// grapple pose but a reaching one, which it then holds in the grapple its move asks
    /// for; nothing caught with its hand nearly back, the miss. Nothing without a model or
    /// a right hand.
    fn grab(&mut self, number: u16, me: &Body, level_time: i32) {
        let Some(hand) = self.right_hand(number, me, level_time) else {
            return;
        };
        let trace = self.shot_sweep(
            usize::from(number),
            me.origin,
            (HAND.map(|size| -size), HAND),
            hand,
            jkr_game_jka::map_turret_world::MASK_SHOT,
            true,
            level_time,
        );
        if trace.fraction != 1.0
            && trace.entity_number < ENTITY_WORLD
            && let Some(held) = self.grapple_body(trace.entity_number)
            && self.grabbable(number, me, trace.entity_number, &held)
            && let Some((hold, held_pose)) = holds(me.forward_move)
        {
            self.begin_grapple(number, trace.entity_number, (hold, held_pose), level_time);
        }
        self.miss_unless_grappling(number);
    }

    /// Whether `held` (client `number`) can be grabbed by `me` (client `grabber`): alive, an
    /// enemy (`G_CanBeEnemy`), level with it, in no grapple pose but a reaching one.
    fn grabbable(&self, grabber: u16, me: &Body, number: u16, held: &Body) -> bool {
        let (Some(mine), Some(theirs)) = (self.kick_side(grabber), self.kick_side(number)) else {
            return false;
        };
        let friendly_fire = self.cvars.integer(b"g_friendlyFire") != 0;
        let free = |animation: u16| grapple_move(animation) == 0 || animation == BOTH_KYLE_GRAB;
        held.health > 0
            && jkr_game_jka::saber_kick::can_be_enemy(mine, theirs, self.gametype, friendly_fire)
            && pretty_close(held.origin[2], me.origin[2], 4.0)
            && free(held.torso)
            && free(held.legs)
    }

    /// The grapple begun: both told whom they grapple, the grabber in its pose with the
    /// weapon busy as long, the held one in its own — its lit saber put out, held at least
    /// as long as the grabber.
    fn begin_grapple(
        &mut self,
        grabber: u16,
        held: u16,
        (hold, held_pose): (u16, u16),
        level_time: i32,
    ) {
        let grabbing = self.with_client(grabber, |mut client| {
            *client.grapple = Grapple {
                index: held,
                state: 1,
            };
            if client.animate(hold) {
                client.busy_with_torso();
            }
            client.state.torso_timer()
        });
        let saber_out = self.with_client(held, |mut client| {
            *client.grapple = Grapple {
                index: grabber,
                state: HELD,
            };
            if !client.animate(held_pose) {
                return false;
            }
            let out = client.saber_out();
            if let Some(timer) = grabbing
                && client.state.torso_timer() < timer
            {
                client.state.set_raw_field(PS_TORSO_TIMER, timer as u32);
            }
            client.busy_with_torso();
            out
        });
        if saber_out == Some(true) {
            self.saber_off_sounds_of(held, level_time);
        }
    }

    /// `G_SetAnim(BOTH_KYLE_MISS)` with the weapon busy as long, for a reaching hand nearly
    /// back (under 300 ms) that holds no one.
    fn miss_unless_grappling(&mut self, number: u16) {
        let _ = self.with_client(number, |mut client| {
            if client.state.torso_timer() < 300 && client.grapple.state == 0 {
                miss(&mut client);
            }
        });
    }

    /// A beat of the grapple (`w_saber.c:8157-8390`): broken off when the other is gone,
    /// grapples someone else, has left the poses (or this one has), either is dead, or
    /// they are 4 units apart in height — the pose cut short by the miss when far from
    /// its end; let go past 64 units apart likewise; else the two face each other, the
    /// held one is kept in front of its grabber, and the grabber's blow lands when due.
    fn grapple_beat(&mut self, number: u16, me: &Body, level_time: i32) {
        let other = self.grapple_body(me.grapple.index).filter(|other| {
            other.grapple.index == number
                && grapple_move(other.torso) != 0
                && grapple_move(other.legs) != 0
                && grapple_move(me.torso) != 0
                && grapple_move(me.legs) != 0
                && other.grapple.state != 0
                && other.health >= 1
                && me.health >= 1
                && pretty_close(me.origin[2], other.origin[2], 4.0)
        });
        let Some(other) = other else {
            let far = me.far_from_done();
            let _ = self.with_client(number, |mut client| {
                client.grapple.state = 0;
                if far {
                    miss(&mut client);
                }
            });
            return;
        };
        let apart: [f32; 3] = std::array::from_fn(|axis| other.origin[axis] - me.origin[axis]);
        let distance = (apart[0] * apart[0] + apart[1] * apart[1] + apart[2] * apart[2]).sqrt();
        if distance > REACH {
            if me.far_from_done() {
                let _ = self.with_client(number, |mut client| {
                    client.grapple.state = 0;
                    miss(&mut client);
                });
            }
            return;
        }
        let (pitch, yaw) = jkr_game_jka::damage::vector_to_angles(apart);
        self.set_client_view_angle(number, [pitch, yaw, 0.0]);
        if me.grapple.state >= HELD {
            self.keep_in_front(number, me, &other, level_time);
        } else {
            self.grabber_blow(number, me, me.grapple.index, &other, level_time);
        }
    }

    /// The held one placed `hold_distance` in front of its grabber when its box gets there
    /// clear (its own trace, `clipmask`).
    fn keep_in_front(&mut self, number: u16, me: &Body, grabber: &Body, level_time: i32) {
        let (forward, _) = jkr_game_jka::pmove::flight::flight_axes(grabber.view_angles);
        let distance = hold_distance(grabber.torso);
        let spot: [f32; 3] =
            std::array::from_fn(|axis| grabber.origin[axis] + distance * forward.to_array()[axis]);
        let Some((bounds, mask)) = self.client_box(number) else {
            return;
        };
        let trace = self.shot_sweep(
            usize::from(number),
            me.origin,
            bounds,
            spot,
            mask,
            false,
            level_time,
        );
        if trace.start_solid || trace.all_solid || trace.fraction != 1.0 {
            return;
        }
        if let Some(peer) = self.peer_mut(usize::from(number)) {
            peer.state.set_origin(spot);
            peer.movement = peer.movement.reseeded(&peer.state);
        } else if let Some(npc) = self.npc_mut(number) {
            npc.player.set_origin(spot);
            npc.current_origin = spot;
            npc.relink();
        }
    }

    /// The grabber's beat on the one it holds (`held`): a lit saber of the held one's put
    /// out, then the blow its pose has due ([`beat`]) — the damage, the survivor put back
    /// in its pose, the throw and knockdown, the cry, the release or the flop.
    fn grabber_blow(&mut self, grabber: u16, me: &Body, held: u16, other: &Body, level_time: i32) {
        if self.with_client(held, |mut client| client.saber_out()) == Some(true) {
            self.saber_off_sounds_of(held, level_time);
        }
        let Some(beat) = beat(me.torso, me.torso_timer, me.grapple.state) else {
            return;
        };
        let (pose, timer) = (other.torso, other.torso_timer);
        if beat.damage > 0 {
            let request = DamageRequest {
                level_time,
                attacker: self.attacker_for(grabber),
                direction: None,
                point: Some(me.origin),
                damage: beat.damage,
                flags: 0,
                means: MOD_MELEE,
            };
            let _ = self.strike(usize::from(grabber), usize::from(held), request, false);
        }
        if beat.cry {
            self.voice_sound(held, other.origin, b"*pain100.wav", level_time);
        }
        let alive = self.grapple_body(held).is_some_and(|body| body.health > 0);
        let grabber_origin = me.origin;
        let _ = self.with_client(held, |client| {
            if beat.restore && alive {
                for (field, value) in [
                    (PS_TORSO_ANIM, u32::from(pose)),
                    (PS_TORSO_TIMER, timer as u32),
                    (PS_LEGS_ANIM, u32::from(pose)),
                    (PS_LEGS_TIMER, timer as u32),
                    (PS_WEAPON_TIME, timer as u32),
                ] {
                    client.state.set_raw_field(field, value);
                }
            }
            if beat.toss {
                let velocity = client.state.velocity();
                let thrown = toss(velocity, grabber_origin, client.state.origin());
                client.state.set_velocity(thrown);
                if alive {
                    client
                        .state
                        .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_KNOCKDOWN);
                    client.knockdown.hand_extend_time = level_time + 1_300;
                }
            }
            if beat.release {
                if alive {
                    client.state.set_raw_field(PS_TORSO_TIMER, 1_000);
                    client.grapple.state = 0;
                } else {
                    client
                        .state
                        .set_raw_field(PS_TORSO_ANIM, u32::from(BOTH_DEADFLOP1));
                    client
                        .state
                        .set_raw_field(PS_LEGS_ANIM, u32::from(BOTH_DEADFLOP1));
                }
            }
        });
        let _ = self.with_client(grabber, |client| client.grapple.state = beat.state);
    }

    /// Client `number` (a player or an NPC) as a grapple reads it.
    fn grapple_body(&self, number: u16) -> Option<Body> {
        let body = |state: &PlayerState, health, grapple, forward_move| Body {
            origin: state.origin(),
            view_angles: state.view_angles(),
            torso: state.torso_animation(),
            torso_timer: state.torso_timer(),
            legs: state.leg_animation(),
            legs_timer: state.legs_timer(),
            health,
            grapple,
            forward_move,
        };
        if let Some(peer) = self.peer(usize::from(number)) {
            return Some(body(
                &peer.state,
                peer.health,
                peer.grapple,
                peer.last_command.forward_move,
            ));
        }
        let npc = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)?;
        Some(body(
            &npc.player,
            npc.health,
            npc.mind.grapple,
            npc.mind.command.forward_move,
        ))
    }

    /// Runs `change` on client `number`'s state, grapple and knockdown with its animation
    /// table; a player's movement then restarts from its state.
    fn with_client<R>(&mut self, number: u16, change: impl FnOnce(Client<'_>) -> R) -> Option<R> {
        if let Some(peer) = self.peer_mut(usize::from(number)) {
            let result = change(Client {
                state: &mut peer.state,
                grapple: &mut peer.grapple,
                knockdown: &mut peer.knockdown,
                lengths: peer.movement.animation_lengths(),
            });
            peer.movement = peer.movement.reseeded(&peer.state);
            return Some(result);
        }
        let npc = self.npc_mut(number)?;
        Some(change(Client {
            state: &mut npc.player,
            grapple: &mut npc.mind.grapple,
            knockdown: &mut npc.mind.knockdown,
            lengths: npc.movement.animation_lengths(),
        }))
    }

    /// NPC `number`, if it is one.
    fn npc_mut(&mut self, number: u16) -> Option<&mut jkr_game_jka::npc_spawn::NpcActor> {
        self.npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == number)
    }

    /// Client `number`'s box (`r.mins`, `r.maxs`) and `clipmask`.
    fn client_box(&self, number: u16) -> Option<(([f32; 3], [f32; 3]), u32)> {
        if let Some(peer) = self.peer(usize::from(number)) {
            return Some((peer.movement.box_bounds(), MASK_PLAYERSOLID));
        }
        let npc = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)?;
        Some(((npc.mins, npc.maxs), npc.clip_mask))
    }

    /// Where client `number`'s right hand (`*r_hand`) is with its model at its origin
    /// facing its view's yaw, at the Ghoul2 clock; `None` without a model or the bolt.
    fn right_hand(&mut self, number: u16, me: &Body, level_time: i32) -> Option<[f32; 3]> {
        // The skeletons answer at the Ghoul2 clock, a frame behind the level's.
        let ghoul2_time = if self.previous_frame_time == 0 {
            level_time
        } else {
            self.previous_frame_time
        };
        let yaw = me.view_angles[1];
        if let Some(peer) = self.peer_mut(usize::from(number)) {
            let skeleton = peer.skeleton.as_mut()?;
            return skeleton
                .skeleton
                .bolt_point(&skeleton.models, "*r_hand", yaw, me.origin, ghoul2_time)
                .ok()
                .flatten();
        }
        self.npcs
            .bolt_point(number, "*r_hand", yaw, me.origin, ghoul2_time)
    }

    /// `SetClientViewAngle` on client `number`: its view, and the delta angles against its
    /// last command (`pers.cmd`) that hold it there.
    fn set_client_view_angle(&mut self, number: u16, angles: [f32; 3]) {
        let delta = |command: &jkr_protocol::UserCommand| -> [i32; 3] {
            std::array::from_fn(|axis| {
                (((angles[axis] * 65_536.0 / 360.0) as i32) & 65_535) - command.angles[axis]
            })
        };
        if let Some(peer) = self.peer_mut(usize::from(number)) {
            peer.state.set_delta_angles(delta(&peer.last_command));
            peer.state.set_view_angles(angles);
            peer.movement = peer.movement.reseeded(&peer.state);
        } else if let Some(npc) = self.npc_mut(number) {
            npc.player.set_delta_angles(delta(&npc.mind.command));
            npc.player.set_view_angles(angles);
        }
    }

    /// `G_Sound(ent, CHAN_AUTO, saber[n].soundOff)` for client `number`'s sabers: the
    /// first's, and the second's where it has one.
    fn saber_off_sounds_of(&mut self, number: u16, level_time: i32) {
        let found = if let Some(peer) = self.peer(usize::from(number)) {
            Some((peer.state.origin(), peer.saber_off_sounds()))
        } else {
            self.npcs
                .roster
                .actors
                .iter()
                .find(|npc| npc.number == number)
                .map(|npc| {
                    let [first, second] = &npc.definition.sabers;
                    let second = if second.model.is_empty() {
                        0
                    } else {
                        second.sound_off
                    };
                    (npc.current_origin, [first.sound_off, second])
                })
        };
        let Some((origin, sounds)) = found else {
            return;
        };
        for index in sounds.into_iter().filter(|index| *index != 0) {
            let mut sound = EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(index),
                origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            sound.extra[0] = (ES_SABER_ENTITY, CHAN_AUTO);
            let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
        }
    }
}

/// `G_SetAnim(BOTH_KYLE_MISS)`, the weapon busy as long.
fn miss(client: &mut Client<'_>) {
    if client.animate(BOTH_KYLE_MISS) {
        client.busy_with_torso();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Identity, NativeGame};
    use super::*;
    use jkr_game_jka::debug_melee::{BOTH_KYLE_PA_1, BOTH_PLAYER_PA_1};
    use jkr_network::LegacyGameHost;
    use jkr_protocol::UserCommand;

    /// Two players in the grab punch: 0 holding 1, `apart` units in front of it, with
    /// 3.39 s of the poses left.
    fn grappling(apart: f32) -> NativeGame {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        for (client, name) in [(0, "a"), (1, "b")] {
            let userinfo = format!(
                "\\name\\{name}\\rate\\25000\\snaps\\40\\model\\kyle/default\\forcepowers\\7-1-032330000000001333\\color1\\4\\color2\\4\\handicap\\100\\cg_predictItems\\1\\saber1\\single_1\\saber2\\none\\char_color_red\\255\\char_color_green\\255\\char_color_blue\\255\\ip\\127.0.0.{}:1",
                client + 1
            );
            game.client_connect(client, userinfo.as_bytes()).unwrap();
            game.enter_world(client, &UserCommand::default(), 100);
        }
        for (client, origin, pose, grapple) in [
            (0, [0.0; 3], BOTH_KYLE_PA_1, Grapple { index: 1, state: 1 }),
            (
                1,
                [apart, 0.0, 0.0],
                BOTH_PLAYER_PA_1,
                Grapple {
                    index: 0,
                    state: HELD,
                },
            ),
        ] {
            let peer = game.peer_mut(client).unwrap();
            peer.state.set_origin(origin);
            for (field, value) in [
                (PS_LEGS_ANIM, u32::from(pose)),
                (PS_TORSO_ANIM, u32::from(pose)),
                (PS_LEGS_TIMER, 3_390),
                (PS_TORSO_TIMER, 3_390),
            ] {
                peer.state.set_raw_field(field, value);
            }
            peer.grapple = grapple;
            // In the game past the spawn's protection, without the armour that would take
            // the blows.
            peer.session.team = 0;
            peer.invulnerable_until = 0;
            peer.state.stats[5] = 0;
            let flags = peer.state.raw_field(super::super::EFLAGS).unwrap_or(0);
            peer.state
                .set_raw_field(super::super::EFLAGS, flags & !super::super::EF_INVULNERABLE);
        }
        game
    }

    /// The grab punch's first blow (`w_saber.c:8252-8272`): 10 off the held one, put back
    /// in its pose, the grabber facing it and on to its next blow.
    #[test]
    fn the_grab_punch_lands_and_holds_its_victim() {
        let mut game = grappling(46.0);
        let health = game.peer(1).unwrap().health;
        game.grapple_frame(0, 200);
        let held = game.peer(1).unwrap();
        // Ten, placed by where on the held one the grabber stands (`point` is its origin).
        assert!(held.health < health && held.health >= health - 10);
        assert_eq!(held.state.torso_animation(), BOTH_PLAYER_PA_1);
        assert_eq!(held.state.leg_animation(), BOTH_PLAYER_PA_1);
        assert_eq!(held.state.weapon_time(), 3_390);
        let grabber = game.peer(0).unwrap();
        assert_eq!(grabber.grapple.state, 2);
        assert_eq!(grabber.state.view_angles()[1], 0.0);
        // The held one turns to face its grabber.
        game.grapple_frame(1, 200);
        assert_eq!(game.peer(1).unwrap().state.view_angles()[1], 180.0);
    }

    /// Drifted past 64 units apart with the pose far from its end, the grabber lets go.
    #[test]
    fn a_grapple_drifted_apart_is_let_go() {
        let mut game = grappling(80.0);
        let health = game.peer(1).unwrap().health;
        game.grapple_frame(0, 200);
        assert_eq!(game.peer(0).unwrap().grapple.state, 0);
        assert_eq!(game.peer(1).unwrap().health, health);
    }
}
