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

## Saber trails

[saber_trail.rs](../crates/jkr-viewer/src/saber_trail.rs) follows codemp
`CG_AddSaberBlade` and `CTrail`: every frame at least 3 ms after the last, a
blade adds one slice from its remembered muzzle and tip to the current ones.
A slice lives `trailLen / 5` ms of the current `saberMove` (30–40 ms for most
moves, 40 ms when the move authors none) and fades by scrolling the clamped
blur texture, not by alpha. The short visible arc is stock behavior; frame rate
changes the slice count, not the arc's duration. Slices split along new tip to
old muzzle as `CTrail::Draw` does. With `cg_saberContact` on, the tip stops at
the first world surface the blade enters
([saber_trail_edge.rs](../crates/jkr-viewer/src/saber_trail_edge.rs)); stock
also stops it at solid brush entities, which JKR does not trace yet. A flying
primary saber trails and shares the owner's blade state, as in stock. Not yet
drawn: the extra trails stock adds while `PW_SPEED` is set with `cg_speedTrail`
and during super-break win animations.

Blade/wall contact ([saber_contacts.rs](../crates/jkr-viewer/src/saber_contacts.rs))
plays a wall-hit sound once a blade has stayed in the wall since the previous
frame, at most every 100 ms per blade. Like stock's `S_StartSound(..., -1,
CHAN_WEAPON, ...)`, all wall hits share one source and channel, so each new hit
replaces the previous one instead of overlapping it.

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

## Submission and lighting work reduction

The renderer records uploads with their frame and hands completed batches to a
submission thread. It finishes outstanding encoder work, applies those uploads,
submits and presents in order while the render thread prepares the next frame.
Only one handed-off frame can remain outstanding. With an offscreen scene target,
swapchain acquisition happens after world recording; direct-to-surface rendering
still acquires its image first. Resize, out-of-band submissions and teardown wait
for the outstanding batch. `JKR_SUBMIT_THREAD=0` selects inline submission as a
fallback. See [frame_queue.rs](../crates/jkr-viewer/src/frame_queue.rs),
[frame_split.rs](../crates/jkr-viewer/src/frame_split.rs) and
[frame_target.rs](../crates/jkr-viewer/src/frame_target.rs).

Upload staging reuses byte and operation storage after warmup. Queue clones share
a synchronized recording; this replaces immediate wgpu upload work on the render
thread, rather than making all queue operations lock-free. Large load-time writes
can bypass recording once pending submissions have completed.

The in-game HUD shader is restricted to crosshair and damage-indicator regions.
Intersecting regions become one rectangle so translucent pixels blend once.
Menus and the shader's status-bar fallback retain full-screen coverage.

Uncached deferred-light receivers are collected into a pixel list and shaded by
four compute lanes per receiver. Cached receivers keep their existing path. The
indirect dispatch uses bounded rows to support large light buffers; the final lamp
sum can differ slightly in floating-point rounding from a serial sum. Receiver
depth identifies valid texels, allowing attribute and light targets to retain data
outside regions that will be overwritten or sampled. Floor mirrors use separate
light images, preserving the main view without save/restore copies.

Static sun-shadow bounds form an exact min/max mip hierarchy, starting at 8-texel
tiles. A receiver chooses a level covering its filter footprint with at most four
tiles. It skips the existing filter only when those conservative bounds determine
the result; shadow radii, tap counts, cascades and visual settings are unchanged.


Verification on 2026-10-02 compared this change against `f3f3db2`, using external
release replay instrumentation on Linux, Ryzen 5 5500 and Radeon RX 9060 XT
(RADV). Each 2560×1080 run measured 3,330 frames after replay warmup, with a
31-player roster and unchanged graphics settings; visible/submitted actors ranged
from 20–23 on `ffa3` and 16–27 on `ffa1`.

| Route | Mean total frame | Mean GPU | Total-frame p99 |
| --- | --- | --- | --- |
| `mp/ffa3` | 2.464 → 1.822 ms | 1.780 → 1.697 ms | 3.517 → 3.316 ms |
| `mp/ffa1` | 3.042 → 2.157 ms | 2.337 → 1.986 ms | 3.757 → 4.263 ms |

These paired offscreen runs establish a mean improvement on the sampled routes,
not universal sub-2 ms performance or improved tail latency. The `ffa1` p99 was
higher despite its lower mean; longer native play and more hardware remain open.
They exclude live networking/audio and window presentation. Comparison captures
at 2560×1080 and 3840×2160 retained the scene appearance: maximum channel error
was 4/255 on `ffa3`, 1/255 on `ffa1` outside its wall-clock kill-feed text, and
3/255 in the 4K `ffa1` snapshot. This is finite image coverage.

External checks covered overlapping HUD regions and viewport bounds at five
resolutions through 8K, indirect-dispatch boundary cases and 7,308 shadow-bound
footprints including non-power-of-two maps. Formatting, locked workspace build
and tests, and release client/server builds passed. Verification harnesses and
reports are kept outside the source repository.
Two 45-second native `ffa1` replay process runs also passed without panic or GPU
validation errors: threaded submission with HDR/FXAA, and inline submission with
HDR/FXAA disabled to exercise direct surface acquisition. Both used isolated
1280×720 settings; these are integration checks, not performance measurements.

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
