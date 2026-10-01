//! Functional hero menu options, not legacy .menu widget mirrors.
use super::*;

/// Register settings consumed by browser filtering or the Force allocation editor.
pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    for (name, value, help) in [
        (
            "ui_freeSaber",
            0_i64,
            "Free first saber offense/defense levels in the Force menu",
        ),
        (
            "ui_browserShowEmpty",
            1_i64,
            "Show servers with no human players",
        ),
        ("ui_browserShowFull", 1, "Show full servers"),
        (
            "ui_browserShowPasswordProtected",
            1,
            "Show password-protected servers",
        ),
        (
            "ui_browserFilterInvalidInfo",
            1,
            "Hide invalid server information",
        ),
        (
            "ui_gametype",
            -1,
            "Browser mode: -1 all, otherwise protocol gametype",
        ),
        (
            "ui_actualNetGametype",
            -1,
            "Browser network mode, synchronized with ui_gametype",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    // Create game's last choices.
    for (name, value, help) in crate::menu::create_game::INTEGER_CVARS {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    for (name, value, help) in crate::menu::create_game::TEXT_CVARS {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    Ok(())
}
