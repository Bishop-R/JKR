//! Snapshot and prediction projection into retained HUD strings.

use super::*;

impl HudOverlay {
    pub(crate) fn update(
        &mut self,
        session: &ClientSession,
        localization: &Localization,
        time_ms: u64,
        predicted: Option<&MovementState>,
        console: Option<&ViewerConsole>,
    ) -> ClientHudData {
        self.update_team_overlay(session, localization);
        let player = &session.latest_snapshot().player;
        self.guides.update(player, predicted, console);
        self.speed
            .update(predicted.map_or(player.velocity(), |p| p.velocity), console);
        self.update_votes(session.game_state(), player.team(), time_ms as i32, console);
        let values = legacy_predicted_hud_data(player, predicted);
        self.update_values(player, values, time_ms)
    }

    pub(crate) fn update_player(&mut self, player: &PlayerState, time_ms: u64) -> ClientHudData {
        let values = legacy_hud_data(player);
        self.update_values(player, values, time_ms)
    }

    pub(crate) fn update_demo_votes(
        &mut self,
        game_state: &GameState,
        player: &PlayerState,
        time_ms: u64,
        console: Option<&ViewerConsole>,
    ) {
        self.guides.update(player, None, console);
        self.update_votes(game_state, player.team(), time_ms as i32, console);
        self.speed.update(player.velocity(), console);
    }

    fn update_values(
        &mut self,
        player: &PlayerState,
        values: ClientHudData,
        time_ms: u64,
    ) -> ClientHudData {
        let maximum = player.max_health().max(1) as f32;
        self.icons.weapon(values.weapon);
        let targets = [
            (values.health as f32 / maximum).clamp(0.0, 1.0),
            (values.armor as f32 / maximum).clamp(0.0, 1.0),
            f32::from(values.force) / 100.0,
        ];
        for (tween, target) in self.ratios.iter_mut().zip(targets) {
            if (tween.target() - target).abs() > f32::EPSILON {
                tween.retarget(
                    target,
                    time_ms,
                    self.theme.motion.normal,
                    Easing::SmoothStep,
                );
            }
        }
        if self.values == Some(values) {
            return values;
        }
        if self
            .values
            .is_none_or(|previous| previous.weapon != values.weapon)
        {
            self.weapon_shown_ms = Some(time_ms);
        }
        self.format_values(values);
        values
    }

    /// Rewrite every retained value string from `values`.
    pub(super) fn format_values(&mut self, values: ClientHudData) {
        use std::fmt::Write as _;
        self.health.clear();
        self.armor.clear();
        self.force.clear();
        self.weapon.clear();
        self.ammo.clear();
        self.health_value.clear();
        self.armor_value.clear();
        self.force_value.clear();
        self.weapon_value.clear();
        self.ammo_value.clear();
        self.style_value.clear();
        let _ = write!(self.health, "^1HEALTH ^7{}", values.health);
        let _ = write!(self.armor, "^5ARMOR ^7{}", values.armor);
        let _ = write!(self.force, "^5FORCE ^7{}", values.force);
        let _ = write!(self.health_value, "{}", values.health);
        let _ = write!(self.armor_value, "{}", values.armor);
        let _ = write!(self.force_value, "{}", values.force);
        self.weapon_value
            .push_str(crate::ingame_menu::weapon_name(values.weapon));
        let _ = write!(
            self.weapon,
            "^3WEAPON ^7{}",
            crate::ingame_menu::weapon_name(values.weapon)
        );
        match (values.saber_style, values.ammo) {
            (Some(style), _) => {
                let style = jkr_client::legacy_saber_style_name(style);
                let _ = write!(self.ammo, "^5STYLE ^7{style}");
                self.style_value.push_str(style);
            }
            (None, Some(ammo)) => {
                let _ = write!(self.ammo, "^3AMMO ^7{ammo}");
                let _ = write!(self.ammo_value, "{ammo}");
            }
            (None, None) => {
                self.ammo.push_str("^3AMMO ^7--");
                self.ammo_value.push_str("--");
            }
        }
        self.values = Some(values);
    }

    fn update_team_overlay(&mut self, session: &ClientSession, localization: &Localization) {
        let revision = session.team_info().revision();
        let team_side = session.latest_snapshot().player.team();
        self.team_revision = revision;
        self.team_side = team_side;
        self.team_len = 0;
        if !matches!(team_side, 1 | 2) {
            return;
        }
        for entry in session.team_info().entries().iter().take(8) {
            let (name, side) =
                client_name_and_team(session.game_state(), u16::from(entry.client_num));
            if side != team_side {
                continue;
            }
            let location = localized_location(
                localization,
                legacy_team_location(session.game_state(), entry.location),
            );
            format_team_row(&mut self.team_rows[self.team_len], &name, location, *entry);
            format_team_parts(
                &mut self.team_names[self.team_len],
                &mut self.team_locations[self.team_len],
                &mut self.team_stats[self.team_len],
                &name,
                location,
                *entry,
            );
            family::gear(
                &mut self.team_gear[self.team_len],
                *entry,
                self.family.team_weapons,
            );
            self.team_len += 1;
        }
    }
}
