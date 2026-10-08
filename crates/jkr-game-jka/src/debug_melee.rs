//! `g_debugMelee`'s game half: the grapple a melee grab begins (`G_GrabSomeMofos` and the
//! hold in `WP_SaberPositionUpdate`, `w_saber.c:7976-8089`, `8157-8390`) and letting go of
//! a held wall (`G_LetGoOfWall`, `w_force.c:2885-2901`) — when hurt (`g_combat.c:5519-5525`)
//! or pushed or pulled (`w_force.c:3358-3365`) — with no offensive Force power while
//! holding one (`w_force.c:713-731`). The grab itself starts in the move
//! ([`crate::pmove_melee`]).
//!
//! A grab that closes on a living enemy level with the grabber (within 4 units of height)
//! holds it: forward, the grab punch (two blows of 10, then one of 30 that throws it 500
//! units a second away and 200 up and knocks it down for 1.3 s); backward, the knee throw
//! (20, a pained cry as it hits the ground, then 30). Each blow puts a survivor back in
//! its held pose; the two face each other, the held one kept 46 (punch) or 34 (throw)
//! units in front of its grabber; the hold breaks when either dies, they drift more than
//! 64 units apart or 4 apart in height, or either leaves the poses.

use jkr_protocol::PlayerState;

/// The grapple's animations (`BG_InGrappleMove`, `bg_panimate.c:691-709`).
pub const BOTH_KYLE_GRAB: u16 = 1_282;
pub const BOTH_KYLE_MISS: u16 = 1_283;
pub const BOTH_KYLE_PA_1: u16 = 1_284;
pub const BOTH_PLAYER_PA_1: u16 = 1_285;
pub const BOTH_KYLE_PA_2: u16 = 1_286;
pub const BOTH_PLAYER_PA_2: u16 = 1_287;
pub const BOTH_PLAYER_PA_FLY: u16 = 1_288;
/// The knee throw's last blow flops a body killed by it.
pub const BOTH_DEADFLOP1: u16 = 87;
/// `grappleState` of the one held: the grabber counts its blows from 1.
pub const HELD: i32 = 20;
/// How far apart the two may drift before the hold breaks.
pub const REACH: f32 = 64.0;

/// `pm_flags`' wire field, `PMF_STUCK_TO_WALL`; the animations and their timers.
const PS_PM_FLAGS: usize = 38;
const PMF_STUCK_TO_WALL: u32 = 16_384;
const PS_LEGS_ANIM: usize = 13;
const PS_TORSO_ANIM: usize = 15;
const PS_TORSO_TIMER: usize = 20;
const PS_LEGS_TIMER: usize = 21;
/// The wall's rebound and hold poses (`BG_InReboundJump`, `BG_InReboundHold`).
const BOTH_FORCEWALLREBOUND_FORWARD: u16 = 875;
const BOTH_FORCEWALLHOLD_RIGHT: u16 = 882;
/// The powers a player holding a wall may not use.
const OFFENSIVE: [usize; 6] = [
    crate::force_powers::FP_GRIP,
    crate::force_powers::FP_LIGHTNING,
    crate::force_powers::FP_DRAIN,
    crate::force_powers::FP_SABER_OFFENSE,
    crate::force_powers::FP_SABER_DEFENSE,
    crate::force_powers::FP_SABER_THROW,
];

/// A client's `grappleIndex` and `grappleState`: whom it grapples, and how far the hold
/// has gone — 0 for none, the grabber's blows from 1, [`HELD`] for the one held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grapple {
    pub index: u16,
    pub state: i32,
}

/// `BG_InGrappleMove`: 1 grabbing, 2 holding, 3 held; 0 for any other animation.
pub fn grapple_move(animation: u16) -> u8 {
    match animation {
        BOTH_KYLE_GRAB | BOTH_KYLE_MISS => 1,
        BOTH_KYLE_PA_1 | BOTH_KYLE_PA_2 => 2,
        BOTH_PLAYER_PA_1 | BOTH_PLAYER_PA_2 | BOTH_PLAYER_PA_FLY => 3,
        _ => 0,
    }
}

/// `G_PrettyCloseIGuess`: `a` and `b` less than `tolerance` apart.
pub fn pretty_close(a: f32, b: f32, tolerance: f32) -> bool {
    a - b < tolerance && a - b > -tolerance
}

/// The hold a grab begins as the grabber moves (`pers.cmd.forwardmove`): the grab punch
/// forward, the knee throw backward — the grabber's animation, then the held one's; none
/// standing still (jaPRO's third, sideways hold is left out: stock has none).
pub fn holds(forward_move: i8) -> Option<(u16, u16)> {
    match forward_move.signum() {
        1 => Some((BOTH_KYLE_PA_1, BOTH_PLAYER_PA_1)),
        -1 => Some((BOTH_KYLE_PA_2, BOTH_PLAYER_PA_2)),
        _ => None,
    }
}

/// How far in front of its grabber the held one is kept, by the grabber's hold.
pub fn hold_distance(grabber_torso: u16) -> f32 {
    if grabber_torso == BOTH_KYLE_PA_1 {
        46.0
    } else {
        34.0
    }
}

/// What one beat of the grabber's hold does to the one it holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Beat {
    /// `G_Damage`'s amount (`MOD_MELEE`, no direction, from the grabber's origin); 0 for
    /// none.
    pub damage: i32,
    /// A survivor's animations and timers are put back as they were before the blow (the
    /// torso's on the legs too), lest a pain animation take over.
    pub restore: bool,
    /// The grab punch's last blow: thrown away and, alive, knocked down.
    pub toss: bool,
    /// The knee throw's smash on the ground: `*pain100.wav` on the held one's voice.
    pub cry: bool,
    /// The knee throw's last blow: a survivor's torso held another second and let go, a
    /// dead one flopped.
    pub release: bool,
    /// The grabber's `grappleState` after it.
    pub state: i32,
}

/// The beat of a hold due with the grabber's torso on `torso` with `timer` left, at blow
/// `state`; `None` until one is due, or in neither hold.
pub fn beat(torso: u16, timer: i32, state: i32) -> Option<Beat> {
    let blow = |damage, restore| Beat {
        damage,
        restore,
        state: state + 1,
        ..Beat::default()
    };
    match (torso, state) {
        (BOTH_KYLE_PA_1, 1) => (timer < 3_400).then(|| blow(10, true)),
        (BOTH_KYLE_PA_1, 2) => (timer < 2_550).then(|| blow(10, true)),
        (BOTH_KYLE_PA_1, _) => (timer < 1_300).then_some(Beat {
            damage: 30,
            toss: true,
            ..Beat::default()
        }),
        (BOTH_KYLE_PA_2, 1) => (timer < 3_200).then(|| blow(20, true)),
        (BOTH_KYLE_PA_2, 2) => (timer < 2_000).then(|| Beat {
            cry: true,
            ..blow(0, false)
        }),
        (BOTH_KYLE_PA_2, _) => (timer < 1_000).then_some(Beat {
            damage: 30,
            release: true,
            ..Beat::default()
        }),
        _ => None,
    }
}

/// The grab punch's throw: 500 units a second away from the grabber, 200 up, added to the
/// held one's velocity.
pub fn toss(velocity: [f32; 3], grabber: [f32; 3], held: [f32; 3]) -> [f32; 3] {
    let mut away: [f32; 3] = std::array::from_fn(|axis| held[axis] - grabber[axis]);
    crate::saber_clash::normalize(&mut away);
    let push = [away[0] * 500.0, away[1] * 500.0, 200.0];
    std::array::from_fn(|axis| velocity[axis] + push[axis])
}

/// Whether the player holds a wall (`PMF_STUCK_TO_WALL`).
pub fn stuck_to_wall(state: &PlayerState) -> bool {
    state.raw_field(PS_PM_FLAGS).unwrap_or(0) & PMF_STUCK_TO_WALL != 0
}

/// Whether `power` is one a player holding a wall may not use.
pub fn offensive(power: usize) -> bool {
    OFFENSIVE.contains(&power)
}

/// `G_LetGoOfWall`: the wall let go, and a rebound or hold pose's timer run out.
pub fn let_go_of_wall(state: &mut PlayerState) {
    let flags = state.raw_field(PS_PM_FLAGS).unwrap_or(0);
    state.set_raw_field(PS_PM_FLAGS, flags & !PMF_STUCK_TO_WALL);
    let on_wall = |animation: u32| {
        (u32::from(BOTH_FORCEWALLREBOUND_FORWARD)..=u32::from(BOTH_FORCEWALLHOLD_RIGHT))
            .contains(&animation)
    };
    for (animation, timer) in [
        (PS_LEGS_ANIM, PS_LEGS_TIMER),
        (PS_TORSO_ANIM, PS_TORSO_TIMER),
    ] {
        if on_wall(state.raw_field(animation).unwrap_or(0)) {
            state.set_raw_field(timer, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_are_the_animation_tables() {
        for (number, name) in [
            (BOTH_KYLE_GRAB, "BOTH_KYLE_GRAB"),
            (BOTH_KYLE_MISS, "BOTH_KYLE_MISS"),
            (BOTH_KYLE_PA_1, "BOTH_KYLE_PA_1"),
            (BOTH_PLAYER_PA_1, "BOTH_PLAYER_PA_1"),
            (BOTH_KYLE_PA_2, "BOTH_KYLE_PA_2"),
            (BOTH_PLAYER_PA_2, "BOTH_PLAYER_PA_2"),
            (BOTH_PLAYER_PA_FLY, "BOTH_PLAYER_PA_FLY"),
            (BOTH_DEADFLOP1, "BOTH_DEADFLOP1"),
            (
                BOTH_FORCEWALLREBOUND_FORWARD,
                "BOTH_FORCEWALLREBOUND_FORWARD",
            ),
            (BOTH_FORCEWALLHOLD_RIGHT, "BOTH_FORCEWALLHOLD_RIGHT"),
        ] {
            assert_eq!(
                crate::legacy_animation_name(usize::from(number)),
                Some(name)
            );
        }
    }

    /// The grab punch lands 10 at 3.4 s left, 10 at 2.55 s, and throws with 30 at 1.3 s;
    /// the knee throw 20 at 3.2 s, cries at 2 s, and ends with 30 at 1 s.
    #[test]
    fn the_holds_land_their_blows_on_time() {
        assert_eq!(beat(BOTH_KYLE_PA_1, 3_400, 1), None);
        let first = beat(BOTH_KYLE_PA_1, 3_399, 1).unwrap();
        assert_eq!((first.damage, first.restore, first.state), (10, true, 2));
        assert_eq!(beat(BOTH_KYLE_PA_1, 2_549, 2).unwrap().state, 3);
        let last = beat(BOTH_KYLE_PA_1, 1_299, 3).unwrap();
        assert_eq!((last.damage, last.toss, last.state), (30, true, 0));
        assert_eq!(beat(BOTH_KYLE_PA_2, 3_199, 1).unwrap().damage, 20);
        let cry = beat(BOTH_KYLE_PA_2, 1_999, 2).unwrap();
        assert_eq!((cry.damage, cry.cry, cry.state), (0, true, 3));
        let end = beat(BOTH_KYLE_PA_2, 999, 3).unwrap();
        assert_eq!((end.damage, end.release, end.state), (30, true, 0));
        assert_eq!(beat(BOTH_KYLE_GRAB, 0, 1), None);
        assert_eq!(holds(127), Some((BOTH_KYLE_PA_1, BOTH_PLAYER_PA_1)));
        assert_eq!(holds(-1), Some((BOTH_KYLE_PA_2, BOTH_PLAYER_PA_2)));
        assert_eq!(holds(0), None);
        let thrown = toss([0.0, 0.0, -10.0], [0.0; 3], [0.0, 46.0, 1.0]);
        assert!(
            (thrown[1] - 499.8).abs() < 0.5 && thrown[2] == 190.0,
            "{thrown:?}"
        );
    }

    #[test]
    fn letting_go_ends_the_wall_poses_only() {
        let mut state = PlayerState::default();
        state.set_raw_field(PS_PM_FLAGS, PMF_STUCK_TO_WALL | 64);
        state.set_raw_field(PS_LEGS_ANIM, 880);
        state.set_raw_field(PS_LEGS_TIMER, 150);
        state.set_raw_field(PS_TORSO_ANIM, 122);
        state.set_raw_field(PS_TORSO_TIMER, 150);
        assert!(stuck_to_wall(&state));
        let_go_of_wall(&mut state);
        assert!(!stuck_to_wall(&state));
        assert_eq!(state.raw_field(PS_PM_FLAGS), Some(64));
        assert_eq!(state.raw_field(PS_LEGS_TIMER), Some(0));
        assert_eq!(state.raw_field(PS_TORSO_TIMER), Some(150));
        assert!(offensive(crate::force_powers::FP_GRIP));
        assert!(!offensive(crate::force_powers::FP_PUSH));
    }
}
