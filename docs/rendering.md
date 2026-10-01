# Rendering and UI

The wgpu renderer lives in `jkr-viewer`. JKA content enters through owned BSP,
model and shader data; GPU resources stay in the viewer. The renderer consumes
BSP geometry, PVS visibility, lightmaps, shader stages and legacy models.

## Where to work

| Area | Entry point |
| --- | --- |
| GPU/device setup | [gpu_context.rs](../crates/jkr-viewer/src/gpu_context.rs) |
| World loading | [world_load.rs](../crates/jkr-viewer/src/world_load.rs) |
| World materials | [world_materials.rs](../crates/jkr-viewer/src/world_materials.rs) |
| Main scene passes | [main_scene_pass.rs](../crates/jkr-viewer/src/main_scene_pass.rs) |
| Secondary views | [scene_views.rs](../crates/jkr-viewer/src/scene_views.rs) |
| Sun and real-time lighting | [sun_shadows.rs](../crates/jkr-viewer/src/sun_shadows.rs) |
| Post processing | [post_aa.rs](../crates/jkr-viewer/src/post_aa.rs) |
| Frame timing | [frame_pacing.rs](../crates/jkr-viewer/src/frame_pacing.rs) |
| HUD integration | [hud.rs](../crates/jkr-viewer/src/hud.rs) |

The normal BSP path supports additional lighting, shadows, GI probes, ambient
occlusion, reflections and post processing. Feature presence does not establish
correctness on every map or GPU. Preserve the ordinary BSP/material path when
working on optional effects and validate shared WGSL programs on an actual GPU.

## Selected controls

| Cvar | Behavior |
| --- | --- |
| `jkr_dayNight` | Map-relative sun/sky atmosphere; default 0, restart required |
| `jkr_realtime` | Lighting tier; default 2. Tier 1 retains world shadow casters between frames; tier 0 also uses available baked indirect light. Applies at map load |
| `jkr_dayHour` | Solar hour, updated live when day/night resources are installed |
| `jkr_dayMinutes` | Minutes per simulated day; 0 holds the hour |
| `jkr_hdr` | Scene precision: 0 display format, 1 RGBA16F; restart required |
| `jkr_hdrExposure` | Fixed exposure multiplier, 0.25–4; restart required |

See [day_night.rs](../crates/jkr-viewer/src/day_night.rs),
[sun_shadow_settings.rs](../crates/jkr-viewer/src/sun_shadow_settings.rs) and
[post_hdr.rs](../crates/jkr-viewer/src/post_hdr.rs). A lighting tier alone does not
enable the day/night system. HDR here describes the scene buffer and display
mapping, not a claim of HDR monitor output.

Set `JKR_FRAME_BUDGET=1` for frame-work and GPU-phase diagnostics. Measurements
must name the build mode, GPU, resolution, settings, map and population. Separate
loading/shader warmup from steady frames and CPU work from GPU timings. The
500+ FPS target remains open; neither a single GPU timestamp nor an uncapped
empty scene demonstrates it.

## UI ownership

`jkr-ui` provides renderer-independent retained widgets. The viewer supplies GPU
and text integration and binds client state to the HUD. Layouts are data in
[assets/hud](../crates/jkr-viewer/assets/hud); menus and HUD may be modern while
movement, combat and network behavior remain compatible.
