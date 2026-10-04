//! Projection from persisted shell cvars into the multiplayer profile.

use super::{ViewerConsole, invalid_cvar};
use jkr_client::PlayerProfile;
use jkr_network::LegacyUserInfo;
use std::error::Error;

impl ViewerConsole {
    /// Return the legacy connection settings backed by typed cvars.
    pub(crate) fn userinfo(&self) -> Result<LegacyUserInfo, Box<dyn Error>> {
        let profile = self.player_profile()?;
        let snaps = u16::try_from(self.positive_u32_cvar("snaps")?)
            .map_err(|_| invalid_cvar("snaps", "must fit in an unsigned 16-bit integer"))?;
        let password = self
            .text_cvar("password")
            .ok()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let mut userinfo = profile.legacy_userinfo(
            self.positive_u32_cvar("rate")?,
            snaps,
            self.bool_cvar("cg_predictItems").unwrap_or(true),
            password,
        );
        userinfo.plugin_disable = self
            .integer_cvar("cp_pluginDisable")
            .and_then(|bits| u32::try_from(bits).ok());
        self.force_profile.apply_to_userinfo(&mut userinfo);
        Ok(userinfo)
    }

    fn player_profile(&self) -> Result<PlayerProfile, Box<dyn Error>> {
        PlayerProfile::from_cvar_values(
            self.text_cvar("name")?,
            self.text_cvar("model")?,
            self.text_cvar("sex")?,
            self.text_cvar("saber1")?,
            self.text_cvar("saber2")?,
            self.integer_cvar("color1").unwrap_or(4),
            self.integer_cvar("color2").unwrap_or(4),
            [
                self.integer_cvar("cp_sbRGB1").unwrap_or(0),
                self.integer_cvar("cp_sbRGB2").unwrap_or(0),
            ],
            [
                self.integer_cvar("char_color_red").unwrap_or(255),
                self.integer_cvar("char_color_green").unwrap_or(255),
                self.integer_cvar("char_color_blue").unwrap_or(255),
            ],
            self.text_cvar("forcepowers")?,
            self.integer_cvar("handicap").unwrap_or(100),
        )
        .map_err(Into::into)
    }
}
