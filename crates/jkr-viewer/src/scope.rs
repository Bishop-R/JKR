//! Multiplayer zoom presentation (`cg_view.c:1140-1280`), independent of the wire codec,
//! with EternalJK's `+zoom` (`CG_ZoomDown_f`/`CG_ZoomUp_f`, `cg_consolecmds.c:934-948`):
//! while held, the view narrows to `cg_zoomFov` over 100 ms (and widens back as long
//! when let go) unless a weapon zooms, the third-person camera backs off, and the mouse
//! slows by `cg_zoomSensitivity` (or with the view). `cg_scopeSensitivity` scales a
//! weapon zoom's own slowing.
use crate::{GameAudio, GpuState};

#[path = "scope_gpu.rs"]
mod gpu;
pub(crate) use gpu::Mask;

#[derive(Clone, Copy)]
/// Render-clock zoom state; not weapon prediction or protocol storage.
pub(crate) struct Zoom {
    fov: f32,
    time: Option<i32>,
    sound_time: i32,
    /// `+zoom` held (`cg.zoomed`), the render time it last changed (`cg.zoomTime`), and a
    /// press or release not yet given a time.
    held: bool,
    held_time: Option<i32>,
    pending: Option<bool>,
    /// `cg_zoomFov`, whole degrees from 1 to 176.
    held_fov: f32,
    /// Stock zoomFov/cgFov multiplier, or one outside zoom.
    pub(crate) sensitivity: f32,
    /// Current zoom mode for suppression of the ordinary crosshair.
    pub(crate) mode: u8,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            fov: 80.0,
            time: None,
            sound_time: 0,
            held: false,
            held_time: None,
            pending: None,
            held_fov: 30.0,
            sensitivity: 1.0,
            mode: 0,
        }
    }
}

/// EternalJK's `+zoom` options.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ZoomOptions {
    /// `cg_zoomFov`: the field of view `+zoom` narrows to.
    pub(crate) fov: f32,
    /// `cg_zoomSensitivity`: the mouse's scale while `+zoom` is held; 0 or less follows
    /// the view (`fov_y / 75`).
    pub(crate) sensitivity: f32,
    /// `cg_scopeSensitivity`: multiplies a weapon zoom's scale; 0 or less leaves stock's.
    pub(crate) scope_sensitivity: f32,
}

impl Zoom {
    /// `+zoom` pressed or released; it takes effect at the next frame's time.
    pub(crate) fn hold(&mut self, held: bool) {
        self.pending = Some(held);
    }

    /// Back to no zoom, keeping whether `+zoom` is held: its key still is.
    fn reset(&mut self) {
        *self = Self {
            held: self.held,
            pending: self.pending,
            ..Self::default()
        };
    }

    /// The third-person camera's range while `+zoom` is held (`range² / cg_zoomFov`,
    /// `CG_CalcIdealThirdPersonViewLocation`), else `range`.
    pub(crate) fn third_person_range(&self, range: f32) -> f32 {
        if self.held {
            range * range / self.held_fov
        } else {
            range
        }
    }

    /// How far into its 100 ms change `+zoom` is, past 1 once done.
    fn held_fraction(&self, time: i32) -> f32 {
        self.held_time.map_or(f32::INFINITY, |since| {
            time.saturating_sub(since) as f32 / 100.0
        })
    }

    /// Return horizontal FOV, before any widescreen widening, and the stock 300 ms zoom
    /// sound pulse. `base` is `cg_fov`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn frame(
        &mut self,
        time: i32,
        base: f32,
        mode: u8,
        locked: bool,
        zoom_time: i32,
        saved_fov: f32,
        options: ZoomOptions,
    ) -> (f32, bool) {
        let elapsed = self.time.map_or(0, |last| time.saturating_sub(last).max(0));
        if self.time.is_some_and(|last| time < last) {
            self.reset();
        }
        self.time = Some(time);
        self.mode = mode;
        if let Some(held) = self.pending.take()
            && held != self.held
        {
            self.held = held;
            self.held_time = Some(time);
        }
        // EternalJK eases in towards up to 178 and settles at 176; both are 176 here.
        self.held_fov = options.fov.trunc().clamp(1.0, 176.0);
        let base = base.clamp(1.0, 130.0);
        let mut sound = false;
        let horizontal = if mode == 2 {
            if self.fov > 40.0 {
                self.fov -= elapsed as f32 * 0.075;
                if self.fov < 40.0 {
                    self.fov = 40.0;
                } else if self.fov > base {
                    self.fov = base;
                }
            }
            self.fov
        } else if mode != 0 {
            if !locked {
                self.fov = self.fov.min(50.0) - elapsed as f32 * 0.035;
                if self.fov < 3.0 {
                    self.fov = 3.0;
                } else if self.fov > base {
                    self.fov = base;
                } else if self.sound_time < time || self.sound_time > time.saturating_add(10000) {
                    sound = true;
                    self.sound_time = time.saturating_add(300);
                }
            }
            if self.fov < 3.0 {
                self.fov = 50.0;
            }
            self.fov
        } else if self.held {
            let fraction = self.held_fraction(time);
            if fraction > 1.0 {
                self.held_fov
            } else {
                base + fraction * (self.held_fov - base)
            }
        } else {
            self.fov = 80.0;
            // EternalJK widens back from `+zoom` in place of stock's widening back from a
            // weapon zoom; here each has its own, the `+zoom` one first.
            let released = self.held_fraction(time);
            let fraction = time.saturating_sub(zoom_time) as f32 / 100.0;
            if released <= 1.0 {
                self.held_fov + released * (base - self.held_fov)
            } else if fraction <= 1.0 {
                saved_fov + fraction * (base - saved_fov)
            } else {
                base
            }
        };
        self.sensitivity = match mode {
            0 => 1.0,
            _ if options.scope_sensitivity > 0.0 => self.fov / base * options.scope_sensitivity,
            _ => self.fov / base,
        };
        (horizontal, sound)
    }

    /// `+zoom`'s mouse scale, once the view's vertical field of view is known.
    fn settle_sensitivity(&mut self, vertical: f32, options: ZoomOptions) {
        if self.mode == 0 && self.held {
            self.sensitivity = if options.sensitivity <= 0.0 {
                vertical / 75.0
            } else {
                options.sensitivity
            };
        }
    }
}

/// Widescreen horizontal FOV per `cg_fovAspectAdjust` (`cgame/cg_view.c:1241-1249`,
/// LordHavoc's Darkplaces formula with a 3:4 base aspect).
pub(crate) fn aspect_adjusted_fov(horizontal_degrees: f32, aspect: f32) -> f32 {
    let half = (horizontal_degrees.to_radians() * 0.5).tan() * 0.75 * aspect;
    half.atan().to_degrees() * 2.0
}

impl GpuState {
    /// Calculate the legacy horizontal zoom before converting to the renderer's vertical FOV.
    pub(crate) fn scope_fov(&mut self, time: i32, audio: &mut Option<GameAudio>) -> f32 {
        let snapshot = self
            .live_session
            .as_ref()
            .map(|s| s.latest_snapshot())
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(|s| s.snapshot_at_or_before(time))
            });
        let Some(snapshot) = snapshot else {
            self.scope.reset();
            if let Some(mask) = &mut self.scope_mask {
                mask.prepare(&self.queue, false, 0, 80.0, time, 0.0, None);
            }
            return self.field_of_view;
        };
        let player = &snapshot.player;
        // Demo playback keeps the initial predictor, so only a live session may
        // read zoom mode from the predicted state (stock cg.predictedPlayerState).
        let predicted = self
            .live_session
            .as_ref()
            .and_then(|_| self.local_prediction.predicted_state());
        let mode = predicted.map_or(player.zoom_mode(), |state| state.zoom_mode);
        let float = |name: &str, fallback: f32| {
            self.console
                .as_ref()
                .and_then(|c| c.float_cvar(name))
                .map_or(fallback, |value| value as f32)
        };
        let base = float("cg_fov", 90.0);
        let options = ZoomOptions {
            fov: float("cg_zoomFov", 30.0),
            sensitivity: float("cg_zoomSensitivity", 0.0),
            scope_sensitivity: float("cg_scopeSensitivity", 1.0),
        };
        let (horizontal, sound) = self.scope.frame(
            time,
            base,
            mode,
            predicted.map_or(player.zoom_locked(), |state| state.zoom_locked),
            predicted.map_or(player.zoom_time(), |state| state.zoom_time),
            predicted.map_or(player.zoom_fov(), |state| state.zoom_fov),
            options,
        );
        if let Some(mask) = &mut self.scope_mask {
            let visible = mode != 0
                && mode != 2
                && !self.third_person
                && player.movement_type() != 7
                && self
                    .console
                    .as_ref()
                    .and_then(|c| c.bool_cvar("cg_draw2D"))
                    .unwrap_or(true);
            let maximum = if player.entity_flags() & (1 << 20) != 0 {
                600.0
            } else {
                300.0
            };
            let state = predicted.map_or(player.weapon_state(), |state| state.weapon_state);
            let charge_time =
                predicted.map_or(player.weapon_charge_time(), |s| s.weapon_charge_time);
            let charge = (state == 5).then(|| (time - charge_time) as f32 / 1500.0);
            mask.prepare(
                &self.queue,
                visible,
                crate::cgame_options::scope_style(self.console.as_ref()),
                self.scope.fov,
                time,
                player.ammo_value(3).unwrap_or(0) as f32 / maximum,
                charge,
            );
        }
        if sound
            && player.movement_type() != 7
            && let Some(audio) = audio
        {
            audio.play_local(
                "sound/weapons/disruptor/zoomloop.wav",
                1.0,
                jkr_audio::SourceId(1022),
                jkr_audio::ChannelId(1),
            );
        }
        let aspect = self.configuration.width as f32 / self.configuration.height as f32;
        let horizontal = if player.movement_type() == 7 {
            80.0
        } else {
            horizontal
        };
        // `cg_view.c:1241-1249`: widen the chosen horizontal FOV — zoomed or not — for
        // widescreen. Stock clamps the cvar first and lets the widened value exceed that.
        let horizontal = if self
            .console
            .as_ref()
            .and_then(|c| c.bool_cvar("cg_fovAspectAdjust"))
            .unwrap_or(true)
        {
            aspect_adjusted_fov(horizontal, aspect)
        } else {
            horizontal
        };
        self.field_of_view =
            (2.0 * ((horizontal.to_radians() * 0.5).tan() / aspect).atan()).to_degrees();
        self.scope.settle_sensitivity(self.field_of_view, options);
        self.field_of_view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPTIONS: ZoomOptions = ZoomOptions {
        fov: 30.0,
        sensitivity: 0.0,
        scope_sensitivity: 1.0,
    };

    fn view(zoom: &mut Zoom, time: i32) -> f32 {
        zoom.frame(time, 90.0, 0, false, 0, 0.0, OPTIONS).0
    }

    #[test]
    fn plus_zoom_eases_in_and_out_over_a_tenth_of_a_second() {
        let mut zoom = Zoom::default();
        assert_eq!(view(&mut zoom, 1000), 90.0);
        zoom.hold(true);
        assert_eq!(view(&mut zoom, 2000), 90.0);
        assert_eq!(view(&mut zoom, 2050), 60.0);
        assert_eq!(view(&mut zoom, 2200), 30.0);
        assert_eq!(zoom.third_person_range(60.0), 120.0);
        zoom.settle_sensitivity(17.0, OPTIONS);
        assert_eq!(zoom.sensitivity, 17.0 / 75.0);
        zoom.hold(false);
        assert_eq!(view(&mut zoom, 3000), 30.0);
        assert_eq!(view(&mut zoom, 3050), 60.0);
        assert_eq!(view(&mut zoom, 3200), 90.0);
        assert_eq!(zoom.third_person_range(60.0), 60.0);
        assert_eq!(zoom.sensitivity, 1.0);
    }
}
