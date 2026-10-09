//! NPCs as script entities. An NPC that carries a spawn script becomes one as it begins:
//! its begin stops where `NPC_Begin` runs the script (`ICARUS_InitEnt`,
//! `G_ActivateBehavior(ent, BSET_SPAWN)` and a turn of its scripts,
//! `NPC_spawn.c:1107`, `1161-1165`), and the roster finishes the begin after it. From then
//! on its scripts take a turn every frame after the NPCs' own (`G_RunThink`'s,
//! `g_main.c:2840-2851`; the turn `NPC_Think` takes at its end is not taken twice), and
//! they go with the NPC (`ICARUS_FreeEnt`). It answers to its `NPC_targetname`.
//!
//! The sets on an NPC's behaviour states apply through the roster (`Q3_SetBState`,
//! `Q3_SetTempBState`, `Q3_SetDefaultBState`, and `G_ActivateBehavior` of a set naming a
//! state), and `remove` frees it; its other sets, client actions and `kill` are reported
//! once a level and complete at once. An NPC whose AI activates one of its behaviour sets
//! (`G_ActivateBehavior`: its pain, death, anger script ...) becomes one then, and the
//! script starts with the NPCs' scripts that frame. Before that an NPC without a spawn
//! script is no script entity, so no script reaches it by name — none of the stock maps'
//! scripts names an NPC.

use super::bridge_icarus::ScriptOwner;
use super::*;
use jkr_game_jka::icarus_set_table::{SET_BEHAVIOR_STATE, SET_DEFAULT_BSTATE, SET_TEMP_BSTATE};
use jkr_game_jka::npc_spawn::NpcThink;
use jkr_game_jka::script_entity::{BSET_SPAWN, EntityKind, ScriptEntity, Think};
use jkr_game_jka::script_runner;
use jkr_game_jka::script_world::{NpcAction, ScriptWorld};

impl NativeGame {
    /// `NPC_Begin`'s scripts for the NPC numbered `number`, whose begin stopped at its
    /// spawn script ([`NpcRoster::take_script_turn`]): a script entity named as the NPC,
    /// with the spawner's spawn script; its sequencer (any an earlier NPC of that number
    /// left freed first, as `G_InitGentity` does); the script started and given a turn.
    pub(super) fn npc_spawn_script(&mut self, number: u16) {
        let Some(ent) = self.npc_as_script_entity(number) else {
            return;
        };
        self.with_scripts(|world, icarus| {
            let owner = ScriptOwner::Npc(number);
            if world.entity(owner).is_some() {
                script_runner::free_scripts(world, icarus, owner);
                world
                    .game
                    .scripts
                    .npcs
                    .retain(|(known, _)| *known != number);
            }
            world.game.scripts.npcs.push((number, ent));
            script_runner::init_entity(world, icarus, owner);
            if script_runner::activate_behavior(world, icarus, owner, BSET_SPAWN) {
                script_runner::maintain(world, icarus, owner);
            }
        });
    }

    /// The scripted NPCs' turn of their scripts this frame, after the NPCs' own; the
    /// scripts of an NPC that is gone go with it. A script's `remove` (`Q3_RemoveEnt`:
    /// `G_FreeEntity` for the NPC's think, a tenth of a second on) becomes the NPC's think.
    pub(super) fn run_npc_scripts(&mut self) {
        self.run_npc_behaviors();
        if self.scripts.npcs.is_empty() {
            return;
        }
        self.with_scripts(|world, icarus| {
            let mut index = 0;
            while let Some((number, ent)) = world.game.scripts.npcs.get_mut(index) {
                let (number, owner) = (*number, ScriptOwner::Npc(*number));
                let removed_at = (ent.think == Think::Free).then_some(ent.next_think);
                ent.think = Think::Nothing;
                let npc = world
                    .game
                    .npcs
                    .roster
                    .actors
                    .iter_mut()
                    .find(|npc| npc.number == number);
                if let Some(npc) = npc {
                    if let Some(at) = removed_at {
                        npc.think = NpcThink::Free(at);
                    }
                    script_runner::maintain(world, icarus, owner);
                    index += 1;
                } else {
                    script_runner::free_scripts(world, icarus, owner);
                    world.game.scripts.npcs.remove(index);
                }
            }
        });
    }

    /// NPC `number` as a script entity: named as the NPC (`NPC_targetname`), with every
    /// behaviour set it has.
    fn npc_as_script_entity(&self, number: u16) -> Option<ScriptEntity<ScriptOwner>> {
        let npc = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)?;
        let text = |bytes: &Option<Vec<u8>>| {
            bytes
                .as_deref()
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        };
        let name = text(&npc.targetname);
        let mut ent = ScriptEntity::new("NPC", EntityKind::Npc, npc.current_origin, [0.0; 3]);
        (ent.targetname, ent.script_targetname) = (name.clone(), name);
        for (set, script) in npc.behavior_sets.iter().enumerate() {
            ent.behavior_sets[set] = text(script);
        }
        Some(ent)
    }

    /// The scripts of the behaviour sets the NPCs activated (`G_ActivateBehavior`'s
    /// `ICARUS_RunScript`), started before the NPCs' scripts take their turn: an NPC not yet
    /// a script entity becomes one (`ICARUS_InitEnt`); one already gone is passed over.
    fn run_npc_behaviors(&mut self) {
        for (number, bset) in std::mem::take(&mut self.npcs.behaviors) {
            let known = self.scripts.npcs.iter().any(|(known, _)| *known == number);
            let fresh = if known {
                None
            } else {
                let Some(ent) = self.npc_as_script_entity(number) else {
                    continue;
                };

                Some(ent)
            };
            self.with_scripts(|world, icarus| {
                let owner = ScriptOwner::Npc(number);
                if let Some(ent) = fresh {
                    world.game.scripts.npcs.push((number, ent));
                    script_runner::init_entity(world, icarus, owner);
                }
                let _ = script_runner::activate_behavior(world, icarus, owner, bset);
            });
        }
    }

    /// A script's `action` on the NPC numbered `number`: whether its task completes at
    /// once.
    pub(super) fn script_npc_action(&mut self, number: u16, action: NpcAction) -> bool {
        let level_time = self.last_frame_time;
        match action {
            NpcAction::Set {
                set: SET_BEHAVIOR_STATE,
                value,
            } => self
                .with_roster(|roster, _, host| {
                    roster.script_set_bstate(number, &value, level_time, host)
                })
                .unwrap_or(true),
            NpcAction::Set {
                set: SET_TEMP_BSTATE,
                value,
            } => {
                self.npcs.roster.script_set_temp_bstate(number, &value);
                true
            }
            NpcAction::Set {
                set: SET_DEFAULT_BSTATE,
                value,
            } => {
                self.npcs.roster.script_set_default_bstate(number, &value);
                true
            }
            NpcAction::BehaviorState(name) => {
                self.npcs.roster.script_behavior_state(number, &name);
                true
            }
            NpcAction::Set { set, value } => {
                // The flag sets (`Q3_SetChaseEnemies` and the rest) apply; any other is named.
                let skill = self.integer_cvar(b"g_npcspskill", 0);
                if self.npcs.roster.script_set_flag(number, set, &value, skill) {
                    return true;
                }
                let name = jkr_game_jka::icarus_set_table::SET_TABLE
                    .iter()
                    .find(|(_, id)| *id == set)
                    .map_or("SET_?", |(name, _)| *name);
                self.scripts.report_once(&format!("npc {name}"), || {
                    format!("{name} \"{value}\" on an NPC: not applied by this server yet")
                });
                true
            }
        }
    }
}
