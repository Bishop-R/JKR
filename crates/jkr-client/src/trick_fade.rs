//! How a player who mind-tricks the viewer fades away, and back once the trick ends
//! (`CG_Player`, `cg_players.c:8837-8896` and `9657-9672`): the model loses half an alpha
//! step per millisecond until it is gone, then nothing of it is drawn; it regains a whole
//! step per millisecond afterwards.

/// How much of a player's model is drawn this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BodyDraw {
    /// As its shaders say.
    #[default]
    Opaque,
    /// Blended at this alpha whatever its shaders say (`RF_FORCE_ENT_ALPHA`).
    Faded(u8),
    /// Not at all.
    Hidden,
}

impl BodyDraw {
    /// A see-through `alpha` as `CG_CheckThirdPersonAlpha` forces it on the viewer's own
    /// player (`cg_thirdPersonAlpha`): only below 1, cut to a byte.
    pub fn see_through(alpha: f32) -> Self {
        if alpha < 1.0 {
            Self::Faded((alpha.max(0.0) * 255.0) as u8)
        } else {
            Self::Opaque
        }
    }
}

/// `centity_t::trickAlpha` and `trickAlphaTime`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrickFade {
    alpha: i32,
    time: i32,
}

impl TrickFade {
    /// One frame at `now`: `tricked` says whether the player mind-tricks the viewer
    /// (`CG_IsMindTricked`). A fade left alone for over a second, or never begun, starts
    /// again from fully drawn ("things got out of sync").
    pub fn advance(&mut self, tricked: bool, now: i32) -> BodyDraw {
        if self.time == 0 || now.wrapping_sub(self.time) > 1_000 {
            *self = Self {
                alpha: 255,
                time: now,
            };
        }
        let elapsed = now.wrapping_sub(self.time);
        if tricked {
            if self.alpha > 1 {
                // `trickAlpha -= (cg.time - trickAlphaTime)*0.5`: a double, cut back to int.
                self.alpha = (f64::from(self.alpha) - f64::from(elapsed) * 0.5) as i32;
                self.time = now;
                self.alpha = self.alpha.max(0);
                return self.faded();
            }
            self.alpha = 1;
            self.time = now;
            return BodyDraw::Hidden;
        }
        if self.alpha < 255 {
            self.alpha = self.alpha.saturating_add(elapsed).min(255);
            self.time = now;
            return self.faded();
        }
        self.alpha = 255;
        self.time = now;
        BodyDraw::Opaque
    }

    /// `legs.shaderRGBA[3] = trickAlpha`, never below 1 ("don't cancel it out").
    fn faded(self) -> BodyDraw {
        BodyDraw::Faded(self.alpha.clamp(1, 255) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trick_fades_the_player_out_at_half_a_step_a_millisecond_then_hides_it() {
        let mut fade = TrickFade::default();
        assert_eq!(fade.advance(false, 1_000), BodyDraw::Opaque);
        assert_eq!(fade.advance(true, 1_016), BodyDraw::Faded(247));
        // An odd frame leaves half a step, which the int drops.
        assert_eq!(fade.advance(true, 1_023), BodyDraw::Faded(243));
        // Gone below nothing, it is still drawn at the least alpha for that frame.
        assert_eq!(fade.advance(true, 1_510), BodyDraw::Faded(1));
        assert_eq!(fade.advance(true, 1_526), BodyDraw::Hidden);
        assert_eq!(fade.advance(true, 1_542), BodyDraw::Hidden);
    }

    #[test]
    fn the_player_comes_back_a_whole_step_a_millisecond_once_the_trick_ends() {
        let mut fade = TrickFade::default();
        fade.advance(true, 1_000);
        for now in (1_016..1_600).step_by(16) {
            fade.advance(true, now);
        }
        assert_eq!(fade.advance(false, 1_616), BodyDraw::Faded(25));
        assert_eq!(fade.advance(false, 1_816), BodyDraw::Faded(225));
        assert_eq!(fade.advance(false, 1_866), BodyDraw::Faded(255));
        assert_eq!(fade.advance(false, 1_882), BodyDraw::Opaque);
    }

    #[test]
    fn only_an_alpha_below_one_sees_through_the_player() {
        assert_eq!(BodyDraw::see_through(1.0), BodyDraw::Opaque);
        assert_eq!(BodyDraw::see_through(0.5), BodyDraw::Faded(127));
        assert_eq!(BodyDraw::see_through(-1.0), BodyDraw::Faded(0));
    }

    #[test]
    fn a_fade_left_alone_for_a_second_starts_again_fully_drawn() {
        let mut fade = TrickFade::default();
        fade.advance(true, 1_000);
        fade.advance(true, 1_400);
        assert_eq!(fade.advance(true, 2_401), BodyDraw::Faded(255));
    }
}
