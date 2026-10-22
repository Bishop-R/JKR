//! Team voice-command whitelist (`bg_customSiegeSoundNames`, codemp).

/// Custom sound names accepted by `Cmd_VoiceCommand_f`, in reference order.
pub const SOUNDS: [&str; 29] = [
    "*att_attack",
    "*att_primary",
    "*att_second",
    "*def_guns",
    "*def_position",
    "*def_primary",
    "*def_second",
    "*reply_coming",
    "*reply_go",
    "*reply_no",
    "*reply_stay",
    "*reply_yes",
    "*req_assist",
    "*req_demo",
    "*req_hvy",
    "*req_medic",
    "*req_sup",
    "*req_tech",
    "*spot_air",
    "*spot_defenses",
    "*spot_emplaced",
    "*spot_sniper",
    "*spot_troops",
    "*tac_cover",
    "*tac_fallback",
    "*tac_follow",
    "*tac_hold",
    "*tac_split",
    "*tac_together",
];

/// `cg_stringEdVoiceChatTable`: MENUS string-table references matching [`SOUNDS`].
pub const LABELS: [&str; SOUNDS.len()] = [
    "VC_ATT",
    "VC_ATT_PRIMARY",
    "VC_ATT_SECONDARY",
    "VC_DEF_GUNS",
    "VC_DEF_POSITION",
    "VC_DEF_PRIMARY",
    "VC_DEF_SECONDARY",
    "VC_REPLY_COMING",
    "VC_REPLY_GO",
    "VC_REPLY_NO",
    "VC_REPLY_STAY",
    "VC_REPLY_YES",
    "VC_REQ_ASSIST",
    "VC_REQ_DEMO",
    "VC_REQ_HVY",
    "VC_REQ_MEDIC",
    "VC_REQ_SUPPLY",
    "VC_REQ_TECH",
    "VC_SPOT_AIR",
    "VC_SPOT_DEF",
    "VC_SPOT_EMPLACED",
    "VC_SPOT_SNIPER",
    "VC_SPOT_TROOP",
    "VC_TAC_COVER",
    "VC_TAC_FALLBACK",
    "VC_TAC_FOLLOW",
    "VC_TAC_HOLD",
    "VC_TAC_SPLIT",
    "VC_TAC_TOGETHER",
];

/// Resolve an unprefixed `voice_cmd` argument. Arbitrary sounds are refused.
pub fn command_sound(argument: &[u8]) -> Option<&'static str> {
    SOUNDS
        .iter()
        .copied()
        .find(|name| name.as_bytes()[1..].eq_ignore_ascii_case(argument))
}

/// Resolve a configstring's exact custom sound name, without its optional extension.
pub fn sound_index(name: &str) -> Option<usize> {
    let name = name.split('.').next()?;
    SOUNDS
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(name))
}
