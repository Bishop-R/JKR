//! jaPRO's gun game on this server ([`jkr_game_jka::gun_game`]; `g_gunGame`: 0 off, 1 on,
//! 2 or more also refusing `kill` outside the duel and team games): each spawn's rung,
//! each kill's next one, the fire mode and a saber rung's speed at every think, the
//! team's shared ladder, and the win — told to everyone, everyone back on the first rung,
//! the scores reset by jaPRO's `resetScores`, which is also a console command. Nothing is
//! dropped at a death and a pull disarms no one; unlike jaPRO, which loses it, a carried
//! flag still drops. Not in siege, whose classes arm their players.

use super::*;
use jkr_game_jka::gun_game::{self, FireMode, Rung};

const GUN_GAME: &[u8] = b"g_gunGame";
const GT_CTF: i32 = 8;
const FP_SABER_THROW: usize = 17;
const WP_SABER: u8 = 3;
const PS_KNOWN: usize = 51;
const PS_ZOOM_MODE: usize = 90;
const STAT_HEALTH: usize = 0;
/// The `persistant` counters `resetScores` clears: the score, the hits, the deaths and
/// the awards and captures.
const RESET_PERSISTANTS: [usize; 9] = [0, 1, 8, 9, 10, 11, 12, 13, 14];

impl NativeGame {
    /// `g_gunGame`, archived and announced, off (jaPRO's).
    pub(super) fn register_gun_game(&mut self) {
        let about: &[u8] =
            b"jaPRO: gun game, a weapon ladder climbed by kills (2: no kill command)";
        self.cvars
            .get_announced(GUN_GAME, b"0", crate::cvars::CVAR_ARCHIVE, Some(about));
    }

    /// Whether the gun game runs on this level.
    pub(super) fn gun_game_runs(&self) -> bool {
        self.gun_game != 0 && self.gametype != GAMETYPE_SIEGE
    }

    /// The rung `client` spawns on while the gun game runs.
    pub(super) fn gun_game_rung(&self, client: usize) -> Option<Rung> {
        if !self.gun_game_runs() {
            return None;
        }
        // A win resets every score at once, so nobody spawns past the last rung.
        gun_game::rung(self.gun_game_score(client)).or_else(|| gun_game::rung(18))
    }

    /// `G_GiveGunGameWeapon`'s score: a player's kills less its suicides; in the team
    /// games its team's score less them.
    fn gun_game_score(&self, client: usize) -> i32 {
        let Some(peer) = self.peer(client) else {
            return 0;
        };
        let (team, stats) = (peer.session.team, peer.stats);
        let team_game = matches!(self.gametype, GAMETYPE_TEAM | GT_CTF);
        if team_game && (1..=2).contains(&team) {
            self.team_scores[(team - 1) as usize] - stats.suicides
        } else {
            stats.kills - stats.suicides
        }
    }

    /// `player_die`'s kill by `killer` (another player's or an NPC's death, the normal
    /// branch): counted, and while the game runs the next rung, and a living killer
    /// healed to 100.
    pub(super) fn gun_game_kill(&mut self, killer: usize) {
        let Some(peer) = self.peer_mut(killer) else {
            return;
        };
        peer.stats.kills += 1;
        if !self.gun_game_runs() {
            return;
        }
        self.arm_gun_game(killer);
        if let Some(peer) = self.peer_mut(killer)
            && peer.health > 0
            && peer.health < 100
        {
            peer.health = 100;
            peer.state.stats[STAT_HEALTH] = 100;
        }
    }

    /// `player_die`'s `ps.fd.suicides`: `client` killed itself.
    pub(super) fn gun_game_suicide(&mut self, client: usize) {
        if let Some(peer) = self.peer_mut(client) {
            peer.stats.suicides += 1;
        }
    }

    /// `G_GiveGunGameWeapon` for `client` mid-life: its rung's arms, shared with its team
    /// in the team games; past the last rung, the game won.
    fn arm_gun_game(&mut self, client: usize) {
        let Some(rung) = gun_game::rung(self.gun_game_score(client)) else {
            self.gun_game_won(client);
            return;
        };
        let team_game = matches!(self.gametype, GAMETYPE_TEAM | GT_CTF);
        let team = self.peer(client).map_or(0, |peer| peer.session.team);
        for other in 0..self.players.places() {
            let shares = other == client
                || (team_game
                    && self
                        .peer(other)
                        .is_some_and(|peer| peer.session.team == team));
            if shares {
                self.give_rung(other, rung);
            }
        }
    }

    /// A rung's arms on `client`: its weapon in hand, its ammunition, its fire mode, the
    /// saber throw on its rung alone, and no zoom (jaPRO leaves the killer's own).
    fn give_rung(&mut self, client: usize, rung: Rung) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        rung.arm(&mut peer.state);
        peer.gun_fire = rung.fire;
        let known = peer.state.raw_field(PS_KNOWN).unwrap_or(0) & !(1 << FP_SABER_THROW);
        peer.state.set_raw_field(
            PS_KNOWN,
            known | u32::from(rung.saber_throw) << FP_SABER_THROW,
        );
        if rung.saber_throw {
            peer.force.levels[FP_SABER_THROW] = 3;
        }
        peer.state.set_raw_field(PS_ZOOM_MODE, 0);
        peer.movement = peer.movement.reseeded(&peer.state);
    }

    /// Past the last rung: the winner told to everyone (its team named in the team
    /// games), everyone back on the first rung, and the scores reset.
    fn gun_game_won(&mut self, client: usize) {
        let Some((name, team)) = self
            .peer(client)
            .map(|peer| (peer.name.clone(), peer.session.team))
        else {
            return;
        };
        let winner = if matches!(self.gametype, GAMETYPE_TEAM | GT_CTF) {
            let team_name: &[u8] = match team {
                1 => b"RED",
                2 => b"BLUE",
                3 => b"SPECTATOR",
                _ => b"FREE",
            };
            [team_name, b" team (", &name, b")"].concat()
        } else {
            name
        };
        let line = [&winner[..], b"^3 won the gungame"].concat();
        self.told
            .push(Told::Everyone([&b"print \""[..], &line, b"\n\""].concat()));
        self.told.push(Told::Everyone(
            [&b"cp \""[..], &line, &[b'\n'; 12], b"\""].concat(),
        ));
        if let Some(first) = gun_game::rung(0) {
            for other in 0..self.players.places() {
                self.give_rung(other, first);
            }
        }
        self.reset_scores();
    }

    /// jaPRO's `resetScores` (`Svcmd_ResetScores_f`): every player's score, hits, deaths,
    /// awards, captures, accuracy and gun game counts back to zero, the teams' scores too,
    /// the ranks again, and everyone told.
    pub(super) fn reset_scores(&mut self) {
        for client in 0..self.players.places() {
            if let Some(peer) = self.peer_mut(client) {
                for index in RESET_PERSISTANTS {
                    peer.state.persistent[index] = 0;
                }
                peer.stats = Default::default();
                peer.accuracy = (0, 0);
            }
        }
        self.team_scores = [0; 2];
        self.calculate_ranks();
        self.told.push(Told::Everyone(
            b"print \"Scores have been reset.\n\"".to_vec(),
        ));
    }

    /// The think's part (`ClientThink_real`): the fire mode the rung allows, for the
    /// server's own move; and a saber rung that may swing runs 28% faster.
    pub(super) fn gun_game_think(&mut self, client: usize) {
        let runs = self.gun_game_runs();
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let fire = if runs { peer.gun_fire } else { FireMode::Both };
        peer.movement.set_forced_fire(fire);
        if !runs || !peer.playing() || peer.state.raw_field(37).unwrap_or(0) == 0 {
            return;
        }
        // `ps.speed = g_speed` then `*= 1.28f` for a saber rung that swings; `basespeed`
        // follows the speed.
        let fast = peer.state.weapon() == WP_SABER && peer.gun_fire != FireMode::Alternate;
        let speed = if fast {
            PLAYER_SPEED * gun_game::SABER_SPEED
        } else {
            PLAYER_SPEED
        };
        peer.state.set_speed(speed);
        peer.state.set_base_speed(speed as i32);
        peer.movement = peer.movement.reseeded(&peer.state);
    }
}
