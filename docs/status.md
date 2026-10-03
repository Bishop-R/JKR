# Status and priorities

Reviewed 2026-10-03 against source baseline `8f692ac` and the owner-approved
rendering and transition changes described below.

JKR currently contains a native client and standard dedicated server in a
20-crate Rust workspace. This page records scope and verification, rather than
claiming complete parity from the presence of an implementation.

## Implemented scope

- PK3/loose-file content, BSP maps/collision, legacy models and shader scripts.
- Protocol-26 client/server sessions, shared JKA game rules and client prediction.
- Graphical client with browser, menus/settings, HUD, console, audio, screenshots,
  demos and a Create game flow.
- wgpu BSP renderer with optional modern lighting and post processing.
- Dedicated-server game integration, console/configuration, stock game-type
  options, map entities, bots/NPCs and script integration.

Entry points and ownership are linked from [architecture.md](architecture.md).
The lists above describe source coverage; they do not close the validation gaps below.

## Verification recorded for this baseline

| Check | Result and scope |
| --- | --- |
| Linux workspace build, Cargo tests and formatting | Passed; no bundled regression tests currently run |
| Optimized client and dedicated-server build | Passed with Rust 1.96.1 on Linux |
| External OpenJK movement reference checks | 560 cases / 72,275 commands at 8/7/4/3 ms; movement, animation, events and compared wire fields matched |
| External player-angle checks | 125 samples matched the OpenJK reference |
| Local OpenJK client against JKR server | Joined `mp/ffa3`, walked, jumped and turned; no prediction misses observed in that run |
| Native Vulkan rendering | Local JKR client/server completed joined-map rendering on `mp/ffa3` with defaults and with day/night + lighting tier 2 + HDR; each continued for 15 seconds without panic or GPU validation error |

The reference checkout used for the external checks was OpenJK
`1a6a643427aa347553e9073dac5570b33337c4d9`, multiplayer `codemp`.
The external harnesses and raw reports are not part of this repository, so these
are recorded maintainer results, not checks reproducible by running `cargo test`
alone. The GPU runs used debug builds and establish startup/integration only;
they do not establish visual parity or release performance.

Sun-shadow correction (2026-10-02, based on `4a8fe31`): external release GPU
captures reproduced and removed ground self-shadow bands in the reported
`mp/ffa3` view, including a nearby camera position. An `mp/ffa1` comparison
showed no obvious regression. See [rendering](rendering.md) for settings,
timings and limits. The updated production release client also rendered
`mp/ffa3` on Linux/Vulkan (Radeon RX 9060 XT) with day/night and HDR enabled
without a panic or GPU validation error during a short startup check.
The owner also playtested the release build and confirmed that the reported
view looked clean.

Sun-shadow edge refinement (2026-10-02, based on `980e693`): the owner
accepted the release playtest with smoother structure shadows and more stable
player-shadow overlaps. Cascades share a reconstruction footprint and wider
transition bands. World and moving-caster depths are filtered independently;
a world-space blocker search and Gaussian reconstruction smooth broad edges
without letting a player change the building's separation estimate.

External release GPU evidence covers the three marked `mp/ffa3` views, camera
approaches and seven positions of an actor in the dynamic-caster pass. Adding
the actor did not brighten unchanged receivers in that probe. Tier 2, day/night
disabled and actor-only GPU smoke checks passed, as did formatting, workspace
build/tests and the release client build. The tested 4K view adds about 0.49 ms
of GPU work over the preceding playtest filter. See [rendering](rendering.md)
for settings, the overlap approximation, memory cost and remaining limits.

Performance work based on `b4debe4` (2026-10-02) preserves the accepted shadow
filter while skipping provably constant footprints, unused baked-lightmap reads
and hidden opaque shading. External 31-player replays show 4K GPU means falling
from 5.30 to 4.57 ms on `mp/ffa3` and 6.71 to 5.81 ms on `mp/ffa1`. At
2560×1080, `ffa3` GPU work is 1.85 ms but total frame time remains 2.47 ms:
the 2 ms target is still open. Shader reference comparisons, alternate-mode
captures, workspace checks and a native release smoke check passed; see
[rendering](rendering.md) for settings, evidence and limitations.
An additional PVS/area candidate cache reduced CPU world-pass encoding by
0.052 ms on the smaller `ffa3` replay and matched 3,178,666 direct-traversal
results, including forced visibility transitions. Its total-frame gain was
0.042 ms there; the reflection-heavy `ffa1` route was essentially unchanged.
Extending opaque depth priming to floor reflections then saved about 0.036 ms
of GPU work in paired 4K `ffa1` runs; lower-resolution timing and captures on
both maps also passed. Reflection resolution and shadow filtering are unchanged.
Lazy particle-stage sampling removes about 0.025 ms of measured effect preparation
on the `ffa3` replay; 3,360 sampled stage values matched the previous implementation
bit for bit. Its total-frame effect was within run variation.
Caching immutable material sort keys saves another 0.01–0.014 ms in instance
preparation on the two routes. The original comparator matched 1,362,200 ordered
draw entries; total-frame improvement remains below run variation.
Reusing each draw's stage-major classification removes a further 0.011–0.016 ms
of CPU world-pass encoding in paired runs, with unchanged draw storage size
and reference-checked classification.
Fixed AO sample tables and equivalent depth-ray arithmetic save another
0.023–0.026 ms of total GPU time in paired 4K runs, with unchanged AO settings
and checked captures on both routes. The owner playtested and accepted the
combined performance preview before publication.


Submission and deferred-lighting work based on `f3f3db2` (2026-10-02) records
frame uploads for a bounded submission worker, restricts HUD shading, shades
uncached lamp receivers in compute, avoids redundant clears/copies and uses
conservative shadow-bound mip levels. On Linux with Ryzen 5 5500 / RX 9060 XT,
external 31-player replays at 2560×1080 reduced mean total frame time from
2.464 to 1.822 ms on `ffa3` and 3.042 to 2.157 ms on `ffa1`. The latter's p99
increased from 3.757 to 4.263 ms, so improved tail latency is not established.
Finite image comparisons and workspace/release checks passed; see
[rendering](rendering.md#submission-and-lighting-work-reduction) for evidence
and limits. Gameplay and protocol code are unchanged.

Resident world transitions (2026-10-03, local changes based on `8f692ac`):
menu joining and live map changes retain a rendered, locally playable world.
External release checks against an isolated loopback TaystJK server exercised
FFA3 entry, FFA3 → FFA1, same-map restart, and two consecutive map changes.
Cancellation after entering the destination restored the menu, and the transition
sequence also passed with a local bot present. Artificially delaying delivery of
the completed session from the connection worker allowed more than 1,600 locally controlled
frames before the verified session attached without rebuilding the map. That
check exercises delayed handoff, not a real slow-network handshake. The
same-map restart reattached about 50 ms after its transition event. Old-map
movement and rendering continued while the replacement was built, without a
loading overlay or old remote actors in the resident runtime world.

The installed-archive checksum comparison matched ordered CRC sequences for all
73 previously readable PK3s. Six small external ZIP cases covered empty archives,
empty files, Unicode names, duplicate names, an executable prefix and ZIP64
sizes; malformed central-directory data was rejected. A catalogue with an invalid
unused local payload header is now inventory-readable, matching OpenJK's
central-directory inventory behavior; loading an affected asset still validates
its header/data. One installed-set comparison measured 4,697 ms for the old
payload-header inventory and 134 ms for the directory reader. These checks do not
constitute a new pure-server/wire certification; protocol codecs were unchanged.

Checks used Linux/RADV on a Radeon RX 9060 XT, 960×540, owner graphics settings,
and external ignored harnesses. Normal joins measured about 7–18 seconds in these
runs, depending on machine/cache pressure; the early pre-optimization run took
41 seconds. Typical FFA1 background preparation was about 8–12 seconds. These
are observations, not a controlled cold-cache speedup or a populated-match frame
budget result. Simultaneous build pressure worsened some runs substantially.
One default-backend headless run emitted an EGL destruction panic on the submit
thread after all assertions passed and while exiting. The explicit Vulkan rerun
completed without that diagnostic; native-window/backend shutdown coverage is
still needed. Cold loading, missing-content downloads, platform coverage and visual acceptance
remain open. The movement adapter calls the existing predictor; the external
OpenJK on-foot/force-jump comparison still passed 560 cases / 72,275 commands,
including 8/7/4/3 ms caps. See [client transitions](client.md#joining-and-changing-maps)
for local-authority and exploration limits.

Local gameplay continuation (2026-10-03, same unmerged baseline): departed live
worlds now run the native dedicated gameplay behind a socket-free client session.
Release GPU checks on isolated loopback TaystJK exercised FFA3 → FFA1, same-map
restart, rapid map changes and cancellation, both with and without a bot. They
asserted that only the local player remained, local saber moves advanced, and
the real connection reattached. Screenshots confirmed the third-person model and
saber remained visible. A further run granted test-only weapons and observed
pistol projectiles after releasing the saber attack and switching weapons. It
injected an intermission movement type at handoff to check recovery from the
cached playable state; natural match-end timing remains unverified. A stock
FFA3 door import check preserved its open position, area portal and closing timer.
Native import/reuse checks covered slots 0/17/31 at
8/7/4/3 ms; the 560-case movement/animation/event fixtures and compiled codemp
weapon zoom/charge fixture passed. These are focused checks, not complete native
combat, mod or vehicle compatibility certification. First gate entry is still
movement-only until a server player is available. Its original connection notice
and pointer Cancel action are restored for menu joins; in-server map changes
keep the local gameplay presentation without that overlay. The notice restoration
passed workspace build/test/format checks; native visual acceptance is pending.

In the final 960×540 Vulkan run, local render calls after the first ten local
frames measured 0.98 ms median, 2.72 ms p99 and 38.35 ms maximum, excluding the
harness sleep and capture work. This is a single-player continuation while
background loading, not a 31-player benchmark. Initial model, effect and shader
work can still hitch; moving native skeleton loading into background preparation
removed the observed roughly 0.6-second first-saber-command stall in that run.
It does not establish hitch-free transitions or complete content coverage.

Transition/input polish (2026-10-03, unmerged changes based on `8f692ac`):
a queued Alt bind was reproduced surviving focus loss; focus handling now drops
that frame's gameplay input and ignores synthetic keyboard events. Isolated
TaystJK and native JKR runs observed a genuine saber throw return after focus
loss. The retained actor keeps its animation tracks and displayed prediction,
with local presentation paced by the same wall-clock origin as local commands.
Remote world adoption and backwards server time retire stale runtime samples;
a forced provisional-clock rollback restored every entity to the current epoch.

Native server lifecycle checks also found bots waiting for an impossible network
acknowledgement after a map change, loss of bot identity during `map_restart`,
and old saber entity handles surviving a rebuilt entity pool. Bots now begin
immediately on the new map, and transient player state is reset while preserving
session/bot ownership, following multiplayer `SV_SpawnServer`/`ClientConnect`.
External integration checks cover active bot slots, current-pool saber handles
and advancing bot commands after both kinds of transition. No packet codec or
movement/combat rules changed. The external 560-case on-foot reference checks
passed again at 8/7/4/3 ms. Native visual acceptance and wider mod/vehicle coverage
remain open.
The final 960×540 Vulkan native-server run covered eight bots, FFA3 → FFA1,
same-map restart, rapid map changes and cancellation. All 8,650 presented remote
actor endpoints matched their received snapshot positions. The saber returned
about 1.21 seconds after the test press, following focus loss. Local render calls
measured 0.56 ms median and 1.93 ms p99, with a 287.81 ms maximum during background
loading; this does not establish hitch-free transitions or 31-player performance.
Workspace build/tests, formatting, standalone clock checks and release builds
passed. The owner playtested the updated transitions and accepted the combined
preview for publication. Wider mod, vehicle and platform coverage remains open.

Default visual profile (local change based on `a993436`): fresh profiles now
use the selected day/night, volumetric, shadow, HDR, AO and filtering defaults.
An external release/Vulkan check on Linux/RADV RX 9060 XT verified the graphics
values for empty, explicit and existing override configs and rendered FFA5 and
FFA1. The fresh and explicit FFA5 captures matched at 99.94% of pixels, with
mean absolute RGB difference 0.00022 levels and maximum 2/255. Existing saved
values remained authoritative. Formatting, workspace build/tests and the
production release build passed on the combined local changes. Personal configuration is
excluded; see [default visual profile](rendering.md#default-visual-profile).
This is startup/rendering evidence, not a new populated-match performance claim.

## Open validation and limitations

- Complete server/gameplay parity remains unverified. Audit concrete scenarios
  across game types, combat, vehicles, NPCs, scripting and map transitions before
  marking individual capabilities complete.
- The client does not play animation-event sounds from a model's
  `animevents.cfg` (`AEV_SOUND`/`AEV_SOUNDCHAN`, played by codemp
  `CG_PlayerAnimEventDo`). Stock melee punches and saber kicks get their swing
  sounds only from there, so they are silent in JKR. The stun baton, unlike
  melee, has a fire sound in the weapon table of
  [sound_events.rs](../crates/jkr-client/src/sound_events.rs).
- Mod compatibility is scoped by explicit profiles; broad BaseJKA/JA+/TaystJK
  feature parity is not established by profile detection.
- Community PK3 compatibility needs broader map/model coverage. One retail map
  cannot establish every shader, animation or content combination.
- Windows runtime behavior is largely unverified. Do not infer platform support
  from source conditionals alone. Windows reserves 1 MiB for the main thread and
  the client overflowed it after loading `mp/ffa3`; [the viewer build
  script](../crates/jkr-viewer/build.rs) now links Windows binaries with the
  8 MiB Linux size. On Windows 11 (Rust 1.96, MSVC), release and debug clients
  then loaded `mp/ffa3`, and a release client joined a local JKR server and
  completed its map load. Longer play, other maps and the GNU toolchain are unchecked.
- The 500+ FPS / roughly 2 ms frame target is not certified. Measure representative
  release workloads, including populated matches and chosen graphics settings.
- The repository does not bundle a regression suite. Required reference evidence
  must be supplied externally until an in-repository verification approach is agreed.

Sky/terrain correction (2026-10-03, local preview based on `8f692ac`): the three
`t1_danger` marks exposed sky draw-buffer reuse between views and smoothed normals
flipping across visible hills. Both terrain bands are removed in fixed GPU views;
a 24-turn capture reproduced sky disappearance in 12 old-reset frames and verified
the corrected path against direct draws. Workspace checks and owner release build
passed. Native owner playtesting remains pending; see
[rendering](rendering.md#sky-scenery-and-hillside-orientation) for scope and timings.

## Current priorities

1. Evaluate dark-area readability while preserving the accepted lighting style
   (owner priority, 2026-10-03). A local, default-preserving indirect gain and fill
   occlusion experiment passed workspace and external GPU checks; see
   [rendering](rendering.md#indirect-lighting-and-dark-area-readability). Owner
   acceptance and populated-match measurements are pending. The marked custom-map
   ceiling lights were recognized but underpowered; an explicit-glow inference
   refinement passed source-policy and GPU checks on that map plus two stock maps
   ([fixture inference](rendering.md#inferring-fixture-light-from-legacy-materials)).
   That refinement also awaits owner acceptance. Static model fixtures were also
   missing from source extraction: both `t2_rancor` marks now gain local illumination,
   with unchanged stock-map appearance in the sampled `ffa1`/`ffa3` views. The
   Rancor checks add approximately 0.017–0.144 ms of GPU work; see
   [static model fixtures](rendering.md#static-model-fixtures) for evidence and
   remaining visibility limits. Owner acceptance is pending. A subsequent GPU audit
   confirmed a sign error in GI voxel traversal. The corrected forward distances
   and range checks pass 8,302 external GPU/reference cases and workspace checks.
   Both Rancor captures remain byte-identical, so the correction has not solved
   their low visibility. Readback confirms nonzero live probe lighting; receiver
   coverage and effective bounce strength remain to investigate. See
   [GI traversal correction](rendering.md#gi-traversal-correction).
   Dust remains parked.
2. Improve populated-match release frame times while preserving the owner-accepted
   appearance (owner priority, 2026-10-02). Target below 2 ms with 31 players;
   reaching that target is not a reason to stop investigating useful savings.
3. Stabilize normal client and dedicated-server use with reproducible local reports.
4. Audit compatibility gaps by subsystem and scenario; preserve exact combat and wire behavior.
5. Broaden community-content and platform validation.

These priorities guide requested work; they do not authorize an assistant to
start an unrelated task. Update this page when evidence or agreed priorities
change. For a new result, record the source revision, environment, scenario,
reference and limits. Keep resolved change history in Git rather than appending
session-by-session notes here.
