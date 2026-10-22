//! Which mod a server runs, as TaystJK reads it from the serverinfo's `gamename`
//! (`cg_servercmds.c:240-291`; `svmod_t`, `cg_local.h:203-208`): client features some mods
//! answer and others do not are gated on it.

/// `svmod_t`, in TaystJK's order: a later mod compares greater (`serverMod >= SVMOD_JAPLUS`).
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ServerMod {
    #[default]
    BaseJka,
    BaseEnhanced,
    OjkAlt,
    Lugormod,
    JaPlus,
    JaPro,
}

impl ServerMod {
    /// The mod serverinfo `info` names by its `gamename` (case-insensitive prefixes as
    /// `Q_stricmpn` compares them); a server speaking a protocol below 26 is base JKA.
    pub fn from_serverinfo(info: &[u8]) -> Self {
        let name = jkr_protocol::info_value(info, b"gamename").unwrap_or_default();
        let starts = |prefix: &[u8]| {
            name.len() >= prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix)
        };
        let protocol =
            jkr_protocol::info_value(info, b"protocol").map_or(0, jkr_game_jka::userinfo::atoi);
        if protocol < 26 {
            return Self::BaseJka;
        }
        if starts(b"JA+") || starts(b"^4U^3A^5Galaxy") || starts(b"AbyssMod") {
            Self::JaPlus
        } else if starts(b"japro") {
            Self::JaPro
        } else if starts(b"smU") {
            Self::OjkAlt
        } else if starts(b"base_enhanced") || starts(b"base_entranced") {
            Self::BaseEnhanced
        } else if name.eq_ignore_ascii_case(b"^5L^7ugormod ^5v3")
            || name.eq_ignore_ascii_case(b"Lugormod")
        {
            Self::Lugormod
        } else {
            Self::BaseJka
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each mod by its `gamename`; an old protocol is base JKA whatever it says.
    #[test]
    fn the_mod_is_read_from_the_gamename() {
        let mod_of = |info: &str| ServerMod::from_serverinfo(info.as_bytes());
        assert_eq!(
            mod_of("\\gamename\\JA+ Mod v1.5\\protocol\\26"),
            ServerMod::JaPlus
        );
        assert_eq!(mod_of("\\gamename\\japro\\protocol\\26"), ServerMod::JaPro);
        assert_eq!(
            mod_of("\\gamename\\base_entranced\\protocol\\26"),
            ServerMod::BaseEnhanced
        );
        assert_eq!(
            mod_of("\\gamename\\Lugormod\\protocol\\26"),
            ServerMod::Lugormod
        );
        assert_eq!(
            mod_of("\\gamename\\basejka\\protocol\\26"),
            ServerMod::BaseJka
        );
        assert_eq!(
            mod_of("\\gamename\\JA+ Mod\\protocol\\25"),
            ServerMod::BaseJka
        );
        assert!(ServerMod::JaPro >= ServerMod::JaPlus);
    }
}
