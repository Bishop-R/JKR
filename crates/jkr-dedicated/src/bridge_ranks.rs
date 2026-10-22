//! The ranking: `CalculateRanks` over everyone connected, and the scoreboard's view of
//! each player.

use super::*;

impl NativeGame {
    /// Everyone connected, as the exit rules see them. Sorted best-first, as
    /// `level.sortedClients` is, because the tie is decided on the top two.
    pub(super) fn end_contenders(&self, into: &mut Vec<jkr_game_jka::match_end::Contender>) {
        into.clear();
        let Some(world) = self.server.world(self.world) else {
            return;
        };
        for (client, handle) in self.players.holders().enumerate() {
            let Some(peer) = handle.and_then(|handle| world.entity(handle)) else {
                continue;
            };
            into.push(jkr_game_jka::match_end::Contender {
                client,
                team: peer.session.team,
                score: peer.state.persistent[PERS_SCORE] as i32,
                connected: peer.begun,
                // A bot is never waited for at an intermission.
                bot: peer.bot,
                ready: peer.ready_to_exit,
                loser: peer.loser,
                wins: peer.session.wins,
            });
        }
        into.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then(left.client.cmp(&right.client))
        });
    }

    /// `CalculateRanks`: the order of everyone connected, each playing client's rank,
    /// and the three strings the scoreboard reads, published when they change.
    pub(super) fn calculate_ranks(&mut self) {
        let contenders = self.contenders();
        let ranks = calculate_ranks_in(&contenders, self.standings());
        let Self {
            server,
            world,
            players,
            told,
            config_strings,
            ..
        } = self;
        if let Some(world) = server.world_mut(*world) {
            for (client, rank) in ranks.ranks {
                if let Some(peer) = players
                    .at(usize::from(client))
                    .and_then(|handle| world.entity_mut(handle))
                {
                    peer.state.persistent[PERS_RANK] = rank;
                }
            }
        }
        for (index, value) in ranks.strings {
            let slot = config_strings.binary_search_by_key(&index, |(index, _)| *index);
            let previous = match slot {
                Ok(at) => std::mem::replace(&mut config_strings[at].1, value.clone()),
                Err(at) => {
                    config_strings.insert(at, (index, value.clone()));
                    Vec::new()
                }
            };
            if previous != value {
                told.push(Told::ConfigString {
                    index,
                    previous,
                    value,
                });
            }
        }
        self.warmup_after_ranks();
        // `CalculateRanks` ends with `CheckExitRules` (`g_main.c:1192-1193`): the frag
        // that reaches the limit ends the match at once, not at the next frame. The
        // guard keeps `exit_level`'s own ranking from recurring into this.
        self.run_match_end(self.last_frame_time);
    }

    /// `level.numPlayingClients` without the ranks' allocations: the begun clients in the
    /// game, or out of it as a duel's loser waiting its turn (as
    /// [`jkr_game_jka::ranks::calculate_ranks_in`] counts them).
    pub(super) fn playing_clients(&self) -> usize {
        let Some(world) = self.server.world(self.world) else {
            return 0;
        };
        self.players
            .holders()
            .filter_map(|handle| world.entity(handle?))
            .filter(|peer| {
                peer.begun && (peer.session.team != i32::from(TEAM_SPECTATOR) || peer.loser)
            })
            .count()
    }

    /// How long the items a pickup leaves stay away (`Pickup_*`, `adjustRespawnTime`):
    /// `g_weaponRespawn`, Team FFA's slow weapons, and `g_adaptRespawn`'s playing clients.
    pub(super) fn item_respawns(&self) -> jkr_game_jka::items::Respawns {
        let adapt = self.cvars.integer(b"g_adaptRespawn") != 0;
        jkr_game_jka::items::Respawns {
            weapon: self.cvars.integer(b"g_weaponRespawn"),
            team_weapons: self.gametype == GAMETYPE_TEAM,
            adapt_players: adapt.then(|| self.playing_clients()),
        }
    }

    /// Everyone connected, as the ranking and the scoreboard see them.
    pub(super) fn contenders(&self) -> Vec<Contender> {
        let Some(world) = self.server.world(self.world) else {
            return Vec::new();
        };
        // jaPRO's `g_fakeClients`: a bot shows a ping of 25 to 40 (`Q_irand(25, 40)` per
        // scoreboard there; here it moves once a second, without the game's generator).
        let fake_pings = self.cvars.integer(b"g_fakeClients") != 0;
        let second = self.last_frame_time / 1_000;
        self.players
            .holders()
            .enumerate()
            .filter_map(|(client, handle)| {
                let peer = world.entity(handle?)?;
                Some(Contender {
                    client: client as u16,
                    team: peer.session.team,
                    connecting: !peer.begun,
                    spectator_number: peer.session.spectator_num,
                    special: peer.session.spectator_state
                        == jkr_game_jka::client_begin::SPECTATOR_SCOREBOARD
                        || peer.session.spectator_client < 0,
                    lone: peer.session.duel_team == jkr_game_jka::power_duel::DUELTEAM_LONE
                        && peer.session.team != i32::from(TEAM_SPECTATOR),
                    loser: peer.loser,
                    persistant: peer.state.persistent.map(|value| value as i32),
                    ping: if fake_pings && peer.bot && peer.begun {
                        25 + (client as i32 * 7 + second).rem_euclid(16)
                    } else {
                        peer.ping
                    },
                    enter_time: peer.enter_time,
                    powerups: 0,
                    accuracy: peer.accuracy,
                })
            })
            .collect()
    }
}
