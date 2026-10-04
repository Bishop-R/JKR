//! HUD data bindings and allocation-free formatting helpers.

use super::*;

/// Borrowed values and visibility predicates for the retained HUD document.
pub(super) struct WidgetData<'a> {
    /// visibility binding supplied by the owning HUD.
    pub(super) visibility: HudVisibility,
    /// ratios binding supplied by the owning HUD.
    pub(super) ratios: [f32; 3],
    /// health binding supplied by the owning HUD.
    pub(super) health: &'a str,
    /// armor binding supplied by the owning HUD.
    pub(super) armor: &'a str,
    /// force binding supplied by the owning HUD.
    pub(super) force: &'a str,
    /// weapon binding supplied by the owning HUD.
    pub(super) weapon: &'a str,
    /// ammo binding supplied by the owning HUD.
    pub(super) ammo: &'a str,
    /// health value binding supplied by the owning HUD.
    pub(super) health_value: &'a str,
    /// armor value binding supplied by the owning HUD.
    pub(super) armor_value: &'a str,
    /// force value binding supplied by the owning HUD.
    pub(super) force_value: &'a str,
    /// weapon value binding supplied by the owning HUD.
    pub(super) weapon_value: &'a str,
    /// ammo value binding supplied by the owning HUD.
    pub(super) ammo_value: &'a str,
    /// Whether a saber style is shown under the Force meter.
    pub(super) style: bool,
    /// Current opacity of the weapon-name transient.
    pub(super) weapon_alpha: f32,
    /// team len binding supplied by the owning HUD.
    pub(super) team_len: usize,
    /// vote active binding supplied by the owning HUD.
    pub(super) vote_active: bool,
    /// team vote active binding supplied by the owning HUD.
    pub(super) team_vote_active: bool,
    /// kill len binding supplied by the owning HUD.
    pub(super) kill_len: usize,
    /// crosshair name binding supplied by the owning HUD.
    pub(super) crosshair_name: bool,
    /// timer binding supplied by the owning HUD.
    pub(super) timer: bool,
    /// warmup binding supplied by the owning HUD.
    pub(super) warmup: bool,
    /// interrupted binding supplied by the owning HUD.
    pub(super) interrupted: bool,
}

impl HudDataSource for WidgetData<'_> {
    fn number(&self, binding: &str) -> Option<f32> {
        match binding {
            "health_ratio" => Some(self.ratios[0]),
            "armor_ratio" => Some(self.ratios[1]),
            "force_ratio" => Some(self.ratios[2]),
            _ => None,
        }
    }

    fn text(&self, binding: &str) -> Option<&str> {
        match binding {
            "health_text" => Some(self.health),
            "armor_text" => Some(self.armor),
            "force_text" => Some(self.force),
            "weapon_text" => Some(self.weapon),
            "ammo_text" => Some(self.ammo),
            "health_value" => Some(self.health_value),
            "armor_value" => Some(self.armor_value),
            "force_value" => Some(self.force_value),
            "weapon_value" => Some(self.weapon_value),
            "ammo_value" => Some(self.ammo_value),
            _ => None,
        }
    }

    fn flag(&self, binding: &str) -> Option<bool> {
        match binding {
            // The ground HUD replaces the status block (and its scrim) and the stance.
            // So does the game-data menu HUD, which also shows the ammo.
            "draw_status" => Some(
                self.visibility.status && !self.visibility.ground_hud && !self.visibility.menu_hud,
            ),
            "draw_weapon" => {
                Some(self.visibility.weapon && !self.visibility.menu_hud && self.weapon_alpha > 0.0)
            }
            "draw_ammo" => Some(
                self.visibility.weapon && !self.visibility.menu_hud && !self.ammo_value.is_empty(),
            ),
            "draw_style" => Some(
                self.visibility.status
                    && !self.visibility.ground_hud
                    && !self.visibility.menu_hud
                    && self.style,
            ),
            "draw_crosshair" => Some(self.visibility.crosshair),
            "draw_team" => Some(self.visibility.team_overlay && self.team_len != 0),
            "draw_vote" => Some(self.visibility.hud && (self.vote_active || self.team_vote_active)),
            "draw_kill_feed" => Some(self.visibility.hud && self.kill_len != 0),
            "draw_crosshair_name" => Some(self.visibility.crosshair_names && self.crosshair_name),
            "draw_timer" => Some(self.visibility.timer && self.timer),
            "draw_warmup" => Some(self.visibility.hud && self.warmup),
            "draw_lagometer" => Some(self.visibility.lagometer),
            "draw_interrupted" => Some(self.visibility.hud && self.interrupted),
            _ => None,
        }
    }
}

pub(super) fn text_id(binding: Option<&str>) -> Option<TextId> {
    match binding {
        Some("health_text") => Some(TextId(0)),
        Some("armor_text") => Some(TextId(1)),
        Some("force_text") => Some(TextId(2)),
        Some("weapon_text") => Some(TextId(3)),
        Some("ammo_text") => Some(TextId(4)),
        Some("health_value") => Some(TextId(6)),
        Some("armor_value") => Some(TextId(8)),
        Some("force_value") => Some(TextId(10)),
        Some("weapon_value") => Some(TextId(12)),
        Some("ammo_value") => Some(TextId(14)),
        Some("style_value") => Some(TextId(15)),
        Some("crosshair_name") => Some(TextId(310)),
        Some("match_timer") => Some(TextId(311)),
        Some("warmup_text") => Some(TextId(312)),
        Some("connection_interrupted") => Some(TextId(313)),
        _ => None,
    }
}

pub(super) fn load_override(path: &Path) -> Option<HudLayoutDocument> {
    let source = std::fs::read_to_string(path).ok()?;
    let document = HudLayoutDocument::from_json(&source).ok()?;
    (document.version == 1 && document.widgets.len() <= WIDGET_LIMIT).then_some(document)
}

pub(super) fn localized_location<'a>(localization: &'a Localization, location: &'a str) -> &'a str {
    let Some(key) = location.strip_prefix('@') else {
        return location;
    };
    localization.strings.get(key).map_or(key, String::as_str)
}

pub(super) fn client_name_and_team(
    game_state: &GameState,
    client_num: u16,
) -> (std::borrow::Cow<'_, str>, u8) {
    game_state
        .config_string(1_131 + usize::from(client_num))
        .and_then(parse_client_info)
        .unwrap_or((std::borrow::Cow::Borrowed("unknown"), 3))
}

/// Split one `configstring` client-info entry into a display name and team.
///
/// JKA client info is a byte string, not UTF-8: names routinely carry high-bit bytes, both
/// from Latin-1 and from the decorated names players use. Validating the whole entry as UTF-8
/// and bailing turned every such player into "unknown" in the crosshair, while chat rendered
/// them correctly because `jkr-client/src/chat.rs` already converts lossily. Parsing the raw
/// bytes means a bad byte in any other field cannot cost us the name, and only a name that is
/// genuinely not UTF-8 allocates.
pub(super) fn parse_client_info(bytes: &[u8]) -> Option<(std::borrow::Cow<'_, str>, u8)> {
    let body = bytes.strip_prefix(b"\\").unwrap_or(bytes);
    let mut name: Option<&[u8]> = None;
    let mut team = 3;
    let mut fields = body.split(|byte| *byte == b'\\');
    while let (Some(key), Some(value)) = (fields.next(), fields.next()) {
        match key {
            b"n" | b"name" if name.is_none() && !value.is_empty() => name = Some(value),
            b"t" => {
                team = std::str::from_utf8(value)
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(3);
            }
            _ => {}
        }
    }
    // Latin-1, not UTF-8 with replacement marks: a name byte above 0x7F is the character of
    // that value in JKA's font, so lossy conversion rendered `Mara\xf1` as `Marai¿½`.
    Some((jkr_client::decode_legacy(name?), team))
}

pub(super) fn format_team_row(target: &mut String, name: &str, location: &str, entry: TeamInfo) {
    use std::fmt::Write as _;
    target.clear();
    let _ = write!(
        target,
        "^7{name:<18.18} ^3{location:<20.20} ^7{:>3} / {:>3}",
        entry.health, entry.armor
    );
}

pub(super) fn format_team_parts(
    name_target: &mut String,
    location_target: &mut String,
    stats_target: &mut String,
    name: &str,
    location: &str,
    entry: TeamInfo,
) {
    use std::fmt::Write as _;
    name_target.clear();
    location_target.clear();
    stats_target.clear();
    name_target.push_str(name);
    location_target.push_str(location);
    let _ = write!(stats_target, "{} / {}", entry.health, entry.armor);
}
