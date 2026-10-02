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
| Optional material maps | [material_maps.rs](../crates/jkr-viewer/src/material_maps.rs) |
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
| `r_normalMapping` | Normal maps on lightmapped world surfaces (rend2 convention); default 0, restart required |
| `r_specularMapping` | Specular, roughness and metalness maps on the same surfaces; default 0, restart required |
| `r_parallaxMapping` | Parallax from the height in `_nh`/`normalHeightMap` images; needs `r_normalMapping`; default 0, restart required |
| `r_materialMapsDebug` | Material-mapped surfaces only: 1 mapped normal as colour, 2 tint by maps found, 3 normal-map relief; default 0, live, not archived |

See [day_night.rs](../crates/jkr-viewer/src/day_night.rs),
[sun_shadow_settings.rs](../crates/jkr-viewer/src/sun_shadow_settings.rs) and
[post_hdr.rs](../crates/jkr-viewer/src/post_hdr.rs). A lighting tier alone does not
enable the day/night system. HDR here describes the scene buffer and display
mapping, not a claim of HDR monitor output.

Sun-shadow filtering compensates for the receiver surface's slope across the
full texel footprint of a linear depth comparison. A half-texel allowance only
covers samples midway between texel centers and can produce bands of false
self-shadowing on flat surfaces. Keep this allowance tied to the cascade's
texel size and receiver slope; increasing a fixed world-space offset can detach
shadows from walls and ground contacts.

The full-footprint correction was checked against the preceding shader from
`4a8fe31` with external release captures on Linux/RADV (Radeon RX 9060 XT), at
1920×1080 with HDR, lighting tier 0, 2048-pixel shadow maps and a fixed 9:00 sun.
The reported `mp/ffa3` view and a nearby camera position lost the ground bands
while retaining the ship and wall shadows; an `mp/ffa1` interior comparison
showed no obvious regression. GPU frame means were 3.336 → 3.341 ms for the
nearby `ffa3` view and 4.122 → 4.125 ms for `ffa1` (64 measured frames each).
These isolated captures are not a full gameplay benchmark or coverage of every
map, sun angle, graphics backend or shadow quality setting.

Sun-shadow cascades share a world-space reconstruction footprint while blending.
The close transition covers the last half of its axial range; the view/far
transition covers the last quarter. The minimum footprint is based on 1.5 texels
of the active cascade, interpolated toward the finer map during a transition.
This avoids cross-fading independently sharp and soft versions of an edge.
The accepted transition refinement added approximately 0.074 ms of GPU work in
the marked 4K ship view on Linux/RADV (Radeon RX 9060 XT, tier 0, HDR,
2048-pixel shadow maps, 16 base taps, fixed 11:00 sun). It does not establish
complete temporal invariance.

Static world and moving casters now keep separate depths in the close and view
cascades. A nearby player cannot replace a distant building's depth in the world
filter's separation estimate. Receivers multiply the independently filtered
visibilities. This approximates their union; it is not exact area-light visibility
for multiple blocker depths. Volumetric lighting samples both depths at each tap.
Tiers 0/1 reuse the existing static maps without copying them under moving casters.
Tier 2 refreshes separate world maps each frame, adding two depth textures
(32 MiB at 2048 pixels, 128 MiB at 4096) and two render-pass boundaries.

World penumbra width still grows with caster-to-receiver separation. The blocker
search bilinearly reconstructs positive gaps and coverage from 16 positions,
correcting for the receiver plane at each texel. Its reach is 24 world units,
expanded if needed for the minimum reconstruction footprint; it no longer clips
the close cascade's broad penumbrae at twelve tiny shadow texels. Reconstruction
uses a truncated Gaussian disk to reduce the visible rim of an equal-weight disk.
`jkr_shadowTaps` (4–32) supplies the base count, with up to four times that budget
for broad filters and a fractional final tap for continuous count changes.
The full-texel slope correction and small normal offset remain unchanged.

Moving casters use the shared contact footprint, independently of the building's
penumbra. Their shadows therefore remain comparatively sharp even when a moving
caster is high above its receiver. Actor-only shadow mode retains separation-based
filtering. Neither approach resolves all shadow-map occlusion or sampling limits.

The preceding combined-map RMS filter was rejected in owner playtesting: the
reported ground-fixed boundary and player interaction remained visible, especially
when structures were far from the surface receiving their shadow. The current
separation/filter changes were checked at the third `mp/ffa3` mark
`(-425.691, -1061.451, 73.832)`, yaw `132.718`, pitch `38.102`, with an external
release harness carrying the production shadow code from the working tree based
on `980e693`. Linux/RADV captures on the same RX 9060 XT at 1080p and 4K,
HDR, tier 0, 2048 maps, 16 base taps and fixed 11:00 sun showed a smoother broad
edge. A frozen Kyle model was moved through seven positions in the actual dynamic
caster pass. GPU attribute readback found no visibility increase on unchanged
receivers when adding the actor (over 515,000 compared pixels per position).
The previous combined-map filter's maximum increase was only 0.000119 in this
particular probe; the more visible improvement is the distinct player shadow
instead of its inheriting the building's broad blur. This is not a complete
reproduction of every reported player interaction.

Earlier wall/ship views and a seven-position camera approach were also captured.
Tier 2, day/night disabled and actor-only modes passed GPU smoke checks. At 4K,
64-frame means compared with the preceding combined-map filter were
1.579 → 2.100 ms for the light pass and 4.212 → 4.700 ms for GPU work excluding
capture/readback, approximately 0.49 ms added. These are empty-scene measurements
on an active desktop, with release compilation running concurrently, not a
populated-match benchmark. Workspace build/tests, formatting and the production
release build passed; Cargo runs no bundled regression tests. The owner accepted
the release playtest on 2026-10-02. Broader live-animation checks, other GPUs and
exhaustive quality/map coverage remain open.

Set `JKR_FRAME_BUDGET=1` for frame-work and GPU-phase diagnostics. Measurements
must name the build mode, GPU, resolution, settings, map and population. Separate
loading/shader warmup from steady frames and CPU work from GPU timings. The
500+ FPS target remains open; neither a single GPU timestamp nor an uncapped
empty scene demonstrates it.

Static sun cascades carry a small min/max depth atlas: one pair of full-precision
bounds per 64×64 shadow texels, with separate view, close and far layers. A layer
is rebuilt immediately after its source map changes. Receivers bound the entire
possible filter footprint, including bilinear neighbours and receiver-plane
variation. Only footprints proved fully lit or fully blocked bypass the blocker
search and Gaussian reconstruction; uncertain footprints run the existing filter.
Moving-caster filtering is unchanged. At 2048 resolution the atlas occupies
24 KiB. Coalesced tile reads keep rebuilding practical in tier 2 as well as the
held-cascade modes. Golden-angle sample directions are constant shader data;
sample counts, radii, weights and cascade transitions are unchanged.

Real-time world shading skips baked-lightmap texture reads when the composition
hook replaces their RGB. Uploaded BSP lightmaps and their fallback have alpha
one; authored image alpha and animation still follow their normal paths.
The expensive main-view and floor-reflection material passes also prime depth for static surfaces
whose first stage guarantees opaque coverage. Deforms, sprites, alpha tests,
polygon offsets and special depth/blend modes keep their ordinary path. Depth
priming reuses the existing visibility ranges and indirect argument storage,
with direct draws as a fallback. It is limited to active real-time lighting;
reflections use their own camera, depth target, receiver frustum and scissor.

Camera-range caches also retain their PVS/area selection independently of the
camera frustum. Turning or moving within a cluster rechecks bounds but reuses
the same candidate indices; a source-cluster, area-mask or PVS-mode change
invalidates that selection. Storage is reserved at map load, at one index per
static draw. Unbounded and blended materials keep direct traversal.

External release replay checks compared these changes with `b4debe4` on
Linux/RADV, Ryzen 5 5500 and Radeon RX 9060 XT. Each recording contains 31
player profiles on `mp/ffa3` or `mp/ffa1`; 19–23 and 16–27 actor groups,
respectively, were evaluated during the measured routes. Settings were render
scale 1, HDR, day/night held at 11:00, lighting tier 0, 2048 shadow maps, 16 base
taps, volumetrics 1 and anisotropy 16. After five seconds of replay warmup,
6,660 frames sampled a 20-second route at 333 simulated frames per second.

| Replay / resolution | GPU mean before → after | Total frame mean before → after |
| --- | --- | --- |
| `mp/ffa3`, 3840×2160 | 5.302 → 4.575 ms | 5.473 → 4.746 ms |
| `mp/ffa1`, 3840×2160 | 6.714 → 5.808 ms | 6.894 → 5.983 ms |
| `mp/ffa3`, 2560×1080 | 2.026 → 1.845 ms | 2.499 → 2.474 ms |

These are offscreen full-frame replays, with no per-frame readback or capture
copy and no live network/audio output. They show a CPU limit at the smaller
resolution, not achievement of the 2 ms total-frame goal. Native presentation,
live matches, other GPUs and exhaustive community-content coverage remain open.

External GPU instrumentation compared the conservative shadow result with the
full filter at 656,749,476 accepted samples across both maps, moving sunlight,
tier 2, 1025-pixel shadow maps with 32 base taps, and 4K output. Maximum
visibility error was 0.00000012; none exceeded the 0.000002 verification threshold.
Twenty 4K scene snapshots across both routes retained the reference appearance;
one snapshot's kill-feed expiry differed with wall-clock timing. Baked lighting
and actor-only captures matched byte for byte. Fullbright, lightmap debug and
the non-table/direct-draw fallback also passed GPU and image checks. These
finite samples are not exhaustive image equivalence for every view or material.

Formatting, locked workspace build/tests and the release client build passed.
Cargo still runs no bundled regression suite. A native 1280×720 `mp/ffa3` static
map run rendered for a 45-second process lifetime without panic or GPU validation
error, using an isolated config. This is an integration smoke check, not a
populated-match performance result. The combined retained changes also completed a
45-second native `ffa1` crowd-replay process run with an isolated configuration
and no panic or GPU validation error. Concurrent compilation makes that latter
run an integration check only.

A follow-up CPU cache comparison used the same routes/settings at 2560×1080:
`ffa3` world-pass encoding fell from 0.565 to 0.513 ms and total frame mean from
2.462 to 2.420 ms. `ffa1` encoding fell from 0.884 to 0.866 ms; its total mean
was essentially unchanged (3.119 to 3.131 ms). An external verifier matched
3,178,666 cached/direct range results across the replays and forced area-mask,
source-cluster and missing-PVS transitions. Twenty further 4K captures retained
the scene appearance, with small pixel differences and the known wall-clock
kill-feed difference. This is finite coverage, not a universal cache proof.

Floor-reflection depth priming was compared in alternating order on the first
10 seconds of the `ffa1` route at 4K (3,330 frames per run, two runs per variant).
Mean GPU time was 5.453 → 5.417 ms; the reflected-plane phase was
1.116 → 1.081 ms. On the full 20-second route at 2560×1080, total frame means
were 3.135 → 3.094 ms. Twenty 4K captures across both maps showed only small
pixel differences (maximum 8/255), with the shadow filter and reflection
resolution unchanged. Paired 4K `ffa3` runs measured essentially unchanged
GPU time (4.547 versus 4.550 ms), with almost no floor-reflection work on that
route. Other mirror/portal types retain their previous path.

Particle stage selection borrows the atlas and evaluates stages as they are
consumed, retaining the existing eight-stage cap, order, animation, waveform,
texture transforms and missing-shader fallback. This avoids filling and copying
eight samples for every effect, including callers that only need the first.
The sampling implementation lives outside the frame-orchestration module.
Paired 3,330-frame `ffa3` runs at 2560×1080 reduced billboard preparation from
about 0.067 to 0.047 ms and effect geometry preparation from 0.034 to 0.029 ms.
Total-frame differences were within run variation; this is a CPU phase result.
An external comparison matched 3,360 stage samples bit for bit across both
loaded atlases and missing, empty and over-capacity shader cases.


Entity sorting caches each source material's immutable shader sort and first-stage
pipeline keys at material creation, including late custom materials. It preserves
missing-material defaults, opaque tie-breaking and back-to-front blend order;
per-frame draw records and allocation behavior are unchanged. Alternating
6,660-frame runs at 2560×1080 reduced instance preparation from about 0.092 to
0.078 ms on `ffa3` and 0.085 to 0.075 ms on `ffa1`. Total-frame differences were
within run variation. An external comparison matched 1,362,200 ordered draw
entries against the original comparator across both routes. Twenty further 4K
captures retained the scene appearance (one pixel differed by 12/255; all others
by at most 8/255).
The draw queue also classifies stage-major eligibility once when a draw is added,
rather than repeating material lookups in every colour pass. Missing materials
and depth-less draws retain their previous handling, and the 32-byte draw size is
unchanged on the measured 64-bit build. Two alternating 6,660-frame runs per
variant reduced CPU world-pass encoding by roughly 0.011 ms on `ffa3` and
0.016 ms on `ffa1`; total means fell by 0.034 and 0.026 ms respectively. The
same external verifier checked every cached classification and all material keys,
including missing-key defaults, across both replays; 20 captures showed only
small pixel differences (maximum 13/255 at one pixel).

Volumetric integration carries each slice's end depth into the following slice
instead of recomputing the same boundary. Sample locations and accumulated
scattering retain their existing arithmetic. Paired 3,330-frame 4K runs on both
routes saved about 0.0033 ms in the volumetric phase; 20 captures retained the
scene appearance, with small pixel differences (maximum 12/255 at one pixel).

Ambient occlusion reuses the fixed 16 sample directions and radii, preserving their
f32 expressions, sample order, reach and strength. Depth reconstruction omits the
ray normalization that cancels in its intersection ratio and scales the degeneracy
guard accordingly. Paired 3,330-frame 4K runs reduced the AO pass from 0.369 to
0.344 ms on `ffa3` and 0.354 to 0.330 ms on `ffa1`; total GPU means fell by
0.023 and 0.026 ms. Twenty captures retained the scene appearance, with one pixel
differing by 12/255 and all others by at most 8/255. These are measurements on the
same Vulkan setup, not exhaustive equivalence across all maps and backends.

## Material maps

The optional material maps follow OpenJK rend2 (`codemp/rd-rend2`), so rend2
texture packs apply without conversion. Stage keywords (`ParseStage` in
`tr_shader.cpp`) are parsed by
[jkr-shader](../crates/jkr-shader/src/material.rs): `normalMap`,
`normalHeightMap`, `specMap`/`specularMap`, the packed `rmoMap`, `moxrMap` and
`ormMap` families, and `specularReflectance`, `specularExponent`, `gloss`,
`roughness`, `normalScale`, `specularScale`, `parallaxDepth` and `parallaxBias`.
Their order-dependent overrides are kept. rend2 selects a packed layout by
comparing the image name with the keyword, so `rmosMap`, `mosrMap` and `ormsMap`
load the three-channel layouts; JKR does the same. Without keywords,
[the lookup](../crates/jkr-viewer/src/material_map_images.rs) tries `<diffuse>_nh`
then `_n` for normals and `_specGloss`, ioquake3's `_s`, `_rmo` then `_orm` for
specular, as in rend2's `CollapseStagesToGLSL`. ioquake3's typed
`stage normalMap` stages are not supported. With the cvars off, the parser
records the keywords and nothing else changes: no image lookup, layout,
buffer or pipeline is created.

Maps apply to lightmapped world surfaces (static and inline movers) whose
lightmap and diffuse stages collapse into one opaque pass. On the retail
`mp/ffa1`, `mp/ffa3` and `mp/duel1` this covers 84–87% of world triangles;
`mp/siege_hoth` covers 52%. Vertex-lit surfaces, stacks that do not collapse,
effect stages, deforms, sprites and models (MD3, Ghoul2) keep their authored shading.
Each material-mapped stage compiles to its own pipeline key and a second bind
group; ordinary stages keep their pipelines, groups and stage-table records.

Map load computes [vertex frames](../crates/jkr-viewer/src/material_map_frames.rs)
for the flattened world only when a stage has maps: a tangent with handedness,
averaged over the triangles of patch and triangle-soup vertices, and the
light-grid direction (`R_CalcVertexLightDirs`/`R_LightDirForPoint`). They use 8
bytes per vertex. Building them for `mp/ffa3` (114,088 vertices) took about 24 ms
in a release test build. Maps are uploaded as linear RGBA8 with box-filtered mips.

Shading lives in [material_maps.wgsl](../crates/jkr-viewer/src/material_maps.wgsl):

- Baked lighting: rend2's lightmap response, with the light-grid direction in
  place of a deluxemap. The texel is taken as arriving along that direction,
  divided by the face's own cosine (at most 4x) and received by the mapped normal;
  the remainder stays ambient. A flat normal map reproduces the texel. Retail BSPs
  have no deluxemaps, so this is an approximation.
- Real-time lighting (`jkr_dayNight 1`): the sun share of the half-resolution
  light buffer is moved to the mapped normal per pixel, using the visibility the
  buffer keeps. It fades out toward the terminator, so mapped bumps never light a
  face turned from the sun or a shadowed texel. Lamps and probe bounce remain as
  the light pass evaluated them for the geometric normal. Without a specular map,
  the existing sun highlight and sky rim use the mapped normal.
- Specular maps use rend2's two paths: spec/gloss, and occlusion, roughness,
  metalness and specular with the albedo as metal colour. Highlights use rend2's GGX
  `CalcSpecular` for the sun or grid direction and for dynamic lights. They are
  added after the albedo and dynamic-light modulation. Occlusion darkens only the
  ambient share.
- Parallax uses rend2's 16 linear and 8 binary steps through the height.

As in rend2, frames come from the untransformed texture coordinates (`tcMod`
rotation misaligns them) and an `animMap` stage uses its first frame's maps.
Material-mapped stages stay off the stage table (which only real-time mode
uses): the CPU colour pass draws them with per-stage bind groups and pipelines.

The controls are sampled once, when the GPU context is created. A change after
that logs `<cvar> changed: restart the viewer to apply material maps`; the
configuration file setting them at startup does not. Each map load logs what
it found, for example `material maps (normal+specular+parallax): 429 stages,
429 normal, 57 parallax, 429 specular; frames for 77283 vertices in 18 ms`, or
`no stage of this map has maps` when the cvars are on and nothing was found.

### What to expect, and checking it

`r_materialMapsDebug` is live and draws only material-mapped stages (one uniform
branch in the material program; the ordinary programs do not change): 1 shows the
mapped world-space normal as colour, 2 tints each stage by the maps it found (red
parallax, green normal, blue specular, so normal plus specular is cyan), 3 shows
the normal map's relief, four times its departure from the face, on grey.
Surfaces without maps keep their ordinary look in every view, so 2 shows at a
glance which surfaces take maps.

The baked response is subtle by construction. rend2 credits all of a lightmap
texel to one direction, so a normal tilted by a small angle α changes the texel
by about tan θ · α, where θ is the angle between the face and the light-grid
direction. A face whose grid direction is more than 78° from its normal falls
back to that normal (`R_LightDirForPoint`) and then only darkens by 1 − cos α. On
the retail `mp/ffa3`, 59% of lightmapped vertices have a usable grid direction
(55% on `mp/duel1`), with a mean tan θ of 1.5 there. Maps generated for ffa3
and duel1 by `jkr-materialgen` (strength 1) are gentle: their normals tilt 3.8°
on average (90th percentile 4–17° per image), so lighting changes by about 5%
on the surfaces that respond. A local headless render (not committed) of six
ffa3 spawn views at 960×540 (Vulkan, RTX 5080), with the generated maps and an
HD texture pack, measured against the cvars off: normal maps alone changed
pixels by 0.06–1.8/255 on average (at most 36/255), specular maps by
0.6–2.6/255 (at most 11/255; the generated `_rmo` maps average roughness 0.68
and metalness 0.02, so highlights are faint). Parallax changed pixels by up to
23/255 on average, but by shifting the texture: it reads as the same texture,
not as relief. Turning the cvars on in baked lighting therefore
shows no obvious change with these maps; that is the model, not a missing draw.
Authored rend2 packs with stronger normal maps respond in proportion to their
tilt.

Real-time lighting shows normal maps far more clearly: the sun share is moved
per pixel, and a low sun lights floors at grazing angles. With `jkr_dayNight 1`
and `jkr_dayHour 7`, the sun stands 15° high, so a floor's tan θ is about 3.7
(2.4 at the default hour 7.5) against 1.5 for baked grid directions. This
estimate is from the formulas above; no real-time render of real content has
been measured.

Unit tests cover keyword parsing, lookup order and conversions (synthetic
in-memory images), frames (planar, mirrored and curved), the stage selection
and the pipeline key. naga validates both material programs. A headless Vulkan
render of a synthetic quad on an RTX 5080 (Windows 11, external harness) checked
the response in both modes. Flat maps matched the ordinary program exactly, tilted
normals brightened and darkened as computed (green follows +t, rend2's
convention), and normal maps did not lighten a shadowed buffer texel.

The same harness timed one full-screen 3840×2160 layer (64 runs, medians):
baked lighting 0.103 ms ordinary, 0.196 with a normal map, 0.219 with a
specular map as well and 0.50 with parallax. With the light buffer, those passes
took 0.226, 0.30, 0.34 and 0.54 ms. Measure a real scene with `JKR_FRAME_BUDGET=1`
in a release build: compare the same map, view and population with the cvars on
and off. No authored rend2 pack was available for testing. Generated maps on
real ffa3 data were rendered headless in baked lighting only (above); real-time
lighting on real content, an in-game image and the frame cost in a match remain
unverified.

## UI ownership

`jkr-ui` provides renderer-independent retained widgets. The viewer supplies GPU
and text integration and binds client state to the HUD. Layouts are data in
[assets/hud](../crates/jkr-viewer/assets/hud); menus and HUD may be modern while
movement, combat and network behavior remain compatible.

## Billboard icons

Frame billboard icons follow OpenJK's `RT_SPRITE` image orientation: texture v=0
belongs at the top of the quad. Their local v is reflected before the shader's
scale/scroll transform; ordinary FX billboards retain their existing convention.
This covers simple-item icons and player-status icons when submitted through
that path. The owner reported an inverted talk balloon during the player-icon
PR playtest. An external probe compared the corrected production transform with
OpenJK `RB_AddQuadStampExt`: four corners with three scale/scroll transforms
matched, while ordinary FX transforms were unchanged. Native visual confirmation
of the correction remains pending.
