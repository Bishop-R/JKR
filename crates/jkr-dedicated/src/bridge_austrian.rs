//! Stock's `g_austrian` duel log, named for the Austrian league it was written for: each
//! duel kill's details, a round's result, the next round's duellists and the tournament's
//! winner, in the game log (`g_combat.c:2473-2490`, `g_main.c:1676-1770`, `2459-2472`).

use super::{GAMETYPE_DUEL, GAMETYPE_POWERDUEL, NativeGame};
use jkr_game_jka::means_of_death::MOD_SABER;

/// `PERS_SCORE`, `PERS_HITS`.
const PERS_SCORE: usize = 0;
const PERS_HITS: usize = 1;

impl NativeGame {
    fn austrian(&self) -> bool {
        self.cvars.integer(b"g_austrian") != 0
    }

    /// A client's name and record as the log writes them: `name wins/losses`.
    fn austrian_record(&self, client: u16) -> String {
        self.peer(usize::from(client))
            .map_or_else(String::new, |peer| {
                format!(
                    "{} {}/{}",
                    String::from_utf8_lossy(&peer.name),
                    peer.session.wins,
                    peer.session.losses
                )
            })
    }

    /// "Duel Kill Details" for a duel's kill of `victim` (`g_combat.c:2473-2490`): the time
    /// since the later of the two spawns, both hit counters, the killer's health, and for a
    /// saber kill its style and torso animation.
    pub(super) fn austrian_kill(
        &mut self,
        victim: usize,
        attacker: Option<u16>,
        means: u32,
        level_time: i32,
    ) {
        if !self.austrian() || self.gametype != GAMETYPE_DUEL {
            return;
        }
        let (sorted, playing) = self.sorted();
        if playing < 2 {
            return;
        }
        let spawned = |client: u16| {
            self.peer(usize::from(client))
                .map_or(0, |peer| peer.mortality.respawn_time)
        };
        let spawn_time = spawned(sorted[0]).max(spawned(sorted[1]));
        let Some(dead) = self.peer(victim) else {
            return;
        };
        let mut lines = vec![
            "Duel Kill Details:\n".to_owned(),
            format!("Kill Time: {}\n", level_time - spawn_time),
            format!(
                "victim: {}, hits on enemy {}\n",
                String::from_utf8_lossy(&dead.name),
                dead.state.persistent[PERS_HITS] as i32
            ),
        ];
        if let Some(killer) = attacker.and_then(|attacker| self.peer(usize::from(attacker))) {
            lines.push(format!(
                "killer: {}, hits on enemy {}, health: {}\n",
                String::from_utf8_lossy(&killer.name),
                killer.state.persistent[PERS_HITS] as i32,
                killer.health
            ));
            if means == MOD_SABER {
                let anim = usize::from(killer.state.torso_animation());
                let name = jkr_game_jka::legacy_animation::NAMES
                    .get(anim)
                    .unwrap_or(&"");
                lines.push(format!(
                    "killer saber style: {}, killer saber anim {name}\n",
                    killer.state.saber_style()
                ));
            }
        }
        for line in lines {
            self.log(&line);
        }
    }

    /// "Duel Results" two seconds into a duel's intermission (`g_main.c:1678-1693`): the
    /// winner's and the loser's score and record.
    pub(super) fn austrian_results(&mut self) {
        if !self.austrian() || self.gametype != GAMETYPE_DUEL {
            return;
        }
        let (sorted, _) = self.sorted();
        let line = |role: &str, client: Option<&u16>| {
            let client = usize::from(*client?);
            let peer = self.peer(client)?;
            Some(format!(
                "{role}: {}, score: {}, wins/losses: {}/{}\n",
                String::from_utf8_lossy(&peer.name),
                peer.state.persistent[PERS_SCORE] as i32,
                peer.session.wins,
                peer.session.losses
            ))
        };
        let lines: Vec<String> = [line("winner", sorted.first()), line("loser", sorted.get(1))]
            .into_iter()
            .flatten()
            .collect();
        self.log("Duel Results:\n");
        for line in lines {
            self.log(&line);
        }
    }

    /// "Duel Initiated" (or "Power Duel Initiated") as the next round is set up
    /// (`g_main.c:1716-1742`): the duellists' records and the kill limit.
    pub(super) fn austrian_initiated(&mut self) {
        if !self.austrian() {
            return;
        }
        let (sorted, _) = self.sorted();
        let records: Vec<String> = sorted
            .iter()
            .take(3)
            .map(|client| self.austrian_record(*client))
            .collect();
        let fraglimit = self.limits.fraglimit;
        let line = match (self.gametype, records.as_slice()) {
            (GAMETYPE_POWERDUEL, [one, two, three, ..]) => format!(
                "Power Duel Initiated: {one} vs {two} and {three}, kill limit: {fraglimit}\n"
            ),
            (GAMETYPE_DUEL, [one, two, ..]) => {
                format!("Duel Initiated: {one} vs {two}, kill limit: {fraglimit}\n")
            }
            _ => return,
        };
        self.log(&line);
    }

    /// "Duel Tournament Winner" as the level ends past the duel limit (`g_main.c:1767-1772`).
    pub(super) fn austrian_winner(&mut self) {
        if !self.austrian() || self.gametype != GAMETYPE_DUEL {
            return;
        }
        let (sorted, _) = self.sorted();
        let Some(peer) = sorted
            .first()
            .and_then(|client| self.peer(usize::from(*client)))
        else {
            return;
        };
        let line = format!(
            "Duel Tournament Winner: {} wins/losses: {}/{}\n",
            String::from_utf8_lossy(&peer.name),
            peer.session.wins,
            peer.session.losses
        );
        self.log(&line);
    }

    /// `CheckTournament`'s line as a power duel's third player is pulled in
    /// (`g_main.c:2459-2472`): "Duel Initiated" with all three records.
    pub(super) fn austrian_power_duel_pulled(&mut self) {
        if !self.austrian() {
            return;
        }
        let (sorted, _) = self.sorted();
        let records: Vec<String> = sorted
            .iter()
            .take(3)
            .map(|client| self.austrian_record(*client))
            .collect();
        if let [one, two, three] = records.as_slice() {
            let fraglimit = self.limits.fraglimit;
            self.log(&format!(
                "Duel Initiated: {one} vs {two} and {three}, kill limit: {fraglimit}\n"
            ));
        }
    }
}
