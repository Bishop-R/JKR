//! The bodies the body queue keeps once a dead player is copied into it (`CopyToBodyQue`,
//! OpenJK `codemp/game/g_client.c:1051-1180`), beside their wire states in the entity pool.
//! A body is a physics object that falls and comes to rest as an item does (`G_RunItem`
//! with no bounce, [`crate::item_physics`]), a `CONTENTS_CORPSE` box every shot meets but
//! its owner's, and takes damage until it begins to fade (`takedamage`, which `BodySink`
//! clears). Hurt past gibbing it burns away (`body_die`, `g_combat.c:700-765`):
//! `EF_DISINTEGRATION`, `origin2` where it lies, and unlinked a second later (`BodyRid`,
//! which the pool runs). `CopyToBodyQue` never sets a body's health, so a slot's next body
//! starts with whatever health the last one was left with.

use crate::entity_pool::BODIES;
use crate::item_physics::{self, ItemBounce, ItemMove};
use crate::pmove::MovementTrace;
use jkr_protocol::EntityState;

/// `CONTENTS_CORPSE`: what a body is to a trace.
pub const CONTENTS_CORPSE: u32 = 0x200;
/// A body's `clipmask`: `CONTENTS_SOLID | CONTENTS_PLAYERCLIP`.
pub const CLIP_MASK: u32 = 0x1 | 0x1_0000;
/// `GIB_HEALTH`.
const GIB_HEALTH: i32 = -40;
/// `BODY_SINK_TIME`: when `BodySink` first runs and takes `takedamage` away.
const BODY_SINK_MS: i32 = 30_000;
/// How long after `body_die` burns a body away `BodyRid` unlinks it.
const RID_MS: i32 = 1_000;
/// `EF_DISINTEGRATION`; the wire fields `s.eFlags`, `s.origin2` and `s.pos.trBase`.
const EF_DISINTEGRATION: u32 = 1 << 26;
const ES_EFLAGS: usize = 19;
const ES_ORIGIN2: [usize; 3] = [56, 60, 53];
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_GROUND: usize = 22;
const ES_POS_TYPE: usize = 23;

/// One body as the game keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corpse {
    /// `r.ownerNum`: the player it was, whose own traces (and its missiles') pass it.
    pub owner: u16,
    /// `r.mins`, `r.maxs`.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `r.currentOrigin`.
    pub current: [f32; 3],
    /// `timestamp`: when it fell.
    since: i32,
    /// A gibbed player's body, which never takes damage.
    gibbed: bool,
    /// Burnt away (`EF_DISINTEGRATION`): `BodyRid` replaced `BodySink`.
    burnt: bool,
}

/// How a body's `G_RunItem` went this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fall {
    /// At rest: only its think runs.
    Resting,
    /// It moved, and is linked again where it is (`trap->LinkEntity`).
    Moved,
    /// It came down in a no-drop volume: `G_FreeEntity` unlinks it for good.
    NoDrop,
}

/// Whether `G_RunItem` moves a body with wire state `state`: it falls, or it was left
/// with no ground (`s.groundEntityNum` none), which makes it fall.
pub fn moving(state: &EntityState) -> bool {
    state.raw_field(ES_POS_TYPE).unwrap_or(0) != item_physics::TR_STATIONARY
        || state.raw_field(ES_GROUND) == Some(u32::from(jkr_protocol::ENTITY_NUMBER_NONE))
}

/// The body queue's slots: the body lying in each, and the health each slot holds.
#[derive(Clone, Debug, Default)]
pub struct Corpses {
    slots: [Slot; BODIES],
}

#[derive(Clone, Copy, Debug, Default)]
struct Slot {
    health: i32,
    body: Option<Corpse>,
}

impl Corpses {
    /// `CopyToBodyQue` into body slot `slot`: player `owner`, dead with `health`, its box
    /// `bounds`, its body's wire state `state` (at `s.pos.trBase`) — at `level_time`.
    pub fn copied(
        &mut self,
        slot: usize,
        owner: u16,
        bounds: ([f32; 3], [f32; 3]),
        state: &EntityState,
        health: i32,
        level_time: i32,
    ) {
        let read = |index: usize| f32::from_bits(state.raw_field(index).unwrap_or(0));
        if let Some(entry) = self.slots.get_mut(slot) {
            entry.body = Some(Corpse {
                owner,
                bounds,
                current: ES_POS_BASE.map(read),
                since: level_time,
                gibbed: health <= GIB_HEALTH,
                burnt: false,
            });
        }
    }

    /// The body lying in `slot`.
    pub fn body(&self, slot: usize) -> Option<&Corpse> {
        self.slots.get(slot)?.body.as_ref()
    }

    /// Whether the body in `slot` was burnt away (`EF_DISINTEGRATION`, which only
    /// `body_die` gives a body: a disintegrated player leaves none).
    pub fn burnt(&self, slot: usize) -> bool {
        self.body(slot).is_some_and(|body| body.burnt)
    }

    /// The health `slot` holds.
    pub fn health(&self, slot: usize) -> i32 {
        self.slots.get(slot).map_or(0, |entry| entry.health)
    }

    /// `takedamage` of the body in `slot` at `level_time`: not a gibbed player's, nor once
    /// `BodySink` ran — which a body burnt away never runs.
    pub fn takes_damage(&self, slot: usize, level_time: i32) -> bool {
        self.body(slot).is_some_and(|body| {
            !body.gibbed && (body.burnt || level_time < body.since + BODY_SINK_MS)
        })
    }

    /// `G_Damage`'s end for the body in `slot`, `take` off its health (what
    /// [`crate::map_turret_world::object_take`] leaves of the blow), then `body_die` at or
    /// below nothing: lifted to `GIB_HEALTH + 1` from below it and, the first time, burnt
    /// away where it lies — `EF_DISINTEGRATION` and `origin2` written to `state`. Returns
    /// `BodyRid`'s time when it was burnt now.
    pub fn hurt(
        &mut self,
        slot: usize,
        take: i32,
        state: &mut EntityState,
        level_time: i32,
    ) -> Option<i32> {
        let entry = self.slots.get_mut(slot)?;
        let body = entry.body.as_mut()?;
        entry.health -= take;
        if entry.health > 0 {
            return None;
        }
        entry.health = entry.health.max(-999);
        let gibbed = entry.health < GIB_HEALTH + 1;
        if gibbed {
            entry.health = GIB_HEALTH + 1;
        }
        let flags = state.raw_field(ES_EFLAGS).unwrap_or(0);
        if !gibbed || flags & EF_DISINTEGRATION != 0 {
            return None;
        }
        state.set_raw_field(ES_EFLAGS, flags | EF_DISINTEGRATION);
        for (index, value) in ES_ORIGIN2.into_iter().zip(body.current) {
            state.set_raw_field(index, value.to_bits());
        }
        body.burnt = true;
        Some(level_time + RID_MS)
    }

    /// `G_RunItem` for the body in `slot`, its wire state `state`, at `level_time` after
    /// `previous_time`: swept by `sweep` (its box from where it is to where its trajectory
    /// has it, its owner passed, [`CLIP_MASK`]) and, striking something, brought to rest on
    /// a floor or dropped from a wall; `contents` gives the point contents where it struck.
    pub fn fall(
        &mut self,
        slot: usize,
        state: &mut EntityState,
        level_time: i32,
        previous_time: i32,
        sweep: impl FnOnce([f32; 3], [f32; 3]) -> MovementTrace,
        contents: impl FnOnce([f32; 3]) -> u32,
    ) -> Fall {
        let Some(body) = self
            .slots
            .get_mut(slot)
            .and_then(|entry| entry.body.as_mut())
        else {
            return Fall::Resting;
        };
        let ItemMove::Moved(trace) =
            item_physics::fall(state, &mut body.current, level_time, sweep)
        else {
            return Fall::Resting;
        };
        match item_physics::bounce(
            state,
            &mut body.current,
            &trace,
            level_time,
            previous_time,
            contents,
        ) {
            ItemBounce::NoDrop => Fall::NoDrop,
            _ => Fall::Moved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ES_POS_TIME: usize = 0;

    fn body_state(origin: [f32; 3]) -> EntityState {
        let mut state = EntityState::zero(40, &jkr_protocol::LEGACY_ENTITY_FIELDS);
        for (index, value) in ES_POS_BASE.into_iter().zip(origin) {
            state.set_raw_field(index, value.to_bits());
        }
        state
    }

    fn copied(health: i32) -> (Corpses, EntityState) {
        let state = body_state([10.0, 20.0, -16.0]);
        let mut corpses = Corpses::default();
        let bounds = ([-15.0, -15.0, -24.0], [15.0, 15.0, -8.0]);
        corpses.copied(3, 1, bounds, &state, health, 5_000);
        (corpses, state)
    }

    #[test]
    fn a_body_burns_away_once_hurt_past_gibbing_and_its_slot_keeps_the_health() {
        let (mut corpses, mut state) = copied(-10);
        assert!(corpses.takes_damage(3, 34_999));
        assert!(!corpses.takes_damage(3, 35_000));
        // A fresh slot holds nothing: 39 points leave it at -39, short of gibbing.
        assert_eq!(corpses.hurt(3, 39, &mut state, 6_000), None);
        assert_eq!(corpses.health(3), -39);
        assert_eq!(corpses.hurt(3, 1, &mut state, 6_050), Some(7_050));
        assert_eq!(corpses.health(3), -39);
        assert_ne!(state.raw_field(ES_EFLAGS).unwrap() & EF_DISINTEGRATION, 0);
        assert_eq!(state.raw_field(ES_ORIGIN2[2]), Some((-16.0f32).to_bits()));
        // Burnt, it still takes damage (no `BodySink` any more), but burns only once.
        assert!(corpses.takes_damage(3, 40_000));
        assert_eq!(corpses.hurt(3, 500, &mut state, 6_100), None);
        // The slot's next body starts at -39: one point burns it.
        let (fresh, _) = copied(-5);
        corpses.slots[3].body = fresh.slots[3].body;
        let mut next = body_state([0.0; 3]);
        assert_eq!(corpses.hurt(3, 1, &mut next, 9_000), Some(10_000));
    }

    #[test]
    fn a_gibbed_players_body_takes_no_damage_and_a_falling_one_lands() {
        let (corpses, _) = copied(-40);
        assert!(!corpses.takes_damage(3, 5_000));
        let (mut corpses, mut state) = copied(-10);
        state.set_raw_field(ES_POS_TYPE, 6);
        state.set_raw_field(ES_POS_TIME, 5_000);
        state.set_raw_field(ES_GROUND, u32::from(jkr_protocol::ENTITY_NUMBER_NONE));
        assert!(moving(&state));
        let floor = |from: [f32; 3], to: [f32; 3]| MovementTrace {
            fraction: 0.5,
            end_position: [from[0], from[1], (from[2] + to[2]) * 0.5],
            plane_normal: [0.0, 0.0, 1.0],
            entity_number: crate::pmove::ENTITY_NUMBER_WORLD,
            ..MovementTrace::miss(to)
        };
        assert_eq!(
            corpses.fall(3, &mut state, 5_050, 5_000, floor, |_| 0),
            Fall::Moved
        );
        // At rest a unit above where it struck, and resting from then on.
        assert_eq!(state.raw_field(ES_POS_TYPE), Some(0));
        assert!(!moving(&state));
        assert_eq!(
            corpses.fall(
                3,
                &mut state,
                5_100,
                5_050,
                |_, to| MovementTrace::miss(to),
                |_| 0
            ),
            Fall::Resting
        );
    }
}
