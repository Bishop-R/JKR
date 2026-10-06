//! Immediate map target actions from multiplayer `g_target.c` and `g_misc.c`.

use crate::event_entity::EventEntity;
use jkr_entity::Entity;

/// A target's authored action; the dedicated host applies it to the real world.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// `Use_Target_Score`, including negative scores.
    Score(i32),
    /// `Use_target_remove_powerups`: return a carried flag, then clear all powerups.
    RemovePowerups,
    /// `Use_Target_Screenshake`, global or limited to its potential visibility set.
    Shake {
        intensity: f32,
        duration: i32,
        global: bool,
    },
}

impl Action {
    /// Recognize and parse an immediate-action classname, or leave it to another owner.
    pub fn spawn(entity: &Entity) -> Option<Self> {
        let int = |key, default| {
            entity
                .get(key)
                .map_or(default, |v| crate::userinfo::atoi(v.as_bytes()))
        };
        match entity.classname()?.to_ascii_lowercase().as_str() {
            "target_score" => Some(Self::Score(match int("count", 0) {
                0 => 1,
                n => n,
            })),
            "target_remove_powerups" => Some(Self::RemovePowerups),
            "target_screenshake" => Some(Self::Shake {
                intensity: entity
                    .get("intensity")
                    .map_or(10.0, |v| crate::text_parse::atof(v.as_bytes())),
                duration: int("duration", 800),
                global: int("globalshake", 1) != 0,
            }),
            _ => None,
        }
    }

    /// `G_ScreenShake`: snapped trajectory position plus the original precise origin.
    pub fn shake_event(&self, origin: [f32; 3]) -> Option<EventEntity> {
        let Self::Shake {
            intensity,
            duration,
            global,
        } = *self
        else {
            return None;
        };
        let mut extra = [(0, 0); 12];
        for (axis, field) in [11, 12, 13].into_iter().enumerate() {
            extra[axis] = (field, origin[axis].to_bits());
        }
        extra[3] = (25, intensity.to_bits());
        extra[4] = (65, duration as u32);
        Some(EventEntity {
            event: 42,
            parameter: 0,
            origin,
            client: None,
            broadcast: global,
            extra,
        })
    }
}
