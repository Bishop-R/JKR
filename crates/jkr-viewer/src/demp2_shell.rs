//! The DEMP2's alternate fire bursting (`FX_DEMP2_AltDetonate`, `fx_demp2.c:262-281`, drawn
//! by `CG_AddFadeScaleModel`, `cg_localents.c`): `models/items/sphere.md3` under
//! `gfx/effects/demp2shell`, growing for 800 ms as the cube of its age to the event's
//! `weapon` (the charge doubled) times the model across and half that up, its colour fading
//! as it grows, shaded as `RF_VOLUMETRIC`.

use glam::Vec3;
use std::time::{Duration, Instant};

/// `cgs.media.demp2Shell` and `demp2ShellShader` (`cg_weaponinit.c:425-426`).
pub(crate) const MODEL: &str = "models/items/sphere.md3";
pub(crate) const SHADER: &str = "gfx/effects/demp2shell";
/// `endTime - startTime`.
const LIFE: Duration = Duration::from_millis(800);
/// Shells at once; a seventeenth waits for one to end.
const MAX_SHELLS: usize = 16;

#[derive(Clone, Copy, Debug)]
struct Shell {
    origin: Vec3,
    size: f32,
    start: Instant,
}

/// One shell as drawn: its centre, the model's scale along each axis, and the colour's
/// level (`shaderRGBA` over 255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shape {
    pub(crate) origin: Vec3,
    pub(crate) scale: [f32; 3],
    pub(crate) level: f32,
}

/// The shells bursting now.
pub(crate) struct Shells {
    live: Vec<Shell>,
}

impl Default for Shells {
    fn default() -> Self {
        Self {
            live: Vec::with_capacity(MAX_SHELLS),
        }
    }
}

impl Shells {
    /// `EFFECT_EXPLOSION_DEMP2ALT` at `origin` with the event's `weapon`.
    pub(crate) fn start(&mut self, origin: Vec3, weapon: u8, now: Instant) {
        if self.live.len() < MAX_SHELLS {
            self.live.push(Shell {
                origin,
                size: f32::from(weapon),
                start: now,
            });
        }
    }

    /// The shells' shapes at `now`; those that have ended are forgotten (`CG_AddLocalEntities`
    /// frees one once `cg.time` reaches its end).
    pub(crate) fn frame(&mut self, now: Instant) -> impl Iterator<Item = Shape> + '_ {
        self.live
            .retain(|shell| now.saturating_duration_since(shell.start) < LIFE);
        self.live.iter().map(move |shell| {
            let age = now.saturating_duration_since(shell.start).as_secs_f32() / LIFE.as_secs_f32();
            // "yes, this is completely ridiculous...but it causes the shell to grow slowly
            // then explode at the end"
            let grown = age * age * age;
            Shape {
                origin: shell.origin,
                scale: [
                    shell.size * grown,
                    shell.size * grown,
                    shell.size * 0.5 * grown,
                ],
                level: 1.0 - grown,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shell_grows_as_the_cube_of_its_age_and_fades() {
        let start = Instant::now();
        let mut shells = Shells::default();
        shells.start(Vec3::ONE, 4, start);
        let half = shells
            .frame(start + Duration::from_millis(400))
            .next()
            .unwrap();
        assert_eq!(half.scale, [0.5, 0.5, 0.25]);
        assert_eq!(half.level, 0.875);
        assert!(shells.frame(start + LIFE).next().is_none());
        assert!(shells.live.is_empty());
    }
}
