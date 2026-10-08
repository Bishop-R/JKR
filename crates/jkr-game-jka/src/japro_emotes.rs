//! jaPRO's emotes (`Cmd_Emote*_f` and `DoEmote`, `g_cmds.c:6297-6341`; the hold in
//! `ClientThink_real`'s movement chain, `g_active.c:3987-4014`): in free-for-all a player
//! strikes a pose by command, sitting, kneeling, dancing or signalling, the saber put away
//! for some. A held pose freezes the player until it moves, attacks or dashes, is gripped
//! or is held by a creature. Each emote belongs to a family `g_emotesDisable` can turn off.
//! `amrun` (a running style jaPRO's own client predicts) is not ported.

use crate::generic_commands::SaberSound;
use crate::pmove::Predictor;
use crate::pmove_anim::{
    SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_HOLDLESS, SETANIM_FLAG_OVERRIDE,
};
use jkr_protocol::{PlayerState, UserCommand};

/// One emote: its command, its family's `g_emotesDisable` bit, its animation, whether it is
/// held (`freeze`) and whether it puts the saber away first (`nosaber`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emote {
    pub command: &'static str,
    pub family: u8,
    pub animation: &'static str,
    pub hold: bool,
    pub holster: bool,
}

/// `emote_type_t`'s families (`g_local.h:404-427`).
const E_BEG: u8 = 0;
const E_BEG2: u8 = 1;
const E_BREAKDANCE: u8 = 2;
const E_CHEER: u8 = 3;
const E_COWER: u8 = 4;
const E_DANCE: u8 = 5;
const E_HUG: u8 = 6;
const E_NOISY: u8 = 7;
const E_POINT: u8 = 8;
const E_RAGE: u8 = 9;
const E_SIT: u8 = 10;
const E_SURRENDER: u8 = 11;
const E_SMACK: u8 = 12;
const E_TAUNT: u8 = 13;
const E_VICTORY: u8 = 14;
const E_BERNIE: u8 = 16;
const E_SLEEP: u8 = 17;
/// `E_SABERFLIP`: `amflip`, which picks its pose by saber style ([`saber_flip`]).
pub const E_SABERFLIP: u8 = 18;
const E_SLAP: u8 = 19;
const E_SIGNAL: u8 = 20;

const fn emote(
    command: &'static str,
    family: u8,
    animation: &'static str,
    hold: bool,
    holster: bool,
) -> Emote {
    Emote {
        command,
        family,
        animation,
        hold,
        holster,
    }
}

/// jaPRO's command table's emotes (`g_cmds.c:8869-8922`), `amflip` and `amrun` apart.
pub const EMOTES: [Emote; 35] = [
    emote("ambeg", E_BEG, "BOTH_KNEES1", true, true),
    emote("ambeg2", E_BEG2, "BOTH_KNEES2", true, true),
    emote(
        "ambernie",
        E_BERNIE,
        "BOTH_FORCE_DRAIN_GRABBED",
        true,
        false,
    ),
    emote(
        "ambreakdance",
        E_BREAKDANCE,
        "BOTH_FORCE_GETUP_B2",
        false,
        true,
    ),
    emote(
        "ambreakdance2",
        E_BREAKDANCE,
        "BOTH_FORCE_GETUP_B4",
        false,
        true,
    ),
    emote(
        "ambreakdance3",
        E_BREAKDANCE,
        "BOTH_FORCE_GETUP_B5",
        false,
        true,
    ),
    emote(
        "ambreakdance4",
        E_BREAKDANCE,
        "BOTH_FORCE_GETUP_B6",
        false,
        true,
    ),
    emote("amcheer", E_CHEER, "BOTH_TUSKENTAUNT1", false, true),
    emote("amcower", E_COWER, "BOTH_COWER1", false, true),
    emote("amdance", E_DANCE, "BOTH_STEADYSELF1", false, true),
    emote("amhug", E_HUG, "BOTH_HUGGER1", false, true),
    emote("amnoisy", E_NOISY, "BOTH_SONICPAIN_HOLD", false, true),
    emote("ampoint", E_POINT, "BOTH_STAND5TOAIM", true, false),
    emote("amrage", E_RAGE, "BOTH_FORCE_RAGE", false, false),
    emote("amsignal", E_SIGNAL, "TORSO_HANDSIGNAL1", false, false),
    emote("amsignal2", E_SIGNAL, "TORSO_HANDSIGNAL2", false, false),
    emote("amsignal3", E_SIGNAL, "TORSO_HANDSIGNAL3", false, false),
    emote("amsignal4", E_SIGNAL, "TORSO_HANDSIGNAL4", false, false),
    emote("amsit", E_SIT, "BOTH_SIT1", true, true),
    emote("amsit2", E_SIT, "BOTH_SIT3", true, true),
    emote("amsit3", E_SIT, "BOTH_SIT6", true, true),
    emote("amsit4", E_SIT, "BOTH_SIT2", true, true),
    emote("amsit5", E_SIT, "BOTH_SLEEP6STOP", true, true),
    emote("amslap", E_SLAP, "BOTH_FORCEGRIP3THROW", false, false),
    emote("amsleep", E_SLEEP, "BOTH_SLEEP1", true, true),
    emote("amsmack", E_SMACK, "BOTH_TOSS1", false, false),
    emote(
        "amsurrender",
        E_SURRENDER,
        "TORSO_SURRENDER_START",
        true,
        true,
    ),
    emote("amtaunt", E_TAUNT, "BOTH_ALORA_TAUNT", false, false),
    emote("amtaunt2", E_TAUNT, "BOTH_SCEPTER_HOLD", false, false),
    emote(
        "amvictory",
        E_VICTORY,
        "BOTH_TAVION_SCEPTERGROUND",
        false,
        false,
    ),
    // `Cmd_EmoteSaberFlip_f`'s poses, chosen by [`saber_flip`].
    emote("", E_SABERFLIP, "BOTH_STAND1TO2", false, false),
    emote("", E_SABERFLIP, "BOTH_STAND2TO1", false, false),
    emote("", E_SABERFLIP, "BOTH_S1_S7", false, false),
    emote("", E_SABERFLIP, "BOTH_SHOWOFF_STAFF", false, false),
    emote("", E_SABERFLIP, "BOTH_SHOWOFF_FAST", false, false),
];

/// How `aminfo` names each family, in its order: jaPRO's list but `amRun` (the Jawa
/// run, not ported).
pub const HELP_NAMES: [(u8, &str); 20] = [
    (E_BEG, "amBeg"),
    (E_BEG2, "amBeg2"),
    (E_BERNIE, "amBernie"),
    (E_BREAKDANCE, "amBreakdance[2/3/4]"),
    (E_CHEER, "amCheer"),
    (E_COWER, "amCower"),
    (E_DANCE, "amDance"),
    (E_HUG, "amHug"),
    (E_SABERFLIP, "amFlip"),
    (E_NOISY, "amNoisy"),
    (E_POINT, "amPoint"),
    (E_RAGE, "amRage"),
    (E_SIGNAL, "amSignal[2/3/4]"),
    (E_SIT, "amSit[2/3/4/5]"),
    (E_SLAP, "amSlap"),
    (E_SLEEP, "amSleep"),
    (E_SURRENDER, "amSurrender"),
    (E_SMACK, "amSmack"),
    (E_TAUNT, "amTaunt[2]"),
    (E_VICTORY, "amVictory"),
];

/// The emote command `command` names, any case.
pub fn find(command: &[u8]) -> Option<&'static Emote> {
    EMOTES.iter().find(|emote| {
        !emote.command.is_empty() && emote.command.as_bytes().eq_ignore_ascii_case(command)
    })
}

/// The pose `amflip` strikes for saber style `style` (`SS_FAST` 1 .. `SS_STAFF` 7), the
/// saber off (`holstered`) or on: drawn or put away for the single styles, twirled for the
/// staff, flourished for dual sabers; `None` for any other style.
pub fn saber_flip(style: u8, holstered: bool) -> Option<&'static Emote> {
    let name = match (style, holstered) {
        (1..=3, true) => "BOTH_STAND1TO2",
        (1..=3, false) => "BOTH_STAND2TO1",
        (7, true) => "BOTH_S1_S7",
        (7, false) => "BOTH_SHOWOFF_STAFF",
        (6, _) => "BOTH_SHOWOFF_FAST",
        _ => return None,
    };
    EMOTES
        .iter()
        .find(|emote| emote.family == E_SABERFLIP && emote.animation == name)
}

/// The message for a family `g_emotesDisable` turns off.
pub const DISABLED: &str = "This emote is not allowed on this server.\n";

/// Why `DoEmote` refuses: nothing said, or the message the player is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Quiet,
    Told(&'static str),
}

/// `PlayerState` fields the emotes read and write.
const PS_SABER_MOVE: usize = 34;
const PS_SABER_CAN_THROW: usize = 49;
const PS_SABER_BLOCKED: usize = 77;
const PS_SABER_HOLSTERED: usize = 81;
const PS_WEAPON_TIME: usize = 10;
const PS_EFLAGS2: usize = 103;
const PS_FORCE_GRIP_CRIPPLE: usize = 111;
const PS_FORCE_RESTRICTED: usize = 115;
/// `WP_SABER`, `ENTITYNUM_NONE`, `GT_FFA`, `EF2_HELD_BY_MONSTER`, `PM_FREEZE`, `PM_NORMAL`.
const WP_SABER: u8 = 3;
const ENTITYNUM_NONE: u16 = 1_023;
const GT_FFA: i32 = 0;
const EF2_HELD_BY_MONSTER: u32 = 1 << 0;
pub const PM_FREEZE: u8 = 6;
const PM_NORMAL: u8 = 0;
/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`, and jaPRO's `BUTTON_DASH`, which end a hold.
const RELEASE_BUTTONS: u16 = 1 | 128 | 8_192;

/// `DoEmote`'s refusals, in its order, for a player in `state` whose legs play `legs` with
/// `legs_timer` left, in game type `gametype`: busy (a swing, a saber move or a Force power),
/// in the air, duelling, knocked down, rolling, or outside free-for-all.
pub fn refusal(
    state: &PlayerState,
    (legs, legs_timer): (u16, i32),
    gametype: i32,
) -> Option<Refusal> {
    if state.weapon_time() > 1 || state.saber_move() > 1 || state.force_powers_active() != 0 {
        return Some(Refusal::Quiet);
    }
    if state.ground_entity_num() == ENTITYNUM_NONE {
        return Some(Refusal::Quiet);
    }
    if state.duel_in_progress() {
        return Some(Refusal::Told("^7Emotes not allowed in duel!\n"));
    }
    if crate::knockdown::in_knockdown(legs)
        || (crate::pmove_roll_anim::in_roll(legs) && legs_timer > 0)
    {
        return Some(Refusal::Quiet);
    }
    (gametype != GT_FFA).then_some(Refusal::Told("^7Emotes not allowed in this gametype!\n"))
}

/// A player's hold (`emote_freeze`), and a release that ends the freeze at the next think.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hold {
    pub held: bool,
    released: bool,
}

/// `DoEmote` once allowed: a held pose toggled (ended if the legs already play it), the saber
/// put away where the emote wants it, and the pose played on the whole body, held for its
/// length. Saber sounds go to `sounds`.
pub fn begin(
    emote: &Emote,
    state: &mut PlayerState,
    movement: &mut Predictor,
    hold: &mut Hold,
    sounds: &mut Vec<SaberSound>,
) {
    let Some(animation) =
        crate::legacy_animation_index(emote.animation).and_then(|index| u16::try_from(index).ok())
    else {
        return;
    };
    if emote.hold {
        hold.held = state.leg_animation() != animation;
    }
    let mut blocking = None;
    if emote.holster && state.weapon() == WP_SABER && state.saber_holstered() < 2 {
        state.set_raw_field(PS_SABER_CAN_THROW, 0);
        state.set_raw_field(PS_SABER_MOVE, 0);
        state.set_raw_field(PS_SABER_BLOCKED, 0);
        state.set_raw_field(PS_SABER_HOLSTERED, 2);
        sounds.extend(SaberSound::both(false));
        blocking = Some(0);
    }
    *movement = movement.reseeded(state);
    if let Some(mode) = blocking {
        movement.set_saber_blocking(mode);
    }
    movement.set_animation_parts(
        SETANIM_BOTH,
        animation,
        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_HOLDLESS,
    );
    movement.write_player_state(state);
}

/// The hold's place in `ClientThink_real`'s movement chain, for a living player neither in
/// noclip nor gripped: the movement type to set, if any. A move, an attack or a dash, a
/// crippling grip or a creature's hold ends it (the freeze lasting out this think, as the
/// chain sets no type then); otherwise the player is frozen, its Force restricted and its
/// saber put away.
pub fn hold(
    state: &mut PlayerState,
    movement: &mut Predictor,
    command: &UserCommand,
    hold: &mut Hold,
    sounds: &mut Vec<SaberSound>,
) -> Option<u8> {
    if !hold.held {
        if std::mem::take(&mut hold.released) && state.movement_type() == PM_FREEZE {
            return Some(PM_NORMAL);
        }
        return None;
    }
    let field = |state: &PlayerState, index| state.raw_field(index).unwrap_or(0);
    let released = field(state, PS_FORCE_GRIP_CRIPPLE) != 0
        || command.forward_move != 0
        || command.right_move != 0
        || command.up_move != 0
        || field(state, PS_EFLAGS2) == EF2_HELD_BY_MONSTER
        || command.buttons & RELEASE_BUTTONS != 0;
    if released {
        *hold = Hold {
            held: false,
            released: true,
        };
        state.set_raw_field(PS_SABER_CAN_THROW, 1);
        state.set_raw_field(PS_SABER_BLOCKED, 1);
        state.set_raw_field(PS_FORCE_RESTRICTED, 0);
        *movement = movement.reseeded(state);
        movement.set_saber_blocking(1);
        return None;
    }
    state.set_raw_field(PS_FORCE_RESTRICTED, 1);
    if state.weapon() == WP_SABER && state.saber_holstered() < 2 {
        sounds.extend(SaberSound::both(false));
        state.set_raw_field(PS_SABER_HOLSTERED, 2);
        state.set_raw_field(PS_WEAPON_TIME, 400);
    }
    *movement = movement.reseeded(state);
    Some(PM_FREEZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every pose names an animation the table has, and every command is found by any case.
    #[test]
    fn every_emote_names_a_known_animation() {
        for emote in EMOTES {
            assert!(
                crate::legacy_animation_index(emote.animation).is_some(),
                "{}",
                emote.animation
            );
            if !emote.command.is_empty() {
                assert_eq!(find(emote.command.to_uppercase().as_bytes()), Some(&emote));
            }
        }
        assert_eq!(find(b"amsheev"), None);
        assert_eq!(
            saber_flip(7, true).map(|emote| emote.animation),
            Some("BOTH_S1_S7")
        );
        assert_eq!(saber_flip(4, true), None);
    }

    /// Only a still, grounded player outside a duel strikes a pose, and only in
    /// free-for-all.
    #[test]
    fn a_busy_airborne_or_duelling_player_is_refused() {
        let mut state = PlayerState::zero();
        state.set_raw_field(PS_WEAPON_TIME, 0);
        let standing = (0, 0);
        // `PlayerState::zero` stands on entity 0.
        assert_eq!(refusal(&state, standing, GT_FFA), None);
        assert_eq!(
            refusal(&state, standing, 3),
            Some(Refusal::Told("^7Emotes not allowed in this gametype!\n"))
        );
        state.set_raw_field(PS_WEAPON_TIME, 2);
        assert_eq!(refusal(&state, standing, GT_FFA), Some(Refusal::Quiet));
    }
}
