//! Runtime projection feeding match information into the retained HUD.

use super::*;
use bytemuck::{Pod, Zeroable};

/// Append HUD text and overhead labels without duplicating the classic-font fallback.
pub(crate) fn append(
    gpu: &mut GpuState,
    classic: bool,
    _visibility: hud::HudVisibility,
    _layout: hud::HudLayout,
    _scale: f32,
    viewport: [f32; 2],
) {
    gpu.hud
        .identification
        .append(&gpu.chat, &mut gpu.text_vertices, &gpu.ui_font, viewport);
    if classic && let Some(font) = &gpu.classic_hud_font {
        gpu.hud.append(
            &mut gpu.game_fonts,
            &mut gpu.classic_text_vertices,
            font,
            viewport,
        );
    } else {
        gpu.hud.append(
            &mut gpu.game_fonts,
            &mut gpu.text_vertices,
            &gpu.ui_font,
            viewport,
        );
    }
}

/// GPU layout shared with `hud.wgsl`; appended parameters begin at byte 144.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct HudUniform {
    /// Resolved crosshair RGBA.
    pub(crate) crosshair_color: [f32; 4],
    /// Displayed health fraction.
    pub(crate) health_ratio: f32,
    /// Displayed armor fraction.
    pub(crate) armor_ratio: f32,
    /// Displayed Force fraction.
    pub(crate) force_ratio: f32,
    /// Menu backdrop selector.
    pub(crate) menu_open: f32,
    /// Reciprocal physical viewport width.
    pub(crate) inverse_width: f32,
    /// Reciprocal physical viewport height.
    pub(crate) inverse_height: f32,
    /// Selected menu row.
    pub(crate) menu_row: f32,
    /// Number of menu rows.
    pub(crate) menu_row_count: f32,
    /// Crosshair size; zero hides it.
    pub(crate) crosshair: f32,
    /// Global HUD visibility.
    pub(crate) hud_visible: f32,
    /// Legacy status-bar switch.
    pub(crate) status_visible: f32,
    /// Menu presentation variant.
    pub(crate) menu_kind: f32,
    /// Seconds since the UI epoch.
    pub(crate) menu_phase: f32,
    /// Damage direction, horizontal component.
    pub(crate) damage_x: f32,
    /// Damage direction, vertical component.
    pub(crate) damage_y: f32,
    /// Damage indicator opacity.
    pub(crate) damage_alpha: f32,
    /// Damage indicator intensity.
    pub(crate) damage_strength: f32,
    /// Retained health-bar rectangle.
    pub(crate) health_bar: [f32; 4],
    /// Retained armor-bar rectangle.
    pub(crate) armor_bar: [f32; 4],
    /// Retained Force-bar rectangle.
    pub(crate) force_bar: [f32; 4],
    /// Existing uniform tail padding.
    pub(crate) _padding: [f32; 3],
    /// Offset UV and dimensions used to size the crosshair.
    pub(crate) crosshair_parameters: [f32; 4],
}

/// Refresh retained HUD data and project movement guides through this frame's camera.
pub(crate) fn update(
    gpu: &mut GpuState,
    view_position: Vec3,
    view: (Vec3, Vec3, f32),
    presentation_time: i32,
    intermission: bool,
) {
    let (view_target, view_up, vertical_fov) = view;
    hud::tints::update(gpu, view_position, presentation_time, intermission);
    let scoreboard = gpu.gameplay_input.held(input::GameButton::Scores);
    let labels_hidden = scoreboard
        || intermission
        || gpu.console.as_ref().is_some_and(|c| {
            !c.bool_cvar("cg_drawhud").unwrap_or(true) || !c.bool_cvar("cg_draw2d").unwrap_or(true)
        });
    gpu.hud.identification.sample(gpu.console.as_ref());
    let camera = hud::identification::Camera {
        eye: view_position,
        target: view_target,
        up: view_up,
        fov: vertical_fov,
        viewport: [
            gpu.configuration.width as f32,
            gpu.configuration.height as f32,
        ],
    };
    if let Some(session) = &gpu.live_session {
        let snapshot = session.latest_snapshot();
        gpu.scoreboard.observe_deaths(
            &gpu.obituaries,
            snapshot,
            session.game_state(),
            gpu.console.as_ref(),
        );
        gpu.hud.identification.update(
            snapshot,
            session.game_state(),
            &gpu.live_world,
            i64::from(presentation_time),
            camera,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            labels_hidden,
        );
        gpu.hud
            .update_family(snapshot, session.game_state(), gpu.console.as_ref());
        gpu.hud.update_selection(
            &gpu.gameplay_input.selection,
            &snapshot.player,
            presentation_time,
        );
        gpu.lagometer
            .add_frame(presentation_time - snapshot.server_time);
        let mut ray = crosshair_scan::dynamic::ray(
            gpu.console.as_ref(),
            &snapshot.player,
            gpu.local_prediction.predicted_state(),
            session.game_state(),
            camera,
            gpu.third_person,
            snapshot.player.weapon(),
        );
        if ray.projected {
            ray.distance = gpu.crosshair_scan.distance_cull.unwrap_or(6000.0);
        }
        let crosshair = gpu.crosshair_scan.scan(
            snapshot,
            session.game_state(),
            ray,
            presentation_time,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            crosshair_scan::Suppression {
                scoreboard,
                intermission,
            },
        );
        gpu.hud.targeting.dynamic_offset = if ray.projected {
            Some(
                crosshair_scan::dynamic::offset(camera, gpu.crosshair_scan.endpoint)
                    .unwrap_or([2.0; 2]),
            )
        } else {
            None
        };
        gpu.hud.update(
            session,
            &gpu.localization,
            presentation_time.max(0) as u64,
            gpu.local_prediction.predicted_state(),
            gpu.console.as_ref(),
        );
        gpu.hud.update_match_information(
            session.game_state(),
            presentation_time,
            gpu.obituaries.feed(),
            &gpu.lagometer,
            crosshair,
            !session.is_local()
                && jkr_client::connection_interrupted(presentation_time, snapshot.server_time),
            &gpu.localization,
        );
    } else if let Some(session) = &gpu.demo_session {
        let snapshot = session.snapshot_at_or_before(presentation_time);
        gpu.scoreboard.observe_deaths(
            &gpu.obituaries,
            snapshot,
            session.game_state(),
            gpu.console.as_ref(),
        );
        gpu.hud.identification.update(
            snapshot,
            session.game_state(),
            session.world(),
            i64::from(presentation_time),
            camera,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            labels_hidden,
        );
        gpu.hud
            .update_family(snapshot, session.game_state(), gpu.console.as_ref());
        gpu.hud.update_selection(
            &gpu.gameplay_input.selection,
            &snapshot.player,
            presentation_time,
        );
        gpu.lagometer
            .add_frame(presentation_time - snapshot.server_time);
        let mut ray = crosshair_scan::dynamic::ray(
            gpu.console.as_ref(),
            &snapshot.player,
            None,
            session.game_state(),
            camera,
            gpu.third_person,
            snapshot.player.weapon(),
        );
        if ray.projected {
            ray.distance = gpu.crosshair_scan.distance_cull.unwrap_or(6000.0);
        }
        let crosshair = gpu.crosshair_scan.scan(
            snapshot,
            session.game_state(),
            ray,
            presentation_time,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            crosshair_scan::Suppression {
                scoreboard,
                intermission,
            },
        );
        gpu.hud.targeting.dynamic_offset = if ray.projected {
            Some(
                crosshair_scan::dynamic::offset(camera, gpu.crosshair_scan.endpoint)
                    .unwrap_or([2.0; 2]),
            )
        } else {
            None
        };
        gpu.hud
            .update_player(&snapshot.player, presentation_time.max(0) as u64);
        gpu.hud.update_demo_votes(
            session.game_state(),
            &snapshot.player,
            presentation_time.max(0) as u64,
            gpu.console.as_ref(),
        );
        gpu.hud.update_match_information(
            session.game_state(),
            presentation_time,
            gpu.obituaries.feed(),
            &gpu.lagometer,
            crosshair,
            false,
            &gpu.localization,
        );
    }
    let game = gpu
        .live_session
        .as_ref()
        .map(|s| s.game_state())
        .or_else(|| gpu.demo_session.as_ref().map(|s| s.game_state()));
    {
        if let Some(game) = gpu
            .resident
            .session
            .as_ref()
            .map(|s| s.game_state())
            .or(game)
        {
            gpu.chat.update_roster(game);
        } else {
            gpu.hud.identification.list.clear();
        }
    }
    if let Some(game) = game {
        let snapshot = gpu
            .live_session
            .as_ref()
            .map(|s| s.latest_snapshot())
            .or_else(|| {
                gpu.demo_session
                    .as_ref()
                    .map(|s| s.snapshot_at_or_before(presentation_time))
            });
        if let Some(snapshot) = snapshot {
            gpu.hud
                .targeting
                .classify(gpu.crosshair_scan.color, snapshot.player.weapon());
            gpu.hud.enemy_info.update(
                game,
                snapshot,
                gpu.console.as_ref(),
                &gpu.chat,
                labels_hidden,
                gpu.live_session.as_ref().map_or(&[], |s| s.scores()),
            );
            if let Some(vfs) = &gpu.vfs {
                gpu.hud.enemy_info.update_portrait(
                    game,
                    vfs,
                    &gpu.shaders,
                    &gpu.ui_shapes,
                    &gpu.queue,
                );
            }
            gpu.chat.observe_combat(
                snapshot,
                game,
                &gpu.obituaries,
                gpu.console.as_ref(),
                camera,
                presentation_time,
            );
        }
    }
    let japro = game
        .and_then(|g| g.config_string(0))
        .is_some_and(|info| info.windows(5).any(|w| w.eq_ignore_ascii_case(b"japro")));
    let keys = gpu
        .live_session
        .as_ref()
        .and_then(|_| gpu.local_prediction.guide_input());
    gpu.hud.restrict_guides(!gpu.third_person && !japro, keys);
    gpu.hud.guide_camera(
        view_target - view_position,
        view_up,
        vertical_fov,
        gpu.configuration.width as f32 / gpu.configuration.height as f32,
    );
}

/// Pixel rectangles (x, y, width, height) the in-game HUD program (`hud.wgsl`) can draw
/// into. Outside a menu it draws only the crosshair and the damage ring and discards
/// every other pixel, so the full-screen pass is scissored to these instead of shading
/// the whole screen; `None` keeps the full screen.
pub(crate) type HudScissors = Option<[Option<[u32; 4]>; 2]>;

impl HudUniform {
    /// The rectangles for this uniform on a `width` x `height` target.
    pub(crate) fn scissors(&self, width: u32, height: u32) -> HudScissors {
        if self.menu_open > 0.5 || self.status_visible > 0.5 || width == 0 || height == 0 {
            return None;
        }
        let [w, h] = [width as f32, height as f32];
        // Shader UVs run right and up from the bottom-left corner; rows run down.
        let rectangle = |center: [f32; 2], half: [f32; 2]| -> Option<[u32; 4]> {
            let left = ((center[0] - half[0]) * w - 2.0).floor().clamp(0.0, w);
            let right = ((center[0] + half[0]) * w + 2.0).ceil().clamp(0.0, w);
            let top = ((1.0 - center[1] - half[1]) * h - 2.0)
                .floor()
                .clamp(0.0, h);
            let bottom = ((1.0 - center[1] + half[1]) * h + 2.0).ceil().clamp(0.0, h);
            (right > left && bottom > top).then_some([
                left as u32,
                top as u32,
                (right - left) as u32,
                (bottom - top) as u32,
            ])
        };
        let visible = self.hud_visible > 0.5;
        let mut crosshair = None;
        if visible && self.crosshair > 0.0 {
            let [x, y, scale_x, scale_y] = self.crosshair_parameters;
            // Degenerate scales would put the crosshair test everywhere.
            if !(scale_x > 0.0 && scale_y > 0.0 && x.is_finite() && y.is_finite()) {
                return None;
            }
            // The outline reaches 14/26 of the crosshair size on each axis.
            let reach = 14.0 / 26.0 * self.crosshair.max(0.001);
            crosshair = rectangle([0.5 + x, 0.5 + y], [reach / scale_x, reach / scale_y]);
        }
        let mut damage = None;
        if visible && self.damage_alpha > 0.0 {
            let outer = 0.092 + 0.004 * self.damage_strength;
            let aspect = self.inverse_height / self.inverse_width;
            if !(aspect > 0.0 && outer.is_finite()) {
                return None;
            }
            damage = rectangle([0.5, 0.5], [outer / aspect, outer]);
        }
        // A HUD draw runs the entire shader, so overlapping scissors would blend
        // the crosshair twice while taking damage. Draw their union once instead.
        if let (Some(a), Some(b)) = (crosshair, damage)
            && a[0] < b[0] + b[2]
            && b[0] < a[0] + a[2]
            && a[1] < b[1] + b[3]
            && b[1] < a[1] + a[3]
        {
            let left = a[0].min(b[0]);
            let top = a[1].min(b[1]);
            crosshair = Some([
                left,
                top,
                (a[0] + a[2]).max(b[0] + b[2]) - left,
                (a[1] + a[3]).max(b[1] + b[3]) - top,
            ]);
            damage = None;
        }
        Some([crosshair, damage])
    }
}
