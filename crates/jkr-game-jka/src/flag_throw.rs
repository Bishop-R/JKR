//! jaPRO's flag throwing (`g_allowFlagThrow`: `Cmd_Throwflag_f`, `Drop_Flag`, and its
//! lines in `LaunchItem`, `Drop_Item`, `G_RunItem`, `G_BounceItem`, `Touch_Item`) and its
//! suicide fix (`g_fixFlagSuicide`). With throwing on, a carrier throws its flag along
//! its view; flags bounce and stand upright; a dead carrier's flag keeps its momentum;
//! nobody may touch a flag for a second after throwing one; taking the enemy flag scores
//! nothing ([`crate::ctf`]). Both off is stock.

use crate::dropped_items::{ES_ANGLES, ES_APOS_BASE, ES_POS_BASE, item_for_powerup, launch};
use crate::items::Pickup;
use crate::pmove::flight::flight_axes;
use jkr_protocol::{EntityState, PlayerState};

/// `PW_REDFLAG`, `PW_BLUEFLAG`, `PW_NEUTRALFLAG`: the flags, in the order
/// `Cmd_Throwflag_f` looks for one.
const FLAGS: [usize; 3] = [4, 5, 6];
/// `CONTENTS_TRIGGER|CONTENTS_CORPSE`: a thrown flag is touched like any item, and shots
/// and blasts meet it.
pub const THROWN_CONTENTS: u32 = 0x400 | crate::corpses::CONTENTS_CORPSE;
/// `physicsBounce` of a flag under throwing (0.5 with `g_tweakWeapons`' Tribes bit).
const FLAG_BOUNCE: f32 = 0.6;
/// `Drop_Flag`'s throw speed (700 with the Tribes bit).
const THROW_SPEED: f32 = 625.0;
/// Above this speed across, a flag in flight is slowed (`G_RunItem`).
const DRAG_SPEED: f32 = 800.0;

/// jaPRO's flag options; both off is stock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlagRules {
    /// `g_allowFlagThrow`.
    pub throw: bool,
    /// `g_fixFlagSuicide`: a suicide drops the flag (`TossClientItems`) instead of
    /// sending it home; with throwing on, such a flag is in the way of shots as a thrown
    /// one is.
    pub suicide_drops: bool,
}

/// `LaunchItem` for a flag under throwing: it keeps some of its speed at each bounce,
/// and stands upright (no roll).
fn launch_flag(item: usize, origin: [f32; 3], velocity: [f32; 3], level_time: i32) -> Pickup {
    let mut pickup = launch(item, origin, velocity, level_time);
    pickup.state.set_raw_field(ES_ANGLES[2], 0f32.to_bits());
    if let Some(dropped) = pickup.dropped.as_mut() {
        dropped.bounce = FLAG_BOUNCE;
    }
    pickup
}

fn read(entity: &EntityState, fields: [usize; 3]) -> [f32; 3] {
    fields.map(|index| f32::from_bits(entity.raw_field(index).unwrap_or(0)))
}

/// `Drop_Flag(ent, item, qfalse)`: thrown from where the player's entity stands along its
/// view, pitch and all, at 625, with the player's own `velocity` and 50 more upward.
pub fn drop_flag(item: usize, player: &EntityState, velocity: [f32; 3], level_time: i32) -> Pickup {
    let forward = flight_axes(read(player, ES_APOS_BASE)).0.to_array();
    let mut thrown = forward.map(|axis| axis * THROW_SPEED);
    thrown[0] += velocity[0];
    thrown[1] += velocity[1];
    thrown[2] += 50.0 + velocity[2];
    launch_flag(item, read(player, ES_POS_BASE), thrown, level_time)
}

/// `Drop_Item` for a flag under throwing (jaPRO's "flag inheritance"): along the dead
/// carrier's view, its pitch kept and its yaw turned by `angle`, at one unit a second
/// plus a quarter of the carrier's `velocity` across and half of it up or down.
pub fn drop_carried(
    item: usize,
    player: &EntityState,
    velocity: [f32; 3],
    angle: f32,
    level_time: i32,
) -> Pickup {
    let mut angles = read(player, ES_APOS_BASE);
    angles[1] += angle;
    let forward = flight_axes(angles).0.to_array();
    // `velocity[0] += 0.25 * ps.velocity[0]`: a double sum, rounded back to a float.
    let add =
        |base: f32, share: f64, speed: f32| (f64::from(base) + share * f64::from(speed)) as f32;
    let thrown = [
        add(forward[0], 0.25, velocity[0]),
        add(forward[1], 0.25, velocity[1]),
        add(forward[2], 0.5, velocity[2]),
    ];
    launch_flag(item, read(player, ES_POS_BASE), thrown, level_time)
}

/// `Cmd_Throwflag_f` for a player allowed to throw: the first flag it carries thrown
/// ([`drop_flag`]) as a trigger shots meet, counting its seconds left, and the flag
/// gone from the player. `None` when it carries none.
pub fn throw_flag(
    state: &mut PlayerState,
    entity: &EntityState,
    level_time: i32,
) -> Option<Pickup> {
    let flag = FLAGS.into_iter().find(|flag| state.powerups[*flag] != 0)?;
    let item = item_for_powerup(flag)?;
    let mut thrown = drop_flag(item, entity, state.velocity(), level_time);
    thrown.contents = THROWN_CONTENTS;
    if let Some(dropped) = thrown.dropped.as_mut() {
        dropped.count = ((state.powerups[flag] as i32).wrapping_sub(level_time) / 1_000).max(1);
    }
    state.powerups[flag] = 0;
    Some(thrown)
}

/// `Touch_Item` under throwing: a flag (red, blue or neutral) is not taken by whoever
/// threw one less than a second ago.
pub fn locked_out(rules: FlagRules, tag: i32, last_throw_time: i32, level_time: i32) -> bool {
    rules.throw
        && FLAGS.contains(&(tag as usize))
        && last_throw_time.saturating_add(1_000) > level_time
}

/// `G_BounceItem`'s stop for a flag under throwing: whatever it struck, it comes to rest
/// once the bounce leaves it barely moving up or down.
pub fn stops(bounced_vertical: f32) -> bool {
    bounced_vertical > -5.0 && bounced_vertical < 25.0
}

/// `G_RunItem`'s drag on a flag in flight, each frame: above 800 a second across, it
/// loses `0.75 * (speed / 800)²` of that speed (a thrown flag keeps its punch, a much
/// faster one is reined in).
pub fn drag(delta: &mut [f32; 3]) {
    let speed = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
    if speed > DRAG_SPEED {
        let cut = (speed * speed / (DRAG_SPEED * DRAG_SPEED)) * 0.75;
        let keep = 1.0 - cut / speed;
        delta[0] *= keep;
        delta[1] *= keep;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ITEMS;
    use jkr_protocol::LEGACY_ENTITY_FIELDS;

    fn carrier(pitch: f32, yaw: f32) -> EntityState {
        let mut entity = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
        for (index, value) in ES_POS_BASE.into_iter().zip([100.0f32, 200.0, 24.0]) {
            entity.set_raw_field(index, value.to_bits());
        }
        for (index, value) in ES_APOS_BASE.into_iter().zip([pitch, yaw, 0.0]) {
            entity.set_raw_field(index, value.to_bits());
        }
        entity
    }

    fn delta(pickup: &Pickup) -> [f32; 3] {
        [6, 7, 10].map(|index| f32::from_bits(pickup.state.raw_field(index).unwrap_or(0)))
    }

    #[test]
    fn a_thrown_flag_flies_along_the_view_with_the_throwers_speed_and_stands_upright() {
        let red = item_for_powerup(4).expect("the red flag");
        let mut state = PlayerState::default();
        state.powerups[4] = i32::MAX as u32;
        state.set_raw_field(6, 300f32.to_bits());
        let thrown = throw_flag(&mut state, &carrier(0.0, 90.0), 1_000).expect("thrown");
        assert_eq!(state.powerups[4], 0);
        assert_eq!(thrown.item, red);
        assert_eq!(thrown.contents, THROWN_CONTENTS);
        let velocity = delta(&thrown);
        assert!((velocity[0] - 300.0).abs() < 0.01, "{velocity:?}");
        assert!((velocity[1] - 625.0).abs() < 0.01, "{velocity:?}");
        assert_eq!(velocity[2], 50.0);
        assert_eq!(thrown.state.raw_field(ES_ANGLES[2]), Some(0));
        let dropped = thrown.dropped.expect("dropped");
        assert_eq!(
            (dropped.bounce, dropped.count),
            (0.6, (i32::MAX - 1_000) / 1_000)
        );
        assert!(ITEMS[thrown.item].kind == crate::items::Kind::Team);
    }

    #[test]
    fn a_dead_carriers_flag_keeps_part_of_its_momentum() {
        let blue = item_for_powerup(5).expect("the blue flag");
        let dropped = drop_carried(blue, &carrier(-90.0, 0.0), [400.0, 0.0, 200.0], 45.0, 0);
        let velocity = delta(&dropped);
        assert!((velocity[0] - 100.0).abs() < 0.01, "{velocity:?}");
        assert!((velocity[2] - 101.0).abs() < 0.01, "{velocity:?}");
    }

    #[test]
    fn the_thrower_waits_a_second_and_fast_flags_are_slowed() {
        let on = FlagRules {
            throw: true,
            suicide_drops: false,
        };
        assert!(locked_out(on, 5, 1_000, 1_999));
        assert!(!locked_out(on, 5, 1_000, 2_000));
        assert!(!locked_out(FlagRules::default(), 5, 1_000, 1_500));
        assert!(!locked_out(on, 1, 1_000, 1_500));
        assert!(stops(24.0) && stops(-4.0) && !stops(25.0) && !stops(-5.0));
        let mut fast = [1_000.0, 0.0, 10.0];
        drag(&mut fast);
        assert!((fast[0] - 998.828).abs() < 0.001, "{fast:?}");
        let mut slow = [800.0, 0.0, 10.0];
        drag(&mut slow);
        assert_eq!(slow, [800.0, 0.0, 10.0]);
    }
}
