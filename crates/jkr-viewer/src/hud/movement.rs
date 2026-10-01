//! First-person JKA airborne CGAZ and snap guides; no simulation or wire changes.
use super::*;
use jkr_ui::{Color, DrawCommand, Rect};

#[path = "movement_math.rs"]
pub(crate) mod math;
#[path = "movement_settings.rs"]
mod settings;
pub(crate) use settings::register;

/// Retained guide input and settings, populated from the existing prediction/snapshot projection.
pub(super) struct Guides {
    settings: settings::Settings,
    velocity: [f32; 3],
    angles: [f32; 3],
    speed: f32,
    direction: usize,
    allowed: bool,
    airborne: bool,
    zones: [f32; 128],
    zone_key: [f32; 2],
    zone_count: usize,
    reported_helper: Option<i64>,
    projection: Option<([f32; 3], [f32; 3], f32)>,
    full_input: bool,
}

impl Default for Guides {
    fn default() -> Self {
        Self {
            settings: settings::Settings::read(None),
            velocity: [0.0; 3],
            angles: [0.0; 3],
            speed: 250.0,
            direction: 0,
            allowed: false,
            airborne: false,
            zones: [0.0; 128],
            zone_key: [0.0; 2],
            zone_count: 0,
            reported_helper: None,
            projection: None,
            full_input: true,
        }
    }
}

impl Guides {
    /// Refresh borrowed player data. Unsupported physics are suppressed, not approximated.
    pub(super) fn update(
        &mut self,
        player: &PlayerState,
        predicted: Option<&MovementState>,
        console: Option<&ViewerConsole>,
    ) {
        self.settings = settings::Settings::read(console);
        if self.reported_helper != Some(self.settings.helper) {
            self.reported_helper = Some(self.settings.helper);
            if self.settings.helper != 0 {
                eprintln!(
                    "strafe helper: first-person airborne JKA CGAZ front markers only; \
                    other styles, ground guidance, rear/max/centre lines and sounds unavailable"
                );
            }
        }
        self.velocity = predicted.map_or(player.velocity(), |p| p.velocity);
        self.angles = predicted.map_or(player.view_angles(), |p| p.view_angles);
        self.speed = predicted.map_or(player.speed(), |p| p.speed);
        self.direction = predicted
            .map_or(player.movement_direction(), |p| p.movement_direction)
            .clamp(0, 7) as usize;
        if predicted.is_none() && self.velocity[0].hypot(self.velocity[1]) < 9.0 {
            self.direction = 8;
        }
        self.airborne =
            predicted.map_or(player.ground_entity_num(), |p| p.ground_entity_number) == 1023;
        self.allowed = player.movement_type() == 0
            && player.vehicle_entity_num() == 0
            && player.zoom_mode() == 0;
    }

    /// Disable guides when the viewer is not using the supported first-person JKA camera/physics.
    pub(super) fn restrict(&mut self, supported: bool) {
        self.allowed &= supported;
    }

    /// Emit fixed-capacity rects; CGAZ markers and snap zones are informational, not backplates.
    pub(super) fn emit(&mut self, draw: &mut DrawList, viewport: [f32; 2]) {
        if !self.allowed {
            return;
        }
        let s = &self.settings;
        let fov = if let Some((_, _, fov)) = self.projection {
            fov
        } else if s.aspect_adjust {
            crate::scope::aspect_adjusted_fov(s.fov, viewport[0] / viewport[1])
        } else {
            s.fov
        };
        if s.helper & 4 != 0 && self.airborne {
            let speed = self.velocity[0].hypot(self.velocity[1]);
            let delta = math::optimal(speed, self.speed / s.helper_fps, self.speed) + s.offset;
            let velocity_yaw = self.velocity[1].atan2(self.velocity[0]).to_degrees();
            const BITS: [i64; 8] = [32, 64, 256, 65536, 32768, 131072, 512, 128];
            for (key, bit) in BITS.into_iter().enumerate() {
                if s.helper & bit == 0 {
                    continue;
                }
                let angle = velocity_yaw + math::direction(delta, key) - self.angles[1];
                let projected = if let Some((forward, left, _)) = self.projection {
                    math::project_basis(
                        velocity_yaw + math::direction(delta, key),
                        s.precision,
                        forward,
                        left,
                        fov,
                    )
                } else {
                    math::project(angle, -self.angles[0], s.precision, fov)
                };
                let Some(x) = projected else {
                    continue;
                };
                let half = if s.cutoff > 240.0 {
                    5.0
                } else {
                    20.0 - s.cutoff / 16.0
                };
                let color = if key == self.direction && self.full_input {
                    s.active
                } else {
                    let mut color = match key {
                        1 | 7 => Color::new(1.0, 1.0, 1.0, 1.0),
                        2 | 6 => Color::new(0.5, 1.0, 1.0, 1.0),
                        3 | 5 => Color::new(0.75, 0.0, 1.0, 1.0),
                        _ => Color::new(1.0, 0.75, 0.0, 1.0),
                    };
                    color.a = s.inactive;
                    color
                };
                rect(
                    draw,
                    viewport,
                    [x - s.width * 0.5, 240.0 - half, s.width, half * 2.0],
                    color,
                );
            }
        }
        if s.snap {
            let speed = if s.snap_speed == 0.0 {
                self.speed
            } else {
                s.snap_speed
            };
            if self.zone_key != [speed, s.snap_fps] {
                self.zone_key = [speed, s.snap_fps];
                self.zone_count = math::zones(speed, s.snap_fps, &mut self.zones);
            }
            let count = self.zone_count;
            let offset = match s.snap_auto {
                0 => s.snap_def,
                1 | 2 if self.direction == 8 => s.snap_def,
                1 | 2 if self.direction & 1 != 0 => 45.0,
                1 | 2 => 0.0,
                _ => return,
            };
            for i in 0..count {
                for add in [0.0, 90.0] {
                    let span = math::snap_span(
                        self.zones[i] + add,
                        self.zones[i + 1] + add,
                        self.angles[1] + offset,
                        fov,
                    );
                    rect(
                        draw,
                        viewport,
                        [span[0], s.snap_y, span[1], s.snap_height],
                        s.snap_colors[i & 1],
                    );
                }
            }
        }
    }
}

impl HudOverlay {
    /// Apply the camera/physics safety gate and real keyboard direction after projection.
    pub(crate) fn restrict_guides(&mut self, supported: bool, keys: Option<[i8; 2]>) {
        self.guides.restrict(supported);
        if let Some([f, r]) = keys {
            self.guides.full_input = [f, r].into_iter().all(|v| v == 0 || v == 127 || v == -127);
            self.guides.direction = match (f.signum(), r.signum()) {
                (1, 0) => 0,
                (1, -1) => 1,
                (0, -1) => 2,
                (-1, -1) => 3,
                (-1, 0) => 4,
                (-1, 1) => 5,
                (0, 1) => 6,
                (1, 1) => 7,
                _ => 8,
            };
        }
    }

    /// Use the rendered camera axes/FOV, including bob and roll, rather than a second camera.
    pub(crate) fn guide_camera(
        &mut self,
        forward: glam::Vec3,
        up: glam::Vec3,
        vertical_fov: f32,
        aspect: f32,
    ) {
        let forward = forward.normalize_or_zero();
        let left = up.cross(forward).normalize_or_zero();
        let fov = ((vertical_fov * 0.5).to_radians().tan() * aspect)
            .atan()
            .to_degrees()
            * 2.0;
        self.guides.angles[1] = forward.y.atan2(forward.x).to_degrees();
        self.guides.projection = Some((forward.to_array(), left.to_array(), fov));
    }
}

fn rect(draw: &mut DrawList, viewport: [f32; 2], r: [f32; 4], color: Color) {
    if !r.iter().all(|x| x.is_finite()) {
        return;
    }
    let x = r[0].max(0.0);
    let end = (r[0] + r[2]).min(640.0);
    let y = r[1].max(0.0);
    let bottom = (r[1] + r[3]).min(480.0);
    if x >= end || y >= bottom {
        return;
    }
    let _ = draw.push(DrawCommand::SolidRect {
        rect: Rect::new(
            x * viewport[0] / 640.0,
            y * viewport[1] / 480.0,
            (end - x) * viewport[0] / 640.0,
            (bottom - y) * viewport[1] / 480.0,
        ),
        color,
    });
}
