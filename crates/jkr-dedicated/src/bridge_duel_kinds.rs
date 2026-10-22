//! jaPRO's duel keys on this server (`Cmd_ForceDuel_f`, `Cmd_GunDuel_f`, `g_cmds.c:4147-4225`
//! of jaPRO): `engage_fullforceduel` challenges to a Force duel, `engage_gunduel [weapon]`
//! (with `g_allowGunDuel`) to a duel with the weapon named or held; and the re-arming at
//! the end of a duel of every weapon (`GiveClientWeapons`). The duels themselves are
//! [`jkr_game_jka::duel`]'s.

use super::bridge_duel_options::ALLOW_GUN_DUEL;
use super::*;
use jkr_game_jka::duel::DuelKind;

/// `STAT_WEAPONS`, `FP_SABER_OFFENSE`; `AMMO_BLASTER` .. `AMMO_DETPACK`, which a spawn
/// leaves at nothing but the blaster's 100.
const STAT_WEAPONS: usize = 4;
const FP_SABER_OFFENSE: usize = 15;
const AMMO: std::ops::RangeInclusive<usize> = 2..=9;
const AMMO_BLASTER: usize = 2;
const SPAWN_BLASTER_AMMO: u32 = 100;

impl NativeGame {
    /// jaPRO's duel keys from `client` (`CMD_NOINTERMISSION`); whether `text` was one.
    pub(super) fn japro_duel_command(
        &mut self,
        client: usize,
        text: &[u8],
        level_time: i32,
    ) -> bool {
        let words: Vec<&[u8]> = text
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty())
            .collect();
        let Some(&command) = words.first() else {
            return false;
        };
        let force = command.eq_ignore_ascii_case(b"engage_fullforceduel");
        if !force && !command.eq_ignore_ascii_case(b"engage_gunduel") {
            return false;
        }
        if self.refused_at_intermission(client, command) {
            return true;
        }
        if force {
            self.duel_key(client, level_time, DuelKind::Force);
            return true;
        }
        if self.cvars.integer(ALLOW_GUN_DUEL) == 0 {
            self.told.push(Told::One(
                client,
                b"print \"Command not allowed. (gunduel).\n\"".to_vec(),
            ));
            return true;
        }
        if words.len() > 2 {
            self.told.push(Told::One(
                client,
                b"print \"Usage: /engage_gunduel or engage_gunduel <weapon name>.\n\"".to_vec(),
            ));
            return true;
        }
        let in_hand = self.peer(client).map_or(0, |peer| peer.state.weapon());
        let kind = DuelKind::gun(words.get(1).copied(), u32::from(in_hand));
        self.duel_key(client, level_time, kind);
        true
    }

    /// `GiveClientWeapons` after a duel of every weapon: the weapons a spawn gives, and of
    /// the ammunition only the blaster's that a spawn gives.
    pub(super) fn rearm_after_duel(&mut self, client: usize) {
        let (gametype, disabled) = (self.gametype, self.settings.weapons_disabled);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (owned, _) = jkr_game_jka::client_spawn::spawn_weapons(
            gametype,
            disabled,
            peer.force.levels[FP_SABER_OFFENSE],
        );
        peer.state.stats[STAT_WEAPONS] = owned;
        for index in AMMO {
            peer.state.ammo[index] = 0;
        }
        peer.state.ammo[AMMO_BLASTER] = SPAWN_BLASTER_AMMO;
        peer.movement = peer.movement.reseeded(&peer.state);
    }
}
