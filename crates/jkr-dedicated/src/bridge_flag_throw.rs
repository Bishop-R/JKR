//! jaPRO's flag throwing on this server ([`jkr_game_jka::flag_throw`]): its options
//! (`g_allowFlagThrow`, `g_fixFlagSuicide`, both off as in jaPRO), `throwflag`, the
//! thrower's second before touching a flag again, and a flag that comes to rest in a
//! death pit going home.

use super::bridge_cheats::{ALIVE, NO_INTERMISSION};
use super::*;
use jkr_game_jka::flag_throw::{self, FlagRules};

const ALLOW_FLAG_THROW: &[u8] = b"g_allowFlagThrow";
const FIX_FLAG_SUICIDE: &[u8] = b"g_fixFlagSuicide";
/// `GT_CTF`: the only game `throwflag` works in here (jaPRO's neutral-flag modes are
/// not ported).
const GT_CTF: i32 = 8;
/// `CONTENTS_TRIGGER`: what the death-pit check meets.
const CONTENTS_TRIGGER: u32 = 0x400;

impl NativeGame {
    /// `g_allowFlagThrow` and `g_fixFlagSuicide`, archived and announced, off (jaPRO's).
    pub(super) fn register_flag_throw(&mut self) {
        let options: [(&[u8], &[u8]); 2] = [
            (
                ALLOW_FLAG_THROW,
                b"jaPRO: CTF carriers may throw the flag (throwflag)",
            ),
            (
                FIX_FLAG_SUICIDE,
                b"jaPRO: a suicide drops the flag instead of sending it home",
            ),
        ];
        for (name, about) in options {
            self.cvars
                .get_announced(name, b"0", crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }

    /// The flag options as they stand.
    pub(super) fn flag_rules(&self) -> FlagRules {
        FlagRules {
            throw: self.cvars.integer(ALLOW_FLAG_THROW) != 0,
            suicide_drops: self.cvars.integer(FIX_FLAG_SUICIDE) != 0,
        }
    }

    /// `throwflag` from `client` (`Cmd_Throwflag_f`, `CMD_ALIVE|CMD_NOINTERMISSION`):
    /// whether `command` was it. In CTF with throwing on, the flag carried is thrown, and
    /// its status is `FLAG_DROPPED` (`Team_CheckDroppedItem`); otherwise nothing happens.
    pub(super) fn flag_throw_command(
        &mut self,
        client: usize,
        command: &[u8],
        level_time: i32,
    ) -> bool {
        if !command.eq_ignore_ascii_case(b"throwflag") {
            return false;
        }
        if !self.passes_gates(client, command, NO_INTERMISSION | ALIVE)
            || self.gametype != GT_CTF
            || !self.flag_rules().throw
        {
            return true;
        }
        let Some(peer) = self.peer_mut(client) else {
            return true;
        };
        let Some(thrown) = flag_throw::throw_flag(&mut peer.state, peer.entity.state(), level_time)
        else {
            return true;
        };
        peer.last_flag_throw = level_time;
        peer.movement = peer.movement.reseeded(&peer.state);
        let item = thrown.item;
        if let Some(number) = self.pool.spawn_entity(thrown.state.clone(), level_time) {
            self.pool.set_bounds(number, thrown.bounds);
            self.items.push((number, thrown));
            self.flag_launched(item, level_time);
        }
        true
    }

    /// `Touch_Item` under throwing: whether `client` threw a flag too recently to take
    /// one with `tag`.
    pub(super) fn flag_locked_out(&self, client: usize, tag: i32, level_time: i32) -> bool {
        self.peer(client).is_some_and(|peer| {
            flag_throw::locked_out(self.flag_rules(), tag, peer.last_flag_throw, level_time)
        })
    }

    /// `G_BounceItem`'s overboard check for flags that came to rest this frame
    /// (`landed`): one inside a death pit's trigger (`trigger_hurt` with `dmg -1`) goes
    /// home at the next frame instead of in thirty seconds. jaPRO checks every dropped
    /// flag; here only while throwing is on, so a stock game keeps stock's wait.
    pub(super) fn flags_in_death_pits(&mut self, landed: &[EntityId], level_time: i32) {
        if landed.is_empty() || !self.flag_rules().throw {
            return;
        }
        let Some(map) = self.map.as_ref() else {
            return;
        };
        for (number, pickup) in &mut self.items {
            if !landed.contains(number) || !in_death_pit(map, &self.triggers, pickup) {
                continue;
            }
            if let Some(dropped) = pickup.dropped.as_mut() {
                dropped.free_at = level_time;
            }
        }
    }
}

/// Whether `pickup`'s box overlaps a linked death-pit trigger's brushes (the reference's
/// trace against `CONTENTS_TRIGGER`, from where it rests to the same place).
fn in_death_pit(
    map: &LoadedMap,
    triggers: &[(EntityId, jkr_game_jka::triggers::Hurt)],
    pickup: &Pickup,
) -> bool {
    let half = (pickup.bounds.1[0] - pickup.bounds.0[0]) / 2.0;
    let Ok(bounds) = jkr_bsp::Aabb::new([-half; 3], [half; 3]) else {
        return false;
    };
    triggers.iter().any(|(_, trigger)| {
        trigger.linked
            && trigger.damage == -1
            && map
                .bsp
                .trace_model_box(
                    trigger.model,
                    pickup.origin,
                    pickup.origin,
                    bounds,
                    CONTENTS_TRIGGER,
                )
                .start_solid
    })
}
