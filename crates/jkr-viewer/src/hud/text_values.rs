//! Retained HUD text resolution, independent of layout.
use super::*;

impl HudOverlay {
    /// Resolve a widget text ID without formatting or allocating.
    pub(crate) fn resolve_text(&self, id: TextId) -> &str {
        match id.0 {
            0 => &self.health,
            1 => &self.armor,
            2 => &self.force,
            3 => &self.weapon,
            4 => &self.ammo,
            6 => &self.health_value,
            8 => &self.armor_value,
            10 => &self.force_value,
            12 => &self.weapon_value,
            14 => &self.ammo_value,
            15 => &self.style_value,
            100 => "TEAM STATUS",
            value @ 101..=108 => &self.team_names[(value - 101) as usize],
            value @ 111..=118 => &self.team_locations[(value - 111) as usize],
            value @ 121..=128 => &self.team_stats[(value - 121) as usize],
            value @ 131..=138 => &self.team_gear[(value - 131) as usize],
            200 => &self.vote_heading,
            201 => &self.vote_text,
            202 => &self.vote_keys,
            203 => &self.team_vote_heading,
            204 => &self.team_vote_text,
            value @ 300..=307 => &self.kill_rows[(value - 300) as usize],
            310 => &self.crosshair_name,
            311 => &self.match_timer,
            312 => &self.warmup_text,
            313 => "CONNECTION INTERRUPTED",
            314 => &self.speed.text,
            315 => &self.score_text,
            316 => &self.snapshot_text,
            317 => &self.targeting.position,
            318 => &self.enemy_info.name,
            319 => &self.enemy_info.detail,
            value @ 400..=415 => self.icons.text((value - 400) as usize),
            _ => selection::text(id),
        }
    }
}
