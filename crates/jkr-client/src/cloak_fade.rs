//! How a cloaked player is drawn (`CG_Player`, `cg_players.c:10672-10781`): cloaking or
//! uncloaking takes two seconds, during which the cloak's shell grows or fades over the
//! model fading the other way; fully cloaked, only the shell is left, and not even that in
//! the player's own first-person view. A viewer with Force Sight sees through it all.

/// `cent->uncloaking - cg.time` at a change.
const TURN: i32 = 2_000;

/// `centity_t::cloaked` and `uncloaking`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloakFade {
    cloaked: bool,
    until: i32,
}

/// What a frame draws of a player as its cloak stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CloakDraw {
    /// No cloak: the model as ever.
    Plain,
    /// Cloaking, or uncloaking (`cloaking` false): the shell at `level` and the model
    /// faded to `1 - level`; uncloaking, the model is also drawn whole.
    Turning { level: f32, cloaking: bool },
    /// Cloaked: only the shell.
    Cloaked,
}

impl CloakFade {
    /// One frame at `now` of a player whose `PW_CLOAKED` is `cloaked`.
    pub fn advance(&mut self, cloaked: bool, now: i32) -> CloakDraw {
        if cloaked != self.cloaked {
            self.cloaked = cloaked;
            self.until = now.wrapping_add(TURN);
        }
        let left = self.until.wrapping_sub(now);
        if left > 0 {
            let level = left as f32 / TURN as f32;
            return CloakDraw::Turning {
                level: if cloaked { 1.0 - level } else { level },
                cloaking: cloaked,
            };
        }
        if cloaked {
            CloakDraw::Cloaked
        } else {
            CloakDraw::Plain
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloak_turns_in_and_out_over_two_seconds() {
        let mut fade = CloakFade::default();
        assert_eq!(fade.advance(false, 1_000), CloakDraw::Plain);
        assert_eq!(
            fade.advance(true, 2_000),
            CloakDraw::Turning {
                level: 0.0,
                cloaking: true
            }
        );
        assert_eq!(
            fade.advance(true, 3_000),
            CloakDraw::Turning {
                level: 0.5,
                cloaking: true
            }
        );
        assert_eq!(fade.advance(true, 4_000), CloakDraw::Cloaked);
        assert_eq!(
            fade.advance(false, 5_000),
            CloakDraw::Turning {
                level: 1.0,
                cloaking: false
            }
        );
        assert_eq!(fade.advance(false, 7_000), CloakDraw::Plain);
    }
}
