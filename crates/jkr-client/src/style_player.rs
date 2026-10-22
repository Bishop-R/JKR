//! jaPRO's `cg_stylePlayer` (EternalJK `cg_local.h:126-144`): bits that change how other
//! players are drawn — fullbright skins, the duel shell, hiding duellists or everyone but
//! one's duel partner, duellists drawn as `duelShader`, one's own ysalamiri shell hidden,
//! spawn bubbles in team colours. JKR's default keeps stock's look: stock always draws the
//! duel shell, which EternalJK draws only with [`SHELL`].
//!
//! Not yet carried: the racing bits (3 to 7, for jaPRO's race mode), the player LOD (12),
//! cosmetics (16), breathing (17) and the old grapple line (18).

use crate::ServerMod;

/// `JAPRO_STYLE_FULLBRIGHT`: players drawn at full brightness.
pub const FULLBRIGHT: u32 = 1 << 0;
/// `JAPRO_STYLE_SHELL`: the duel shell, and the duellists' tint.
pub const SHELL: u32 = 1 << 1;
/// `JAPRO_STYLE_HIDEDUELERS1`: duellists hidden from a player not duelling.
pub const HIDE_DUELLISTS: u32 = 1 << 2;
/// `JAPRO_STYLE_VFXDUELERS`: duellists drawn as `gfx/effects/duelShader`.
pub const DUELLIST_STAND_IN: u32 = 1 << 8;
/// `JAPRO_STYLE_HIDENONDUELERS`: everyone but the partner hidden in a duel (on servers
/// other than JA+ and jaPRO, which hide them anyway).
pub const HIDE_NON_DUELLISTS: u32 = 1 << 10;
/// `JAPRO_STYLE_HIDEYSALSHELL`: one's own ysalamiri shell hidden.
pub const HIDE_OWN_YSALAMIRI: u32 = 1 << 11;
/// `JAPRO_STYLE_NOBODIES`: bodies fade as soon as they fall and are gone in five seconds,
/// with no cut-off limbs.
pub const NO_BODIES: u32 = 1 << 13;
/// `JAPRO_STYLE_NOFADESFX`: a fading body's crackle, hum and electric shell left out.
pub const NO_FADE_EFFECTS: u32 = 1 << 14;
/// `JAPRO_STYLE_COLOREDSPAWNBUBBLE`: the spawn bubble in the player's team colour.
pub const TEAM_SPAWN_BUBBLE: u32 = 1 << 15;
/// JKR's default `cg_stylePlayer`: stock's look.
pub const DEFAULT: u32 = SHELL;

/// `cgs.media.duelShader`, carried by JKR (`assets/shaders/eternaljk`).
pub const DUEL_SHADER: &str = "gfx/effects/duelShader";

/// What the local player's view is: in a duel and with whom, and whether it spectates in
/// free flight (`TEAM_SPECTATOR` without `PMF_FOLLOW`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalView {
    pub duelling: bool,
    pub partner: u16,
    pub free_spectator: bool,
}

/// Whether another player (`number`, its `bolt1`) is hidden (`CG_Player`,
/// `cg_players.c:10206-10230`): in a duel, everyone but the partner on JA+ and jaPRO
/// servers or with [`HIDE_NON_DUELLISTS`]; out of one, duellists with [`HIDE_DUELLISTS`].
pub fn hides(style: u32, server: ServerMod, view: LocalView, number: u16, bolt1: u32) -> bool {
    if view.duelling {
        number != view.partner
            && !view.free_spectator
            && (matches!(server, ServerMod::JaPlus | ServerMod::JaPro)
                || style & HIDE_NON_DUELLISTS != 0)
    } else {
        style & HIDE_DUELLISTS != 0 && bolt1 == 1
    }
}

/// Whether another player in a duel the local player is not in is drawn as the duel
/// stand-in instead of itself (`cg_players.c:12517-12560`).
pub fn duellist_stand_in(style: u32, view: LocalView, bolt1: u32) -> bool {
    !view.duelling && bolt1 == 1 && style & DUELLIST_STAND_IN != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duels_hide_as_the_mod_and_style_ask() {
        let duel = LocalView {
            duelling: true,
            partner: 4,
            free_spectator: false,
        };
        // A base server keeps bystanders unless asked; JA+ and jaPRO hide them.
        assert!(!hides(DEFAULT, ServerMod::BaseJka, duel, 7, 0));
        assert!(hides(HIDE_NON_DUELLISTS, ServerMod::BaseJka, duel, 7, 0));
        assert!(hides(DEFAULT, ServerMod::JaPro, duel, 7, 0));
        assert!(!hides(DEFAULT, ServerMod::JaPro, duel, 4, 1));
        // Out of a duel, duellists only when asked.
        let free = LocalView::default();
        assert!(!hides(DEFAULT, ServerMod::BaseJka, free, 7, 1));
        assert!(hides(HIDE_DUELLISTS, ServerMod::BaseJka, free, 7, 1));
        assert!(duellist_stand_in(DUELLIST_STAND_IN, free, 1));
        assert!(!duellist_stand_in(DUELLIST_STAND_IN, duel, 1));
    }
}
