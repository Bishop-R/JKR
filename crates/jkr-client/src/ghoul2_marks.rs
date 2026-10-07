//! Ghoul2 damage marks as stock's client lays them on players and NPCs: burns where a
//! bolt or a rocket struck (`CG_G2MarkEvent`, OpenJK `codemp/cgame/cg_event.c:1024-1154`)
//! and glowing cuts where a blade touched (`CG_SaberCompWork`, `cg_players.c:5838-5947`),
//! each projected onto the posed model by `CG_AddGhoul2Mark` (`cg_players.c:5778-5836`).
//!
//! A struck missile carries the mark: the server sets `trickedentindex` on its impact
//! event when the struck entity is the one its trace touched, with the point of impact
//! in `origin` and where the missile would have flown on in `origin2`
//! (`g_missile.c:968-987`). A model takes no new mark while it already carries
//! `cg_ghoul2Marks` of them (16), counted per surface marked.
//!
//! Not here: a vehicle weapon's mark (`EF_JETPACK_ACTIVE`), sized and shaded by its
//! `.vwp` file's `g2MarkSize` and `g2MarkShader`, which only the server reads yet.

use jkr_protocol::EntityState;
use std::ops::Range;

/// `cgs.media.bdecal_bodyburn1`, `bdecal_burn1`, `bdecal_saberglow` (`cg_main.c:2539-2541`).
pub const BODY_BURN: &str = "gfx/damage/bodyburnmark1";
pub const BIG_BURN: &str = "gfx/damage/bodybigburnmark1";
pub const SABER_GLOW: &str = "gfx/damage/saberglowmark";
/// Every shader a mark can be drawn with.
pub const SHADERS: [&str; 3] = [BODY_BURN, BIG_BURN, SABER_GLOW];
/// `cg_ghoul2Marks`' default: how many marks a model may carry before it takes no more.
pub const DEFAULT_LIMIT: usize = 16;
/// `cg_g2TraceLod`: the LOD a client's Ghoul2 trace tests.
pub const TRACE_LOD: usize = 2;
/// The LODs a mark is laid at (`G2API_AddSkinGore`: from `r_lodbias`, 0, below the third).
pub const MARK_LODS: Range<usize> = 0..3;
/// `CG_MAX_SABER_COMP_TIME`: how long after the server's saber hit the client keeps marking.
pub const SABER_COMPENSATION_MS: i32 = 400;

/// `EV_MISSILE_HIT`, `EV_MISSILE_MISS`; `EF_JETPACK_ACTIVE` (a vehicle's projectile).
const EV_MISSILE_HIT: u16 = 85;
const EV_MISSILE_MISS: u16 = 86;
const EF_JETPACK_ACTIVE: u32 = 1 << 11;
/// `WP_*` numbers.
const WP_BRYAR_PISTOL: u8 = 4;
const WP_BLASTER: u8 = 5;
const WP_DISRUPTOR: u8 = 6;
const WP_BOWCASTER: u8 = 7;
const WP_REPEATER: u8 = 8;
const WP_ROCKET_LAUNCHER: u8 = 11;
const WP_THERMAL: u8 = 12;
const WP_CONCUSSION: u8 = 15;
const WP_BRYAR_OLD: u8 = 16;
const WP_TURRET: u8 = 18;

/// How a mark looks and lasts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkStyle {
    pub shader: &'static str,
    /// How far the mark reaches from the hit, at least and at most (`flrand`).
    pub size: (f32, f32),
    /// How long it stays, in milliseconds, at least and at most (`Q_irand`).
    pub lifetime_ms: (i32, i32),
}

/// A saber's mark (`CG_SaberCompWork`): a glowing cut for five to ten seconds.
pub fn saber_style() -> MarkStyle {
    MarkStyle {
        shader: SABER_GLOW,
        size: (3.0, 4.0),
        lifetime_ms: (5_000, 10_000),
    }
}

/// A missile's mark request, read from its impact event (`CG_G2MarkEvent`).
#[derive(Clone, Debug, PartialEq)]
pub struct MissileMark {
    /// The struck entity, whose model takes the mark (`otherEntityNum`).
    pub target: u16,
    /// Where the missile struck (`s.origin`).
    pub origin: [f32; 3],
    /// Where it would have flown on (`origin2`): the shot runs from the struck point here.
    pub end: [f32; 3],
    /// `eventParm` set: trace from `origin` to `end` for the point on the model first.
    pub trace: bool,
    pub style: MarkStyle,
}

/// The mark an impact event entity asks for, if any: a missile's hit or miss with the
/// server's mark flag, from a weapon that leaves one.
pub fn missile_mark(state: &EntityState) -> Option<MissileMark> {
    let event = state.event() & !0x300;
    if !matches!(event, EV_MISSILE_HIT | EV_MISSILE_MISS) || state.tracked_entity_num() == 0 {
        return None;
    }
    if state.e_flags() & EF_JETPACK_ACTIVE != 0 {
        return None;
    }
    Some(MissileMark {
        target: state.other_entity_num(),
        origin: state.event_origin(),
        end: state.origin2(),
        trace: state.event_parameter() != 0,
        style: weapon_style(state.weapon())?,
    })
}

/// The mark a weapon's missile leaves (`cg_event.c:1104-1153`): a small burn for bolts, a
/// large one for rockets and thermals, ten to twenty seconds; none for the rest.
pub fn weapon_style(weapon: u8) -> Option<MarkStyle> {
    let (shader, size) = match weapon {
        WP_BRYAR_PISTOL | WP_CONCUSSION | WP_BRYAR_OLD | WP_BLASTER | WP_DISRUPTOR
        | WP_BOWCASTER | WP_REPEATER | WP_TURRET => (BODY_BURN, 4.0),
        WP_ROCKET_LAUNCHER | WP_THERMAL => (BIG_BURN, 24.0),
        _ => return None,
    };
    Some(MarkStyle {
        shader,
        size: (size, size),
        lifetime_ms: (10_000, 20_000),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_protocol::LEGACY_ENTITY_FIELDS;

    /// `entityState_t` field indices (`LEGACY_ENTITY_FIELDS`).
    const ES_EVENT: usize = 28;
    const ES_EVENT_PARM: usize = 42;
    const ES_WEAPON: usize = 14;
    const ES_OTHER_ENTITY_NUM: usize = 59;
    const ES_TRICKED: usize = 58;
    const ES_EFLAGS: usize = 19;

    fn impact(event: u32, weapon: u32, flagged: bool) -> EntityState {
        let mut state = EntityState::zero(300, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(ES_EVENT, event | 0x100);
        state.set_raw_field(ES_EVENT_PARM, 5);
        state.set_raw_field(ES_WEAPON, weapon);
        state.set_raw_field(ES_OTHER_ENTITY_NUM, 3);
        state.set_raw_field(ES_TRICKED, u32::from(flagged));
        state
    }

    #[test]
    fn a_flagged_bolt_or_rocket_marks_its_target_and_nothing_else_does() {
        let bolt = missile_mark(&impact(85, 5, true)).unwrap();
        assert_eq!((bolt.target, bolt.trace), (3, true));
        assert_eq!(
            (bolt.style.shader, bolt.style.size),
            (BODY_BURN, (4.0, 4.0))
        );
        let rocket = missile_mark(&impact(86, 11, true)).unwrap();
        assert_eq!(
            (rocket.style.shader, rocket.style.size),
            (BIG_BURN, (24.0, 24.0))
        );
        // No flag, a weapon that leaves no mark, another event: nothing.
        assert_eq!(missile_mark(&impact(85, 5, false)), None);
        assert_eq!(missile_mark(&impact(85, 10, true)), None);
        assert_eq!(missile_mark(&impact(87, 5, true)), None);
        // A vehicle's projectile is left to the vehicle weapon table.
        let mut vehicle = impact(85, 5, true);
        vehicle.set_raw_field(ES_EFLAGS, EF_JETPACK_ACTIVE);
        assert_eq!(missile_mark(&vehicle), None);
    }
}
