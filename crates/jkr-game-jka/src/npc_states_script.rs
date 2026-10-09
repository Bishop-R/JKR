//! The states only a script sets (`NPC_behavior.c`): asleep until an alert (`NPC_BSSleep`,
//! `:518-539`; the wake-up script is not run), a jump to the navigation goal along a
//! parabola (`NPC_BSJump`, `:749-939`), waiting to be out of client 0's sight to vanish
//! (`NPC_BSRemove`, `:941-958`), and flying through walls to the goal (`NPC_BSNoClip`,
//! `:1181-1213`). A host that runs scripts sets them through the roster as `Q3_SetBState`
//! does (`g_ICARUScb.c:2599-2713`): the behaviour state, `client->noclip` for `BS_NOCLIP`,
//! the jump's state for `BS_JUMP`; and as `Q3_SetTempBState`, `Q3_SetDefaultBState` and
//! `G_ActivateBehavior` do.

use crate::npc_behavior::bstate;
use crate::npc_roster::NpcRoster;
use crate::npc_senses::{AEL_MINOR, subtract};
use crate::npc_spawn::{NpcHost, NpcThink};
use crate::npc_states::jump;
use crate::npc_world::NpcWorld;
use crate::player_angle_math::vector_angles;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS};
use jkr_protocol::UserCommand;

/// `BOTH_CROUCH1`, `BOTH_INAIR1`, `BOTH_LAND1` (`anims.h`).
const BOTH_CROUCH1: u16 = 1_004;
const BOTH_INAIR1: u16 = 1_139;
const BOTH_LAND1: u16 = 1_140;
/// `APEX_HEIGHT`; `MIN_ANGLE_ERROR`.
const APEX_HEIGHT: f32 = 200.0;
const MIN_ANGLE_ERROR: f32 = 0.01;
/// `FL_NO_KNOCKBACK`; `NPCAI_MOVING`.
const FL_NO_KNOCKBACK: u32 = 0x800;
const NPCAI_MOVING: u32 = 0x4;
/// `NPCAI_TOUCHED_GOAL`.
const NPCAI_TOUCHED_GOAL: u32 = 0x8;
/// `EF_NODRAW`, `ET_INVISIBLE`; `s.eType`, `s.eFlags`.
const EF_NODRAW: u32 = 0x100;
const ET_INVISIBLE: u32 = 12;
const ES_TYPE: usize = 8;
const ES_EFLAGS: usize = 19;
/// `FRAMETIME`.
const FRAMETIME: i32 = 100;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = 1_023;

/// `GetIDForString(BSTable, name)`: the behaviour state a script names, in any case.
pub fn behavior_state_named(name: &str) -> Option<i32> {
    crate::script_entity::BEHAVIOR_STATES
        .iter()
        .position(|known| known.eq_ignore_ascii_case(name))
        .map(|state| state as i32)
}

impl NpcRoster {
    /// [`NpcWorld::script_set_bstate`] on the NPC numbered `number`, the state `name`:
    /// whether the script's task completes at once (it does where there is no such NPC).
    pub fn script_set_bstate(
        &mut self,
        number: u16,
        name: &str,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> bool {
        let Some(at) = self.actors.iter().position(|npc| npc.number == number) else {
            return true;
        };
        let mut fired = Vec::new();
        self.world(level_time, host, &mut fired)
            .script_set_bstate(at, behavior_state_named(name))
    }

    /// `Q3_SetTempBState` (`g_ICARUScb.c:2725-2763`): the temporary state, where `name`
    /// is one. Its task completes at once.
    pub fn script_set_temp_bstate(&mut self, number: u16, name: &str) {
        if let (Some(npc), Some(state)) = (
            self.actors.iter_mut().find(|npc| npc.number == number),
            behavior_state_named(name),
        ) {
            npc.mind.temp_behavior = state;
        }
    }

    /// `Q3_SetDefaultBState` (`g_ICARUScb.c:2775-2797`): the state it falls back to, where
    /// `name` is one.
    pub fn script_set_default_bstate(&mut self, number: u16, name: &str) {
        if let (Some(npc), Some(state)) = (
            self.actors.iter_mut().find(|npc| npc.number == number),
            behavior_state_named(name),
        ) {
            npc.default_behavior = state;
        }
    }

    /// `G_ActivateBehavior` of a behaviour set that names a state (`NPC_utils.c:891-895`):
    /// the temporary state cleared and the state set.
    pub fn script_behavior_state(&mut self, number: u16, name: &str) {
        if let (Some(npc), Some(state)) = (
            self.actors.iter_mut().find(|npc| npc.number == number),
            behavior_state_named(name),
        ) {
            npc.mind.temp_behavior = bstate::DEFAULT;
            npc.behavior_state = state;
        }
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `G_ActivateBehavior(NPC, bset)` (`NPC_utils.c:873-900`) from the NPC's own doing:
    /// whether it has that set. A set naming a behaviour state takes it at once; a script
    /// is the host's to start this frame (`ICARUS_RunScript`, [`NpcHost::run_behavior`]).
    pub(crate) fn activate_behavior(&mut self, me: usize, bset: usize) -> bool {
        let npc = &mut self.actors[me];
        let Some(name) = npc.behavior_sets[bset]
            .as_deref()
            .filter(|name| !name.is_empty())
        else {
            return false;
        };
        match behavior_state_named(&String::from_utf8_lossy(name)) {
            Some(state) => {
                npc.mind.temp_behavior = bstate::DEFAULT;
                npc.behavior_state = state;
            }
            None => {
                let number = npc.number;
                self.host.run_behavior(number, bset);
            }
        }
        true
    }

    /// `Q3_SetBState` (`g_ICARUScb.c:2599-2713`), a script's `SET_BEHAVIOR_STATE` of `state`
    /// (`None` for a name `BSTable` lacks, which changes only the flags): a search or a
    /// wander started from the NPC's waypoint (none found: nothing changes); the temporary
    /// state cleared; the state set (and made the default for `BS_DEFAULT`);
    /// `client->noclip` for `BS_NOCLIP` alone; a jump begun facing its goal. Whether the
    /// script's task completes at once: all but `BS_ADVANCE_FIGHT`'s, which waits for the
    /// state to say it is done.
    pub fn script_set_bstate(&mut self, me: usize, state: Option<i32>) -> bool {
        if let Some(state) = state {
            if state == bstate::SEARCH || state == bstate::WANDER {
                let mut waypoint = self.actors[me].mind.tactics.waypoint;
                if waypoint == crate::npc_mind::WAYPOINT_NONE {
                    waypoint = self.closest_waypoint_for(
                        crate::npc_navigator::NavHolder::Actor(me),
                        crate::npc_mind::WAYPOINT_NONE,
                    );
                    self.actors[me].mind.tactics.waypoint = waypoint;
                    if waypoint == crate::npc_mind::WAYPOINT_NONE {
                        return true;
                    }
                }
                self.search_start(me, waypoint, state);
            }
            let npc = &mut self.actors[me];
            npc.mind.temp_behavior = bstate::DEFAULT;
            if npc.behavior_state == bstate::NOCLIP && state != bstate::NOCLIP {
                // "rise up out of the floor after noclipping" (`G_SetOrigin`).
                npc.current_origin[2] += 0.125;
                let origin = npc.current_origin;
                npc.player.set_origin(origin);
            }
            npc.behavior_state = state;
            if state == bstate::DEFAULT {
                npc.default_behavior = state;
            }
        }
        let npc = &mut self.actors[me];
        npc.ai_flags &= !NPCAI_TOUCHED_GOAL;
        npc.mind.noclip = state == Some(bstate::NOCLIP);
        if state == Some(bstate::JUMP) {
            npc.mind.states.jump_state = jump::FACING;
        }
        state != Some(bstate::ADVANCE_FIGHT)
    }

    /// `NPC_BSSleep` (`NPC_behavior.c:518-539`): an alert noticed runs its wake-up script
    /// (`BSET_AWAKE`); without one it sleeps on.
    pub fn bs_sleep(&mut self, me: usize) {
        if self.check_alerts(me, -1, false, AEL_MINOR).is_some() {
            let _ = self.activate_behavior(me, crate::script_entity::BSET_AWAKE);
        }
    }

    /// `NPC_BSJump` (`NPC_behavior.c:749-939`): facing the goal it crouches, then leaps
    /// for the apex a hundred units above the higher end of the way, lands, and is done
    /// (the goal cleared).
    pub fn bs_jump(&mut self, me: usize, command: &mut UserCommand) {
        let Some(goal) = self.actors[me].mind.goal.and_then(|_| self.goal_origin(me)) else {
            return;
        };
        let state = self.actors[me].mind.states.jump_state;
        if state != jump::JUMPING && state != jump::LANDING {
            let angles = vector_angles(subtract(goal, self.actors[me].current_origin));
            let npc = &mut self.actors[me];
            let (pitch, yaw) = (
                crate::npc_droid::angle_normalize360(angles[0]),
                crate::npc_droid::angle_normalize360(angles[1]),
            );
            (npc.mind.desired_pitch, npc.mind.locked_desired_pitch) = (pitch, pitch);
            (npc.desired_yaw, npc.mind.locked_desired_yaw) = (yaw, yaw);
        }
        self.update_angles(me, true, true, command);
        let npc = &self.actors[me];
        let yaw_error =
            crate::npc_senses::angle_delta(npc.player.view_angles()[1], npc.desired_yaw);
        let legs_timer = npc.player.legs_timer();
        match state {
            jump::FACING => {
                if yaw_error < MIN_ANGLE_ERROR {
                    self.set_animation(
                        me,
                        SETANIM_LEGS,
                        BOTH_CROUCH1,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                    self.actors[me].mind.states.jump_state = jump::CROUCHING;
                }
            }
            jump::CROUCHING => {
                if legs_timer > 0 {
                    return;
                }
                self.leap(me, goal);
            }
            jump::JUMPING => {
                if self.actors[me].state.ground_entity_num() != ENTITYNUM_NONE {
                    self.actors[me].player.set_velocity([0.0; 3]);
                    self.set_animation(
                        me,
                        SETANIM_BOTH,
                        BOTH_LAND1,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                    self.actors[me].mind.states.jump_state = jump::LANDING;
                } else if legs_timer > 0 {
                    return;
                } else {
                    self.set_animation(me, SETANIM_BOTH, BOTH_INAIR1, SETANIM_FLAG_OVERRIDE);
                }
            }
            jump::LANDING => {
                if legs_timer > 0 {
                    return;
                }
                self.actors[me].mind.states.jump_state = jump::WAITING;
                // "task complete no matter what".
                self.clear_goal(me);
                let npc = &mut self.actors[me];
                npc.mind.tactics.goal_time = self.level_time;
                npc.ai_flags &= !NPCAI_MOVING;
                command.forward_move = 0;
                npc.flags &= !FL_NO_KNOCKBACK;
            }
            _ => self.actors[me].mind.states.jump_state = jump::FACING,
        }
    }

    /// `NPC_BSJump`'s leap (`NPC_behavior.c:787-874`): the apex on the way from the higher
    /// end to the lower, half the rise above the higher; the velocity to reach it under the
    /// NPC's gravity (none without the time to).
    fn leap(&mut self, me: usize, goal: [f32; 3]) {
        let origin = self.actors[me].current_origin;
        let (p1, p2) = if origin[2] < goal[2] {
            (goal, origin)
        } else {
            (origin, goal)
        };
        let mut dir = subtract(p2, p1);
        dir[2] = 0.0;
        let mut xy = crate::saber_clash::normalize(&mut dir);
        let apex_height = APEX_HEIGHT / 2.0;
        let rise = p1[2] - p2[2];
        let z = (f64::from(apex_height + rise).sqrt() - f64::from(apex_height).sqrt()) as f32;
        if xy > 0.0 {
            xy -= z;
            xy *= 0.5;
        }
        let mut apex: [f32; 3] = std::array::from_fn(|axis| p1[axis] + xy * dir[axis]);
        apex[2] += apex_height;
        let npc = &mut self.actors[me];
        npc.mind.fight.death_point = apex;
        let gravity = npc.player.gravity();
        let height = apex[2] - origin[2];
        let time = (f64::from(height) / (0.5 * f64::from(gravity))).sqrt() as f32;
        if time == 0.0 {
            return;
        }
        let mut velocity = subtract(apex, origin);
        velocity[2] = 0.0;
        let distance = crate::saber_clash::normalize(&mut velocity);
        let forward = distance / time;
        let mut velocity = velocity.map(|value| value * forward);
        velocity[2] = time * gravity as f32;
        npc.player.set_velocity(velocity);
        npc.flags |= FL_NO_KNOCKBACK;
        npc.mind.states.jump_state = jump::JUMPING;
    }

    /// `NPC_BSRemove` (`NPC_behavior.c:941-958`): out of client 0's potentially visible set
    /// it fires its `target3`, vanishes (undrawn, invisible, no contents, no health, no
    /// name) and is freed at the next think.
    pub fn bs_remove(&mut self, me: usize, command: &mut UserCommand) {
        self.update_angles(me, true, true, command);
        let client_zero = self.body(0).map_or([0.0; 3], |player| player.origin);
        if self
            .host
            .in_pvs(self.actors[me].current_origin, client_zero)
        {
            return;
        }
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if let Some(target) = npc.target3.clone() {
            self.fired.push(target);
        }
        let npc = &mut self.actors[me];
        let flags = npc.state.raw_field(ES_EFLAGS).unwrap_or(0);
        npc.state.set_raw_field(ES_EFLAGS, flags | EF_NODRAW);
        npc.state.set_raw_field(ES_TYPE, ET_INVISIBLE);
        npc.contents = 0;
        npc.health = 0;
        npc.targetname = None;
        npc.think = NpcThink::Free(level_time + FRAMETIME);
    }

    /// `NPC_BSNoClip` (`NPC_behavior.c:1181-1213`): straight at the goal in every axis
    /// (the NPC flies, `client->noclip`), or — no goal — stopped.
    pub fn bs_noclip(&mut self, me: usize, command: &mut UserCommand) {
        if self.update_goal(me, command).is_some() {
            let goal = self.goal_origin(me).unwrap_or([0.0; 3]);
            let npc = &mut self.actors[me];
            let mut dir = subtract(goal, npc.current_origin);
            npc.desired_yaw = vector_angles(dir)[1];
            let (forward, right) = crate::pmove::flight::flight_axes(npc.mind.current_angles);
            let (forward, right) = (forward.to_array(), right.to_array());
            crate::saber_clash::normalize(&mut dir);
            let dot =
                |axis: [f32; 3]| (axis[0] * dir[0] + axis[1] * dir[1] + axis[2] * dir[2]) * 127.0;
            command.forward_move = f64::from(dot(forward)).floor() as i8;
            command.right_move = f64::from(dot(right)).floor() as i8;
            command.up_move = f64::from(dot([0.0, 0.0, 1.0])).floor() as i8;
        } else {
            self.actors[me].player.set_velocity([0.0; 3]);
        }
        self.update_angles(me, true, true, command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_names_a_behaviour_state_in_any_case() {
        assert_eq!(behavior_state_named("BS_WANDER"), Some(bstate::WANDER));
        assert_eq!(behavior_state_named("bs_default"), Some(bstate::DEFAULT));
        assert_eq!(
            behavior_state_named("BS_CINEMATIC"),
            Some(bstate::CINEMATIC)
        );
        // The states past `BS_CINEMATIC` are the game's own; a script cannot name them.
        assert_eq!(behavior_state_named("BS_HUNT_AND_KILL"), None);
        assert_eq!(behavior_state_named("mp/wander"), None);
    }
}
