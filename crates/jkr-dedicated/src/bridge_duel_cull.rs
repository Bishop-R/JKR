//! TaystJK's duel isolation (`sv_snapShotDuelCull`, on by default there; its
//! `server/duel_cull.cpp`). While two players duel, every other player and NPC, and
//! everything theirs, is cut off from the pair: never sent in a duellist's snapshot
//! (`SV_AddEntitiesVisibleFromPoint`, `sv_snapshot.cpp:483`) and never met by a
//! duellist's traces (`SV_ClipMoveToEntities`, `sv_world.cpp:614`). Everyone else still
//! sees the duellists, but passes through them and what is theirs, and is sent them as
//! not solid so that its prediction does too (`sv_snapshot.cpp:733-735`).
//!
//! Who an entity belongs to is `flatten`'s answer: a missile its owner, a saber or shield
//! hit the one it names, a player's own event that player. TaystJK spares JA+ and jaPRO
//! plugin clients the unsolid copies and lets jaPRO's "duel see others" opt out; this
//! server announces itself as the base game, where TaystJK does neither.

use super::*;

/// `ET_PLAYER`, `ET_MISSILE`, `ET_MOVER`, `ET_NPC`, `ET_EVENTS` (`bg_public.h`).
const ET_PLAYER: u32 = 1;
const ET_MISSILE: u32 = 3;
const ET_MOVER: u32 = 6;
const ET_NPC: u32 = 13;
const ET_EVENTS: u32 = 18;
/// The events `flatten` follows to their owner (`entity_event_t`).
const EV_SABER_HIT: u32 = 30;
const EV_SABER_BLOCK: u32 = 31;
const EV_PLAYER_TELEPORT_IN: u32 = 64;
const EV_PLAYER_TELEPORT_OUT: u32 = 65;
const EV_GENERAL_SOUND: u32 = 76;
const EV_SHIELD_HIT: u32 = 110;
/// `EF_PLAYER_EVENT`: a player's own event, sent to all but that player.
const EF_PLAYER_EVENT: u32 = 1 << 5;
/// The wire fields `eType`, `eFlags`, `clientNum`, `otherEntityNum2`, `otherEntityNum`.
const ES_TYPE: usize = 8;
const ES_EFLAGS: usize = 19;
const ES_CLIENT_NUM: usize = 32;
const ES_OTHER_ENTITY_NUM2: usize = 39;
const ES_OTHER_ENTITY_NUM: usize = 59;
/// `MAX_GENTITIES`: the legacy entity numbers an owner can be found by.
const ENTITY_NUMBERS: usize = 1_024;

/// What `DuelCull` answers for an entity seen or met by another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Isolation {
    /// 0: as if nobody duelled.
    Shared,
    /// 1: not sent, not met.
    Cut,
    /// 2: sent, but not solid to it and not met by its traces.
    Passed,
}

/// Who an entity belongs to (`flatten`, then `isActor`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Party {
    /// Not a player's or an NPC's: the world, a mover, an item, an event of its own.
    #[default]
    Nobody,
    /// A player, by wire client number.
    Player(u16),
    /// An NPC.
    Npc,
}

/// One frame's duels and who each entity belongs to, worked out only while the option is
/// on and somebody duels.
#[derive(Debug, Default)]
pub(super) struct DuelCull {
    /// The frame and pool generation it was worked out for.
    key: Option<(i32, u64, i32)>,
    /// Somebody duels and the option is on: nothing is cut off otherwise.
    active: bool,
    /// Each wire client's opponent while it duels.
    opponents: Vec<Option<u16>>,
    /// Who each legacy entity number belongs to.
    parties: Vec<Party>,
}

impl DuelCull {
    /// Who entity `number` belongs to.
    pub(super) fn party(&self, number: u16) -> Party {
        self.parties
            .get(usize::from(number))
            .copied()
            .unwrap_or_default()
    }

    /// `isDueling`: the opponent of the player a party is, while it duels.
    fn opponent(&self, party: Party) -> Option<u16> {
        match party {
            Party::Player(number) => self.opponents.get(usize::from(number)).copied().flatten(),
            Party::Nobody | Party::Npc => None,
        }
    }

    /// `DuelCull(ent, touch)`: what `ent`'s party sees of, and meets in, `touch`'s.
    pub(super) fn between(&self, ent: Party, touch: Party) -> Isolation {
        if !self.active || ent == Party::Nobody || touch == Party::Nobody {
            return Isolation::Shared;
        }
        match (self.opponent(ent), self.opponent(touch)) {
            (None, None) => Isolation::Shared,
            (None, Some(_)) => Isolation::Passed,
            (Some(opponent), Some(_)) if ent == touch || touch == Party::Player(opponent) => {
                Isolation::Shared
            }
            _ => Isolation::Cut,
        }
    }

    /// Who a pool entity belongs to, as `flatten` reads its state: a missile its owner's,
    /// a saber or shield hit the one it names, a player's teleport that player's, a
    /// player's own event (`EF_PLAYER_EVENT`, sent to all but `not_for`) that player's.
    pub(super) fn party_of(&self, number: u16, state: &EntityState, not_for: Option<u16>) -> Party {
        if !self.active {
            return Party::Nobody;
        }
        let field = |index| state.raw_field(index).unwrap_or(0);
        let kind = field(ES_TYPE);
        let named = |index| (field(index) & 0x3ff) as u16;
        let owner = match kind {
            ET_MOVER => return Party::Nobody,
            ET_MISSILE | ET_PLAYER | ET_NPC => number,
            _ if kind > ET_EVENTS => match kind - ET_EVENTS {
                EV_GENERAL_SOUND => return Party::Nobody,
                EV_SABER_HIT | EV_SABER_BLOCK => {
                    let second = named(ES_OTHER_ENTITY_NUM2);
                    if second == jkr_protocol::ENTITY_NUMBER_NONE {
                        named(ES_OTHER_ENTITY_NUM)
                    } else {
                        second
                    }
                }
                EV_SHIELD_HIT => named(ES_OTHER_ENTITY_NUM),
                _ if field(ES_EFLAGS) & EF_PLAYER_EVENT != 0 => match not_for {
                    Some(player) => player,
                    None => return Party::Nobody,
                },
                EV_PLAYER_TELEPORT_IN | EV_PLAYER_TELEPORT_OUT => named(ES_CLIENT_NUM),
                _ => return Party::Nobody,
            },
            _ => return Party::Nobody,
        };
        self.party(owner)
    }
}

impl NativeGame {
    /// The frame's duels and owners ([`DuelCull`]), worked out again once the frame, the
    /// pool or the option has moved on.
    pub(super) fn duel_cull(&self) -> std::cell::Ref<'_, DuelCull> {
        let option = self.cvars.integer(b"sv_snapShotDuelCull");
        let key = (self.last_frame_time, self.pool.generation(), option);
        if self.duel_cull.borrow().key != Some(key) {
            let mut cull = self.duel_cull.borrow_mut();
            cull.key = Some(key);
            self.work_out_duel_cull(&mut cull, option != 0);
        }
        self.duel_cull.borrow()
    }

    fn work_out_duel_cull(&self, cull: &mut DuelCull, enabled: bool) {
        cull.active = false;
        cull.opponents.clear();
        cull.parties.clear();
        let Some(world) = self.server.world(self.world) else {
            return;
        };
        // Who plays (an `ET_PLAYER` entity), and whom each duels.
        for (number, handle) in self.players.holders().enumerate() {
            let peer = handle
                .and_then(|handle| world.entity(handle))
                .filter(|peer| peer.begun);
            cull.parties
                .push(peer.map_or(Party::Nobody, |_| Party::Player(number as u16)));
            cull.opponents.push(peer.and_then(|peer| {
                (peer.state.duel_in_progress()).then(|| peer.state.duel_index())
            }));
        }
        if !enabled || cull.opponents.iter().all(Option::is_none) {
            return;
        }
        cull.active = true;
        cull.parties.resize(ENTITY_NUMBERS.max(cull.parties.len()), Party::Nobody);
        for npc in &self.npcs.roster.actors {
            if let Some(party) = cull.parties.get_mut(usize::from(npc.number)) {
                *party = Party::Npc;
            }
        }
        // `flatten` follows only what is `ET_MISSILE` now: a stuck trip mine or det pack
        // (`ET_GENERAL`) is nobody's.
        let owned = self
            .missiles
            .iter()
            .chain(self.dead_sabers_waiting.iter())
            .map(|(id, missile)| (*id, missile.owner))
            .chain(
                self.charges
                    .iter()
                    .map(|(id, charge)| (*id, charge.missile.owner)),
            )
            .filter(|(id, _)| {
                self.pool
                    .state(*id)
                    .and_then(|state| state.raw_field(ES_TYPE))
                    == Some(ET_MISSILE)
            })
            .map(|(id, owner)| (id.legacy_number(), owner));
        for (number, owner) in owned {
            let party = cull.party(owner);
            if let Some(slot) = cull.parties.get_mut(usize::from(number)) {
                *slot = party;
            }
        }
    }

    /// The obstacles a tracer of party `pass` does not meet, taken out of the gathered
    /// ones: what is cut off from it or passed by it.
    pub(super) fn isolate_obstacles(&mut self, pass: Party) {
        if pass == Party::Nobody || !self.duel_cull().active {
            return;
        }
        let cull = self.duel_cull.borrow();
        self.obstacles.retain(|obstacle| {
            cull.between(pass, cull.party(obstacle.entity)) == Isolation::Shared
        });
    }

    /// [`Self::isolate_obstacles`] for the traces of entity `tracer` (a client, or the
    /// NPC whose number it is).
    pub(super) fn isolate_tracer_obstacles(&mut self, tracer: usize) {
        if let Ok(number) = u16::try_from(tracer) {
            let pass = self.duel_cull().party(number);
            self.isolate_obstacles(pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Identity, NativeGame};
    use super::*;
    use jkr_network::LocalSnapshotBuffer;
    use jkr_protocol::{PlayerState, Snapshot, UserCommand};

    /// `ps.duelIndex`, `ps.duelInProgress`; the wire `solid`.
    const PS_DUEL_INDEX: usize = 44;
    const PS_DUEL_IN_PROGRESS: usize = 119;
    const ES_SOLID: usize = 26;

    /// Three players in the game, all solid, 0 and 1 duelling, 2 looking on.
    fn duel() -> NativeGame {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        game.register_taystjk_server();
        for client in 0..3 {
            let userinfo = format!(
                "\\name\\p{client}\\rate\\25000\\snaps\\40\\model\\kyle/default\\forcepowers\\7-1-032330000000001333\\color1\\4\\color2\\4\\handicap\\100\\cg_predictItems\\1\\saber1\\single_1\\saber2\\none\\char_color_red\\255\\char_color_green\\255\\char_color_blue\\255\\ip\\127.0.0.{}:1",
                client + 1
            );
            game.client_connect(client, userinfo.as_bytes()).unwrap();
            game.enter_world(client, &UserCommand::default(), 100);
            let peer = game.peer_mut(client).unwrap();
            peer.session.team = 0;
            peer.entity.state_mut().set_raw_field(ES_SOLID, 0x0040_2010);
        }
        for (client, opponent) in [(0, 1), (1, 0)] {
            let state = &mut game.peer_mut(client).unwrap().state;
            state.set_raw_field(PS_DUEL_IN_PROGRESS, 1);
            state.set_raw_field(PS_DUEL_INDEX, opponent);
        }
        game
    }

    /// The players client `client`'s snapshot lists, each with its `solid`.
    fn seen(game: &NativeGame, client: usize) -> Vec<(u16, u32)> {
        let mut snapshot = Snapshot {
            message_sequence: 0,
            reliable_acknowledge: 0,
            server_commands: Vec::new(),
            server_time: 0,
            delta_from: None,
            flags: 0,
            area_mask: Vec::new(),
            player: PlayerState::zero(),
            vehicle_player: None,
            entities: Vec::new(),
            consumed_bits: 0,
        };
        LocalSnapshotBuffer::new().capture(game, client, &mut snapshot);
        snapshot
            .entities
            .iter()
            .filter(|entity| entity.number() < 3)
            .map(|entity| (entity.number(), entity.raw_field(ES_SOLID).unwrap_or(0)))
            .collect()
    }

    /// The players client `client`'s traces meet.
    fn met(game: &mut NativeGame, client: usize) -> Vec<u16> {
        game.gather_obstacles(client);
        let mut met: Vec<u16> = (game.obstacles.iter())
            .map(|obstacle| obstacle.entity)
            .filter(|&entity| entity < 3)
            .collect();
        met.sort_unstable();
        met
    }

    /// TaystJK's default: a duellist sees and meets only its opponent; a bystander still
    /// sees both, unsolid, and passes through them. Off, everything is as without duels.
    #[test]
    fn duellists_are_cut_off_in_snapshots_and_traces() {
        let mut game = duel();
        let solid = 0x0040_2010;
        assert_eq!(seen(&game, 0), [(1, solid)]);
        assert_eq!(seen(&game, 1), [(0, solid)]);
        assert_eq!(seen(&game, 2), [(0, 0), (1, 0)]);
        assert_eq!(met(&mut game, 0), [1]);
        assert_eq!(met(&mut game, 2), Vec::<u16>::new());
        game.cvars.set(b"sv_snapShotDuelCull", b"0");
        assert_eq!(seen(&game, 0), [(1, solid), (2, solid)]);
        assert_eq!(seen(&game, 2), [(0, solid), (1, solid)]);
        assert_eq!(met(&mut game, 0), [1, 2]);
        assert_eq!(met(&mut game, 2), [0, 1]);
    }

    fn cull(opponents: &[Option<u16>]) -> DuelCull {
        let mut cull = DuelCull {
            active: true,
            opponents: opponents.to_vec(),
            ..DuelCull::default()
        };
        cull.parties.resize(ENTITY_NUMBERS, Party::Nobody);
        for number in 0..opponents.len() {
            cull.parties[number] = Party::Player(number as u16);
        }
        cull
    }

    #[test]
    fn duellists_and_bystanders_as_taystjk_isolates_them() {
        // 0 and 1 duel; 2 does not; an NPC and the world.
        let cull = cull(&[Some(1), Some(0), None]);
        let (a, b, c) = (Party::Player(0), Party::Player(1), Party::Player(2));
        let rows = [
            (a, b, Isolation::Shared),
            (b, a, Isolation::Shared),
            (a, a, Isolation::Shared),
            (a, c, Isolation::Cut),
            (a, Party::Npc, Isolation::Cut),
            (c, a, Isolation::Passed),
            (Party::Npc, a, Isolation::Passed),
            (c, Party::Npc, Isolation::Shared),
            (a, Party::Nobody, Isolation::Shared),
            (Party::Nobody, a, Isolation::Shared),
        ];
        for (ent, touch, expected) in rows {
            assert_eq!(cull.between(ent, touch), expected, "{ent:?} -> {touch:?}");
        }
    }

    #[test]
    fn a_second_duel_is_cut_off_from_the_first() {
        let cull = cull(&[Some(1), Some(0), Some(3), Some(2)]);
        assert_eq!(
            cull.between(Party::Player(0), Party::Player(2)),
            Isolation::Cut
        );
        assert_eq!(
            cull.between(Party::Player(3), Party::Player(2)),
            Isolation::Shared
        );
    }

    #[test]
    fn nothing_is_cut_off_while_the_option_is_off_or_nobody_duels() {
        let mut off = cull(&[Some(1), Some(0), None]);
        off.active = false;
        assert_eq!(
            off.between(Party::Player(0), Party::Player(2)),
            Isolation::Shared
        );
    }

    #[test]
    fn events_belong_to_whom_they_name() {
        let mut cull = cull(&[Some(1), Some(0), None]);
        // Missile 100 is player 2's.
        cull.parties[100] = Party::Player(2);
        let event = |kind: u32, fields: &[(usize, u32)]| {
            let mut state = EntityState::zero(200, &jkr_protocol::LEGACY_ENTITY_FIELDS);
            state.set_raw_field(ES_TYPE, kind);
            for &(index, value) in fields {
                state.set_raw_field(index, value);
            }
            state
        };
        let none = u32::from(jkr_protocol::ENTITY_NUMBER_NONE);
        let rows = [
            (event(ET_MISSILE, &[]), 100, None, Party::Player(2)),
            (
                event(
                    ET_EVENTS + EV_SABER_HIT,
                    &[(ES_OTHER_ENTITY_NUM, 1), (ES_OTHER_ENTITY_NUM2, none)],
                ),
                200,
                None,
                Party::Player(1),
            ),
            (
                event(
                    ET_EVENTS + EV_SABER_BLOCK,
                    &[(ES_OTHER_ENTITY_NUM, 1), (ES_OTHER_ENTITY_NUM2, 2)],
                ),
                200,
                None,
                Party::Player(2),
            ),
            (
                event(ET_EVENTS + EV_SHIELD_HIT, &[(ES_OTHER_ENTITY_NUM, 0)]),
                200,
                None,
                Party::Player(0),
            ),
            (
                event(ET_EVENTS + 2, &[(ES_EFLAGS, EF_PLAYER_EVENT)]),
                200,
                Some(1),
                Party::Player(1),
            ),
            (
                event(ET_EVENTS + EV_PLAYER_TELEPORT_IN, &[(ES_CLIENT_NUM, 2)]),
                200,
                None,
                Party::Player(2),
            ),
            (
                event(
                    ET_EVENTS + EV_GENERAL_SOUND,
                    &[(ES_EFLAGS, EF_PLAYER_EVENT)],
                ),
                200,
                Some(1),
                Party::Nobody,
            ),
            (event(ET_MOVER, &[]), 1, None, Party::Nobody),
        ];
        for (state, number, not_for, expected) in rows {
            assert_eq!(cull.party_of(number, &state, not_for), expected, "{number}");
        }
    }
}
