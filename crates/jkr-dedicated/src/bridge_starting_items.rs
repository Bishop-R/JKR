//! jaPRO's `g_startingItems` on this server: the holdable items every player spawns with
//! (a bit per `HI_*`; none by default, as in stock), handed to everyone at once when the
//! option changes (`CVU_StartingItems`); its extra bits — the `jetpack` command's toggle
//! (bit 12) and the heal on a kill (bit 15, full armour with bit 14) — and `jetpack`.

use super::bridge_cheats::{ALIVE, NO_INTERMISSION};
use super::*;

const STARTING_ITEMS: &[u8] = b"g_startingItems";
/// `HI_SEEKER` to `HI_CLOAK`: the holdables the option hands out.
const HOLDABLES: u32 = 0b1111_1111_1110;
/// `HI_JETPACK`.
const JETPACK: u32 = 1 << 7;
/// `1 << HI_NUM_HOLDABLE`: `jetpack` may give one.
const JETPACK_TOGGLE: u32 = 1 << 12;
/// `1 << (HI_NUM_HOLDABLE + 2)`, `+ 3`: a kill heals the killer, its armour full too.
const FULL_ARMOUR_ON_KILL: u32 = 1 << 14;
const HEAL_ON_KILL: u32 = 1 << 15;
const STAT_HEALTH: usize = 0;
const STAT_HOLDABLE_ITEM: usize = 1;
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_ARMOR: usize = 5;

/// `GiveClientItems`: the holdables `items` names, none selected.
pub(super) fn give_starting_items(state: &mut jkr_protocol::PlayerState, items: u32) {
    state.stats[STAT_HOLDABLE_ITEMS] = items & HOLDABLES;
    state.stats[STAT_HOLDABLE_ITEM] = 0;
}

impl NativeGame {
    /// `g_startingItems`, archived and announced, 0 (jaPRO's).
    pub(super) fn register_starting_items(&mut self) {
        let about: &[u8] = b"jaPRO: holdables every player spawns with, a bit per item";
        self.cvars.get_announced(
            STARTING_ITEMS,
            b"0",
            crate::cvars::CVAR_ARCHIVE,
            Some(about),
        );
    }

    /// The option as it stands.
    pub(super) fn starting_items(&self) -> u32 {
        self.cvars.integer(STARTING_ITEMS) as u32
    }

    /// `CVU_StartingItems`: everyone in the game handed the items again.
    pub(super) fn starting_items_changed(&mut self) {
        let items = self.starting_items();
        for client in 0..self.players.places() {
            if let Some(peer) = self.peer_mut(client)
                && peer.begun
            {
                give_starting_items(&mut peer.state, items);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
    }

    /// `player_die`'s heal under the option's bit 15: a killer below 100 health is put
    /// back to 100, with a quarter of that in armour (or 100 with bit 14).
    pub(super) fn kill_heal(&mut self, killer: usize) {
        let items = self.starting_items();
        if items & HEAL_ON_KILL == 0 {
            return;
        }
        if let Some(peer) = self.peer_mut(killer)
            && peer.health > 0
            && peer.health < 100
        {
            peer.health = 100;
            peer.state.stats[STAT_HEALTH] = 100;
            peer.state.stats[STAT_ARMOR] = if items & FULL_ARMOUR_ON_KILL != 0 {
                100
            } else {
                25
            };
        }
    }

    /// jaPRO's `jetpack` (`Cmd_Jetpack_f`, `CMD_NOINTERMISSION|CMD_ALIVE`): one held is
    /// taken away; otherwise one is given where players spawn with it or may toggle it
    /// (not in a duel). Whether `command` was it.
    pub(super) fn jetpack_command(&mut self, client: usize, command: &[u8]) -> bool {
        if !command.eq_ignore_ascii_case(b"jetpack") {
            return false;
        }
        if !self.passes_gates(client, command, NO_INTERMISSION | ALIVE) {
            return true;
        }
        let items = self.starting_items();
        let Some(peer) = self.peer_mut(client) else {
            return true;
        };
        let held = peer.state.stats[STAT_HOLDABLE_ITEMS];
        if held & JETPACK != 0 {
            peer.state.stats[STAT_HOLDABLE_ITEMS] = held & !JETPACK;
            // Not held, not flying.
            peer.gear.packs.jetpack_on = false;
        } else if items & (JETPACK | JETPACK_TOGGLE) != 0 {
            if peer.state.duel_in_progress() {
                self.told.push(Told::One(
                    client,
                    b"print \"Command not allowed in duel. (jetpack).\n\"".to_vec(),
                ));
                return true;
            }
            peer.state.stats[STAT_HOLDABLE_ITEMS] = held | JETPACK;
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        true
    }
}
