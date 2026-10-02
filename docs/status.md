# Status and priorities

Reviewed 2026-10-01 against source baseline `0d215f1`.

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

## Open validation and limitations

- Complete server/gameplay parity remains unverified. Audit concrete scenarios
  across game types, combat, vehicles, NPCs, scripting and map transitions before
  marking individual capabilities complete.
- The client plays the sound events of a skeleton's `animevents.cfg`
  (`AEV_SOUND`/`AEV_SOUNDCHAN`, and the saber swing and spin sounds codemp makes
  of `saberhup`/`saberspin` lines) as actors' legs and torso reach their frames:
  [animation_events.rs](../crates/jkr-client/src/animation_events.rs), played by
  [audio_animation_events.rs](../crates/jkr-viewer/src/audio_animation_events.rs).
  These are the fast-style taunt's saber spins, saber kicks and katas, melee
  punches and body falls. Covered by unit tests and a parse of the retail
  humanoid file; not yet heard in game. Not played from the same file:
  `AEV_FOOTSTEP` (dry footsteps), `AEV_EFFECT`, `AEV_FIRE`, a saber's own
  `swingSound`/`spinSound`, and corpses' events. The stun baton, unlike melee,
  has a fire sound in the weapon table of
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

## Current priorities

1. Stabilize normal client and dedicated-server use with reproducible local reports.
2. Audit compatibility gaps by subsystem and scenario; preserve exact combat and wire behavior.
3. Measure and improve release frame times while retaining presentation correctness.
4. Broaden community-content and platform validation.

These priorities guide requested work; they do not authorize an assistant to
start an unrelated task. Update this page when evidence or agreed priorities
change. For a new result, record the source revision, environment, scenario,
reference and limits. Keep resolved change history in Git rather than appending
session-by-session notes here.
