//! Stock's client-side saber compensation (`CG_SaberCompWork`, OpenJK
//! `codemp/cgame/cg_players.c:5838-6010`, run for every drawn blade by `CG_AddSaberBlade`,
//! `6168-6191`): between the server's saber hits the client keeps showing the contact.
//! For 400 ms after the server reports a hit by a blade's owner (`EV_SABER_HIT`,
//! `cg_event.c:2264-2283`), each of the owner's drawn blades — cut short where it meets
//! the world — is traced from hilt to tip every 5 ms at most (stock switched the trace
//! back from the tip off); where the first thing it strikes is the body the server said
//! it struck and it passes into that model, sparks fly (blood sparks on flesh, the cut
//! effect on anything else) and, with `cg_ghoul2Marks`, the blade leaves a glowing cut
//! ([`crate::ghoul2_marks::saber_mark`]).
//!
//! Not here: a custom saber's own hit effects and mark shaders (`hitPersonEffect`,
//! `g2MarksShader`, `g2WeaponMarkShader` in its `.sab` file); no stock saber sets them.

use jkr_protocol::EntityState;

/// `CG_MAX_SABER_COMP_TIME`.
pub const WINDOW_MS: i64 = 400;
/// The blade's `storageTime` debounce.
pub const DEBOUNCE_MS: i64 = 5;
/// `cgs.effects.mSaberBloodSparks`, `mSaberCut` (`cg_main.c:1178-1180`).
pub const FLESH_EFFECT: &str = "saber/blood_sparks_mp";
pub const CUT_EFFECT: &str = "saber/saber_cut";
/// `EV_SABER_HIT`, `ENTITYNUM_NONE`.
const EV_SABER_HIT: u16 = 30;
const ENTITY_NONE: u16 = 1_023;

/// The last saber hit the server reported for a blade's owner: `serverSaberHitIndex`,
/// `serverSaberHitTime` and `serverSaberFleshImpact`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ServerHit {
    pub victim: u16,
    pub time: i64,
    pub flesh: bool,
}

impl ServerHit {
    /// Whether the owner's blades compensate at presentation time `now`: up to 400 ms
    /// after the hit, but not in the very frame it came. Stock stamps the hit with
    /// `cg.time` as the view reaches the hit's snapshot; a hit seen on arrival, before
    /// the view gets there, waits for it.
    pub fn compensating(&self, now: i64) -> bool {
        (1..=WINDOW_MS).contains(&(now - self.time))
    }

    /// The effect a compensated contact plays.
    pub fn effect(&self) -> &'static str {
        if self.flesh { FLESH_EFFECT } else { CUT_EFFECT }
    }
}

/// A blade owner's server hits as the view reaches them: the latest seen and the one
/// before it, which stands while the view has not reached the latest yet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OwnerHits {
    latest: Option<ServerHit>,
    previous: Option<ServerHit>,
}

impl OwnerHits {
    /// Another hit by the owner; one in the same snapshot as the latest replaces it, as
    /// stock's last event in a snapshot wins.
    pub fn record(&mut self, hit: ServerHit) {
        if self.latest.is_some_and(|latest| latest.time < hit.time) {
            self.previous = self.latest;
        }
        self.latest = Some(hit);
    }

    /// `serverSaberHitIndex`/`Time`/`FleshImpact` as they stand at presentation time `now`.
    pub fn at(&self, now: i64) -> Option<ServerHit> {
        [self.latest, self.previous]
            .into_iter()
            .flatten()
            .find(|hit| hit.time <= now)
    }
}

/// A saber hit event's owner and its hit at `time` (its snapshot's server time), when the
/// event names both. `event` is the event as decoded, sequence bits off: a temporary event
/// entity carries it in its type (`ET_EVENTS + event`), not in `event`.
pub fn server_hit(state: &EntityState, event: u16, time: i64) -> Option<(u16, ServerHit)> {
    if event != EV_SABER_HIT {
        return None;
    }
    let (victim, owner) = (state.other_entity_num(), state.other_entity_num2());
    (victim != ENTITY_NONE && owner != ENTITY_NONE).then_some((
        owner,
        ServerHit {
            victim,
            time,
            flesh: state.event_parameter() != 0,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_protocol::LEGACY_ENTITY_FIELDS;

    #[test]
    fn a_hit_names_its_owner_and_compensates_for_four_hundred_milliseconds() {
        let mut state = EntityState::zero(400, &LEGACY_ENTITY_FIELDS);
        // `eventParm`, `otherEntityNum`, `otherEntityNum2`.
        state.set_raw_field(42, 2);
        state.set_raw_field(59, 4);
        state.set_raw_field(39, 7);
        let (owner, hit) = server_hit(&state, 30, 1_000).unwrap();
        assert_eq!((owner, hit.victim, hit.flesh), (7, 4, true));
        assert_eq!(hit.effect(), FLESH_EFFECT);
        // Not before the view reaches the hit, nor in its frame; for 400 ms after.
        assert!(!hit.compensating(950) && !hit.compensating(1_000));
        assert!(hit.compensating(1_016) && hit.compensating(1_400));
        assert!(!hit.compensating(1_401));
        // A hit that names nobody is not kept.
        state.set_raw_field(39, 1_023);
        assert_eq!(server_hit(&state, 30, 1_000), None);
        assert_eq!(server_hit(&state, 31, 1_000), None);
    }

    #[test]
    fn a_hit_seen_early_waits_while_the_one_before_it_stands() {
        let hit = |victim, time| ServerHit {
            victim,
            time,
            flesh: true,
        };
        let mut hits = OwnerHits::default();
        assert_eq!(hits.at(1_000), None);
        hits.record(hit(4, 1_000));
        hits.record(hit(5, 1_050));
        assert_eq!(hits.at(990), None);
        assert_eq!(hits.at(1_030).map(|hit| hit.victim), Some(4));
        assert_eq!(hits.at(1_050).map(|hit| hit.victim), Some(5));
        // A second hit in the same snapshot replaces the latest only.
        hits.record(hit(6, 1_050));
        assert_eq!(hits.at(1_030).map(|hit| hit.victim), Some(4));
        assert_eq!(hits.at(1_060).map(|hit| hit.victim), Some(6));
    }
}
