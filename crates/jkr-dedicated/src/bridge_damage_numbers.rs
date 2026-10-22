//! jaPRO's damage numbers on this server ([`crate::damage_numbers`]): the option, and a
//! blow's numbers sent to its attacker, to everyone or to the attacker's followers; a score
//! plum goes to the attacker alone (`ScorePlum`, `g_combat.c:436-446`). Blows on NPCs are
//! told as jaPRO tells them, an NPC going by its `targetname` — or by its type where jaPRO
//! prints an empty name or "(null)"; an NPC's own blows reach only the everyone line, also
//! named. Not told:
//! one NPC's blows on another (the roster deals them), and jaPRO's own client's opt-out is
//! not modelled.

use super::*;
use crate::damage_numbers;

/// The option's name.
const DAMAGE_NUMBERS: &[u8] = b"g_damageNumbers";
/// `EV_SCOREPLUM`; the plum's wire fields: whose it is (`otherEntityNum`) and its number
/// (`time`).
const EV_SCOREPLUM: u32 = 98;
const ES_OTHER_ENTITY_NUM: usize = 59;
const ES_TIME: usize = 65;
/// `TEAM_SPECTATOR`, `SPECTATOR_FOLLOW`.
const TEAM_SPECTATOR: i32 = 3;
const SPECTATOR_FOLLOW: i32 = 2;

/// Who dealt a blow the numbers tell of, or took it: a player by its place, or an NPC by its
/// entity number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Party {
    Player(usize),
    Npc(u16),
}

impl Party {
    /// A blow's attacker as `G_Damage` reads it.
    pub(super) fn attacker(attacker: jkr_game_jka::damage::Attacker) -> Self {
        if attacker.npc {
            Self::Npc(attacker.client)
        } else {
            Self::Player(usize::from(attacker.client))
        }
    }
}

impl NativeGame {
    /// `g_damageNumbers`, archived, off by default (jaPRO's).
    pub(super) fn register_damage_numbers(&mut self) {
        let about: &[u8] = b"jaPRO: damage numbers (1 line, 2 total, 3 plum, 4 chat, 5 1+2, 6 1+3, 7 total kept, 8 to all)";
        self.cvars.get_announced(
            DAMAGE_NUMBERS,
            b"0",
            crate::cvars::CVAR_ARCHIVE,
            Some(about),
        );
    }

    /// A blow by `dealer` on `taker`, alive before it, that did `dealt` (health and armour)
    /// at `level_time`. An NPC dealer is told nothing (a server command to it goes nowhere)
    /// and nobody follows it: only the everyone line goes out for it.
    pub(super) fn tell_damage(&mut self, dealer: Party, taker: Party, dealt: i32, level_time: i32) {
        let mode = self.cvars.integer(DAMAGE_NUMBERS);
        if mode == 0 || dealer == taker || dealt <= 1 {
            return;
        }
        let Some((target_name, origin)) = self.party_shown(taker) else {
            return;
        };
        let npc_target = matches!(taker, Party::Npc(_));
        let attacker = match dealer {
            Party::Player(attacker) => attacker,
            Party::Npc(_) => {
                if let Some((name, _)) = self.party_shown(dealer)
                    && let Some(line) =
                        damage_numbers::told(mode, dealt, 0, &target_name, &name, npc_target)
                            .everyone
                {
                    self.told.push(Told::Everyone(line));
                }
                return;
            }
        };
        let Some(peer) = self.peer_mut(attacker) else {
            return;
        };
        if damage_numbers::keeps_total(mode) {
            peer.damage_total.add(dealt, level_time);
        }
        let told = damage_numbers::told(
            mode,
            dealt,
            peer.damage_total.get(),
            &target_name,
            &peer.name,
            npc_target,
        );
        for line in told.attacker {
            self.told.push(Told::One(attacker, line));
        }
        if let Some(line) = told.everyone {
            self.told.push(Told::Everyone(line));
        }
        if told.plum {
            let x = self.deaths.rng.flrand(-1.0, 1.0) * 8.0;
            let y = self.deaths.rng.flrand(-1.0, 1.0) * 8.0;
            let mut plum = EventEntity {
                event: EV_SCOREPLUM,
                parameter: 0,
                origin: [origin[0] + x, origin[1] + y, origin[2] + 16.0],
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            plum.extra[0] = (ES_OTHER_ENTITY_NUM, attacker as u32);
            plum.extra[1] = (ES_TIME, dealt as u32);
            if let Some(id) = self.pool.spawn_temporary(plum.state(), level_time, None) {
                self.pool.set_only_for(id, Some(attacker as u16));
            }
        }
        if let Some(line) = told.followers {
            for client in 0..self.players.places() {
                let follows = self.peer(client).is_some_and(|peer| {
                    peer.session.team == TEAM_SPECTATOR
                        && peer.session.spectator_state == SPECTATOR_FOLLOW
                        && peer.session.spectator_client == attacker as i32
                });
                if follows {
                    self.told.push(Told::One(client, line.clone()));
                }
            }
        }
    }
}

impl NativeGame {
    /// A party's name as the numbers show it, and where it stands (`r.currentOrigin`): an
    /// NPC goes by its `targetname`, else (none, or the empty one `npc spawn` gives) by its
    /// type.
    fn party_shown(&self, party: Party) -> Option<(Vec<u8>, [f32; 3])> {
        match party {
            Party::Player(client) => self
                .peer(client)
                .map(|peer| (peer.name.clone(), peer.state.origin())),
            Party::Npc(number) => self
                .npcs
                .roster
                .actors
                .iter()
                .find(|npc| npc.number == number)
                .map(|npc| {
                    let name = npc
                        .targetname
                        .as_ref()
                        .filter(|name| !name.is_empty())
                        .unwrap_or(&npc.npc_type);
                    (name.clone(), npc.current_origin)
                }),
        }
    }
}
