//! jaPRO's statistics on this server ([`jkr_game_jka::japro_stats`]): damage counted as
//! jaPRO counts it — in team FFA, CTF and the gun game, outside duels, capped at what the
//! victim had — and `printStats`, the table for whoever asks (in team FFA, CTF, the gun
//! game or under a frag limit). Kills, suicides, team kills and the Force's tallies are
//! counted where they happen. jaPRO also prints the table to everyone at every match's
//! end; here, as in stock, nothing is printed then.

use super::bridge_cheats::NO_INTERMISSION;
use super::*;
use jkr_game_jka::damage::Attacker;
use jkr_game_jka::japro_stats::{self, Columns, Row};

const GT_CTF: i32 = 8;
const GT_CTY: i32 = 9;
const STAT_ARMOR: usize = 5;
/// `PERS_SCORE`, `PERS_KILLED`.
const PERS_SCORE: usize = 0;
const PERS_KILLED: usize = 8;
/// `FP_TEAM_HEAL`, `FP_TEAM_FORCE`, `FP_DRAIN`.
const FP_TEAM_HEAL: u32 = 11;
const FP_TEAM_FORCE: u32 = 12;
const FP_DRAIN: u32 = 13;
/// `WP_CONCUSSION`, past which `g_weaponDisable` leaves only the saber.
const WP_CONCUSSION: u32 = 15;

impl NativeGame {
    /// Whether jaPRO counts damage now: team FFA, CTF or the gun game.
    fn damage_counted(&self) -> bool {
        matches!(self.gametype, GAMETYPE_TEAM | GT_CTF) || self.gun_game_runs()
    }

    /// A player's health and armour before a blow, for [`Self::count_damage`].
    pub(super) fn health_and_armor(&self, client: usize) -> (i32, i32) {
        self.peer(client).map_or((0, 0), |peer| {
            (peer.health, peer.state.stats[STAT_ARMOR] as i32)
        })
    }

    /// `G_Damage`'s statistics for a blow that did `dealt` (health and armour) on player
    /// `target`, who had `health` and `armor` before it: the attacker's damage given (its
    /// team damage for a teammate, itself included), the target's damage taken unless a
    /// teammate dealt it. Between two duellists only the attacker's duel damage, in any
    /// game type (`duelDamageGiven`).
    pub(super) fn count_damage(
        &mut self,
        attacker: Option<Attacker>,
        target: usize,
        dealt: i32,
        (health, armor): (i32, i32),
    ) {
        // A body (jaPRO counts it, against the attacker, by its negative health) is left out.
        if dealt <= 0 || health <= 0 {
            return;
        }
        let counted = japro_stats::counted(dealt, health, armor);
        let player = attacker
            .filter(|attacker| !attacker.npc)
            .map(|attacker| usize::from(attacker.client));
        let duellists = player.is_some_and(|attacker| {
            self.peer(attacker)
                .is_some_and(|peer| peer.state.duel_in_progress())
                && self
                    .peer(target)
                    .is_some_and(|peer| peer.state.duel_in_progress())
        });
        if duellists {
            if let Some(peer) = player.and_then(|attacker| self.peer_mut(attacker)) {
                peer.duel_record.damage_given += counted;
            }
            return;
        }
        if !self.damage_counted() {
            return;
        }
        let teammates = player.is_some_and(|attacker| self.players_on_same_team(target, attacker));
        if let Some(attacker) = player
            && let Some(peer) = self.peer_mut(attacker)
        {
            if teammates {
                peer.stats.team_damage_given += counted;
            } else {
                peer.stats.damage_given += counted;
            }
        }
        if !teammates && let Some(peer) = self.peer_mut(target) {
            peer.stats.damage_taken += counted;
        }
    }

    /// The same for a player's blow on an NPC, which had `health` and `armor`: the
    /// player's damage given.
    pub(super) fn count_npc_damage(
        &mut self,
        attacker: Option<Attacker>,
        dealt: i32,
        (health, armor): (i32, i32),
    ) {
        if !self.damage_counted() || dealt <= 0 || health <= 0 {
            return;
        }
        let Some(attacker) = attacker.filter(|attacker| !attacker.npc) else {
            return;
        };
        if let Some(peer) = self.peer_mut(usize::from(attacker.client)) {
            peer.stats.damage_given += japro_stats::counted(dealt, health, armor);
        }
    }

    /// `printStats` from `client` (`Cmd_PrintStats_f`, not at the intermission): the table,
    /// in team FFA, CTF, the gun game or under a frag limit. Whether `command` was it.
    pub(super) fn print_stats_command(&mut self, client: usize, command: &[u8]) -> bool {
        if !command.eq_ignore_ascii_case(b"printstats") {
            return false;
        }
        if !self.passes_gates(client, command, NO_INTERMISSION) {
            return true;
        }
        let allowed = matches!(self.gametype, GAMETYPE_TEAM | GT_CTF)
            || self.gun_game != 0
            || self.limits.fraglimit != 0;
        if !allowed {
            self.told.push(Told::One(
                client,
                b"print \"Command only allowed in TFFA, CTF, Gun Game, or when there is a fraglimit. (printStats).\n\"".to_vec(),
            ));
            return true;
        }
        for print in self.stats_table() {
            self.told.push(Told::One(client, print));
        }
        true
    }

    /// `PrintStats`' table as the server stands: everyone in the game (not a spectator,
    /// nor free in a team game), in slot order.
    fn stats_table(&self) -> Vec<Vec<u8>> {
        let integer = |name: &[u8]| self.cvars.integer(name) as u32;
        let (weapons_disabled, powers_disabled) =
            (integer(b"g_weaponDisable"), integer(b"g_forcePowerDisable"));
        let gun_game = self.gun_game != 0;
        let friendly_fire = self.force_rules.friendly_fire;
        let team_powers_off = powers_disabled & (1 << FP_TEAM_HEAL) != 0
            && powers_disabled & (1 << FP_TEAM_FORCE) != 0;
        let columns = Columns {
            net: self.gametype != GT_CTF,
            team_damage: self.gametype == GAMETYPE_TEAM && friendly_fire,
            ctf: matches!(self.gametype, GT_CTF | GT_CTY),
            // jaPRO hides it on a saber-only server (every gun disabled, the saber the
            // whole loadout); here the loadout is stock's.
            accuracy: weapons_disabled <= 1 << WP_CONCUSSION || gun_game,
            team_powers: !team_powers_off && !gun_game,
            drain: powers_disabled & (1 << FP_DRAIN) == 0 && friendly_fire && !gun_game,
            ctf_counting: self.gametype == GT_CTF,
        };
        let team_game = self.gametype >= jkr_game_jka::match_end::GT_TEAM;
        let level_time = self.last_frame_time;
        let names: Vec<(usize, Vec<u8>)> = (0..self.players.places())
            .filter_map(|client| {
                let peer = self.peer(client)?;
                let free = team_game && peer.session.team == crate::admin::TEAM_FREE;
                (!peer.name.is_empty() && peer.session.team != i32::from(TEAM_SPECTATOR) && !free)
                    .then(|| (client, peer.name.clone()))
            })
            .collect();
        let rows: Vec<Row<'_>> = names
            .iter()
            .filter_map(|(client, name)| {
                let peer = self.peer(*client)?;
                Some(Row {
                    score: peer.state.persistent[PERS_SCORE] as i32,
                    killed: peer.state.persistent[PERS_KILLED] as i32,
                    stats: peer.stats,
                    hits: peer.accuracy.0,
                    shots: peer.accuracy.1,
                    captures: peer.team_state.captures,
                    returns: peer.team_state.flag_recovery,
                    carrier_kills: peer.team_state.frag_carrier,
                    minutes: (level_time - peer.enter_time) / 60_000,
                    name,
                })
            })
            .collect();
        japro_stats::table(columns, &rows)
    }
}
