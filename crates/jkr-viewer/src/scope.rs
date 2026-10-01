//! Multiplayer zoom presentation (`cg_view.c:1140-1280`), independent of the wire codec.
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
            sensitivity: 1.0,
            mode: 0,
        }
    }
}

impl Zoom {
    /// Return horizontal FOV and the stock 300 ms zoom sound pulse.
    pub(crate) fn frame(
        &mut self,
        time: i32,
        base: f32,
        mode: u8,
        locked: bool,
        zoom_time: i32,
        saved_fov: f32,
    ) -> (f32, bool) {
        let elapsed = self.time.map_or(0, |last| time.saturating_sub(last).max(0));
        if self.time.is_some_and(|last| time < last) {
            *self = Self::default();
        }
        self.time = Some(time);
        self.mode = mode;
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
        } else {
            self.fov = 80.0;
            let fraction = time.saturating_sub(zoom_time) as f32 / 100.0;
            if fraction <= 1.0 {
                saved_fov + fraction * (base - saved_fov)
            } else {
                base
            }
        };
        self.sensitivity = if mode != 0 { self.fov / base } else { 1.0 };
        (horizontal, sound)
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
            self.scope = Zoom::default();
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
        let base = self
            .console
            .as_ref()
            .and_then(|c| c.float_cvar("cg_fov"))
            .unwrap_or(90.0) as f32;
        // `cg_view.c:1241-1249`: widen the horizontal FOV for widescreen. Stock
        // clamps the cvar first and lets the adjusted value exceed that clamp.
        let base = if self
            .console
            .as_ref()
            .and_then(|c| c.bool_cvar("cg_fovAspectAdjust"))
            .unwrap_or(true)
        {
            let aspect = self.configuration.width as f32 / self.configuration.height as f32;
            aspect_adjusted_fov(base.clamp(1.0, 130.0), aspect)
        } else {
            base
        };
        let (horizontal, sound) = self.scope.frame(
            time,
            base,
            mode,
            predicted.map_or(player.zoom_locked(), |state| state.zoom_locked),
            predicted.map_or(player.zoom_time(), |state| state.zoom_time),
            predicted.map_or(player.zoom_fov(), |state| state.zoom_fov),
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
        self.field_of_view =
            (2.0 * ((horizontal.to_radians() * 0.5).tan() / aspect).atan()).to_degrees();
        self.field_of_view
    }
}
