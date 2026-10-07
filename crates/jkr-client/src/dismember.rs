//! A cut limb as stock's client shows it (`CG_General`'s client-limb case,
//! `cg_ents.c:1019-1308`; `CG_ReattachLimb`, `cg_event.c:712-785`). The server sends the
//! limb as a general entity (`s.weapon` `G2_MODEL_PART`, the part in `s.modelGhoul2`, its
//! owner in `s.modelindex`); the client copies the owner's posed model for it, keeps only
//! what lies below the limb's surface with its cap on, and turns the limb about one of its
//! bones; the owner loses the limb with all below it and shows the stump's cap. Once the
//! owner lives again the model is whole.

use jkr_game_jka::pmove_roll_anim::death;

/// `G2_MODEL_PART`: a limb's `s.weapon` (`bg_public.h`).
pub const G2_MODEL_PART: u8 = 50;
/// `G2_MODELPART_HEAD` .. `G2_MODELPART_RLEG`: a limb's `s.modelGhoul2`.
pub const HEAD: u8 = 10;
pub const WAIST: u8 = 11;
pub const LEFT_ARM: u8 = 12;
pub const RIGHT_ARM: u8 = 13;
pub const RIGHT_HAND: u8 = 14;
pub const LEFT_LEG: u8 = 15;
pub const RIGHT_LEG: u8 = 16;
/// `BOTH_RIGHTHANDCHOPPEDOFF`: the one living pose a limb may leave.
const BOTH_RIGHTHANDCHOPPEDOFF: u16 = 1_254;

/// The surfaces and bones of one cut on the owner's model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cut {
    /// The bone the limb turns about (`G2API_SetNewOrigin`).
    pub rotate_bone: &'static str,
    /// The limb's root surface, its own cap, and the cap left on the stump.
    pub limb: String,
    pub limb_cap: String,
    pub stub_cap: String,
    /// The bolts the smoke rises from, on the limb and on the stump.
    pub limb_tag: &'static str,
    pub stub_tag: &'static str,
}

/// The cut of `part` on a model whose surface names `variant` resolves as
/// `BG_GetRootSurfNameWithVariant` does; `humanoid` is a `localAnimIndex` of at most one
/// (the humanoid and rockettrooper skeletons turn a trunk about its thoracic bone).
pub fn cut(part: u8, humanoid: bool, variant: impl Fn(&str) -> String) -> Cut {
    let paired = |limb: &str, stub: &str, rotate, cap_on_limb: &str, cap_on_stub: &str, tags| {
        let (limb, stub) = (variant(limb), variant(stub));
        let (limb_tag, stub_tag) = tags;
        Cut {
            rotate_bone: rotate,
            limb_cap: format!("{limb}_cap_{cap_on_limb}"),
            stub_cap: format!("{stub}_cap_{cap_on_stub}"),
            limb,
            limb_tag,
            stub_tag,
        }
    };
    match part {
        HEAD => Cut {
            rotate_bone: "cranium",
            limb: "head".to_owned(),
            limb_cap: "head_cap_torso".to_owned(),
            stub_cap: "torso_cap_head".to_owned(),
            limb_tag: "*head_cap_torso",
            stub_tag: "*torso_cap_head",
        },
        WAIST => Cut {
            rotate_bone: if humanoid { "thoracic" } else { "pelvis" },
            limb: "torso".to_owned(),
            limb_cap: "torso_cap_hips".to_owned(),
            stub_cap: "hips_cap_torso".to_owned(),
            limb_tag: "*torso_cap_hips",
            stub_tag: "*hips_cap_torso",
        },
        LEFT_ARM => paired(
            "l_arm",
            "torso",
            "lradius",
            "torso",
            "l_arm",
            ("*l_arm_cap_torso", "*torso_cap_l_arm"),
        ),
        RIGHT_ARM => paired(
            "r_arm",
            "torso",
            "rradius",
            "torso",
            "r_arm",
            ("*r_arm_cap_torso", "*torso_cap_r_arm"),
        ),
        RIGHT_HAND => paired(
            "r_hand",
            "r_arm",
            "rhand",
            "r_arm",
            "r_hand",
            ("*r_hand_cap_r_arm", "*r_arm_cap_r_hand"),
        ),
        LEFT_LEG => paired(
            "l_leg",
            "hips",
            "ltibia",
            "hips",
            "l_leg",
            ("*l_leg_cap_hips", "*hips_cap_l_leg"),
        ),
        // The right leg, and anything else ("same as on server").
        _ => paired(
            "r_leg",
            "hips",
            "rtibia",
            "hips",
            "r_leg",
            ("*r_leg_cap_hips", "*hips_cap_r_leg"),
        ),
    }
}

/// Whether `cg_dismember` at `setting` shows a cut of `part`: none at 0, and heads and
/// trunks only from 2.
pub fn shown(part: u8, setting: i64) -> bool {
    setting > 0 && (setting >= 2 || !matches!(part, HEAD | WAIST))
}

/// Whether a cut of `part` takes the hand that holds the weapon.
pub fn disarms(part: u8) -> bool {
    matches!(part, RIGHT_ARM | RIGHT_HAND | WAIST)
}

/// `torsoBolt`'s bit for `part`.
pub fn bit(part: u8) -> u8 {
    part.checked_sub(HEAD)
        .filter(|index| *index < 8)
        .map_or(0, |index| 1 << index)
}

/// Whether the owner is ready to lose a limb: flagged dead, its torso playing a death
/// animation both in its state and as this client drew it last, or the living pose of a
/// hand cut off.
pub fn owner_ready(dead: bool, torso_animation: u16, drawn_torso_animation: u16) -> bool {
    dead && ((death(torso_animation) && death(drawn_torso_animation))
        || torso_animation == BOTH_RIGHTHANDCHOPPEDOFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_names_the_owner_s_variant_surfaces_and_their_caps() {
        let variant = |name: &str| {
            if name == "torso" {
                "torsoa".to_owned()
            } else {
                name.to_owned()
            }
        };
        let arm = cut(LEFT_ARM, true, variant);
        assert_eq!(
            (arm.rotate_bone, arm.limb.as_str(), arm.limb_cap.as_str()),
            ("lradius", "l_arm", "l_arm_cap_torso")
        );
        assert_eq!(arm.stub_cap, "torsoa_cap_l_arm");
        assert_eq!(arm.stub_tag, "*torso_cap_l_arm");
        let trunk = cut(WAIST, false, variant);
        assert_eq!(
            (trunk.rotate_bone, trunk.limb.as_str()),
            ("pelvis", "torso")
        );
        assert_eq!(cut(99, true, variant).limb, "r_leg");
    }

    #[test]
    fn heads_and_trunks_need_the_higher_setting_and_the_sword_hand_takes_the_weapon() {
        assert!(!shown(LEFT_LEG, 0));
        assert!(shown(LEFT_LEG, 1));
        assert!(!shown(HEAD, 1));
        assert!(shown(WAIST, 2));
        assert!(disarms(RIGHT_HAND) && !disarms(LEFT_ARM));
        assert_eq!((bit(HEAD), bit(RIGHT_LEG)), (1, 64));
    }
}
