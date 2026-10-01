//! CG_PlayerShieldHit / CG_DrawPlayerShield, codemp/cgame/cg_players.c:5114-5198.

/// Fixed per-entity damage shell timer and outward-facing direction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LegacyShieldHit {
    /// Presentation-time expiry of the longest overlapping hit.
    pub until: i32,
    /// Negated ByteToDir normal, retained only when the timer is extended.
    pub direction: [f32; 3],
}

impl LegacyShieldHit {
    /// Apply the stock amount rule; a weaker overlapping hit cannot turn the shell.
    pub fn hit(&mut self, now: i32, amount: i32, direction: [f32; 3]) {
        let duration = if amount > 100 {
            2000
        } else {
            500 + amount.saturating_mul(15)
        };
        let until = now.saturating_add(duration);
        if until > self.until {
            self.until = until;
            self.direction = direction.map(|v| -v);
        }
    }

    /// Stock RGB brightness and scale; alpha remains opaque in the shader input.
    pub fn sample(self, now: i32, random_unit: f32) -> Option<(u8, f32)> {
        if self.until <= now {
            return None;
        }
        let brightness = (255.0 * (self.until - now) as f32 / 2000.0
            + random_unit.clamp(0.0, 1.0) * 16.0)
            .clamp(0.0, 255.0) as u8;
        Some((brightness, 1.4 - f32::from(brightness) * (0.4 / 255.0)))
    }
}
