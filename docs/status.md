# Status and priorities

Reviewed 2026-10-04 against GitHub baseline `b394022` and the owner-approved
client, server, rendering and loading changes described below.

JKR currently contains a native client and standard dedicated server in a
20-crate Rust workspace. This page records scope and verification, rather than
claiming complete parity from the presence of an implementation.

## Server shader remaps

Local changes on `dc36792`, verified on Linux/RADV on 2026-10-05, add initial and
live multiplayer shader-state consumption, reliable `remapShader` commands,
demo playback support, destination animation offsets and Tayst-style controls.
`cg_remaps` defaults to 1 (exclude player-texture configstring remaps); 0 disables
server remaps and 2 includes player textures. The setting applies live.
`listRemaps` and temporary local `remapShader` overrides are available.
See [rendering scope and limitations](rendering.md#server-shader-remaps).

External evidence used the unmodified OpenJK multiplayer
`CG_ShaderStateChanged` function: 6,000 valid entries matched the compatibility
parser's result, including extension/case variants, repeated sources, shared
clocks and truncated tails. Separate checks covered one-hop aliases, self-reset,
empty configstrings, nonfinite offsets, policy filtering and gamestate reset.
A 400-snapshot synthetic demo derived from a local recording exercised a direct
remap command followed by reapplication of an unchanged shader-state string.
Both its decoded state and offscreen blue-to-green rendering passed.

An isolated local Tayst server and an original small BSP verified join-time green
replacement, live pulsing material, self-reset to red, enable/disable, translucent
local replacement, and the default/player-inclusive policies (24 player material
slots changed when enabled). Automated checks used an ALSA null sink and isolated
zero-volume settings. Simple remap updates measured 0.5–0.8 ms. One settled
2,048-frame sample at 1280×720 measured 0.419 ms mean and 0.609 ms p99 CPU frame
work; this is a small fixture, not a populated-server benchmark, and excludes cold
pipeline compilation. No wire, movement or combat rules changed. Workspace
formatting, locked build/tests and the optimized Linux viewer build passed.

Shader replacement does not imply geometry editing. Existing server entity and
sub-BSP paths were inspected but not changed. Arbitrary particle/HUD and generated
sprite remaps, full lighting reconstruction and broad custom-map parity remain
outside this implementation; these limitations are recorded on the rendering page.

## Third-person camera collision and vehicle framing

Local change against `dc36792` (2026-10-04): restore the multiplayer camera's
collapsed-target fallback, pitch bounds/offset sign, rapid-turn damping,
vehicle-authored framing and mount/teleport resets. Camera collision now includes
presented inline doors/platforms and excludes BODY from the stock camera mask.
Vehicle definitions are read at appearance loading, with no per-frame file reads.
See [client camera behavior](client.md#third-person-camera).

An external harness compiled the six original OpenJK `codemp/cgame/cg_view.c`
camera functions and compared 10,000 camera frames at 8/7/4/3 ms. Cases include
open space, confined rooms, fully collapsed traces, rapid yaw, animal offsets,
vehicle overrides, pitch-dependent offsets, fighter strafing, unrestricted pitch
and sideways offsets. Maximum component error was 0.000132; this is a floating
point tolerance comparison with shared synthetic trace conditions, not full
client parity certification.

A separately compiled original BSP exercised the production collision adapter:
player-clip obstruction, a translating inline door and ignored packed vehicle
bodies passed. All 8,640 low-ceiling/corner camera frames remained finite. The
optimized camera/collision microbenchmark took 336 ns/frame on this small fixture;
it does not establish populated-map frame performance. Workspace formatting,
build and tests passed, and an optimized Linux viewer was built. Offscreen Vulkan
validation exercised the confined fixture and a mounted stock swoop on an
isolated loopback server. Broader vehicle/mod playtesting remains open; vehicle
rider animation is outside this camera correction. The follow-up below addresses
local vehicle model prediction.

### Local mount presentation follow-up

The local pilot's vehicle model and rider seat now consume its committed/per-frame
vehicle prediction, instead of interpolating old snapshot transforms behind the
predicted camera. This follows OpenJK `codemp/cgame/cg_ents.c`:
`CG_AddPacketEntities` publishes `cg.predictedVehicleState`, and
`CG_CalcEntityLerpPositions` bypasses snapshot interpolation for that vehicle.
No movement, command quantization, animation timing or wire code changed.

An external harness exercised the production vehicle placement/seat module for
8,000 frames at 8/7/4/3 ms, with 50 ms snapshots and 100 ms simulated delay.
The old vehicle-root lag reached 38.735 units; the new root exactly matched the
predicted input. Rider bolt placement, remote/demo fallback and unrelated-vehicle
isolation passed. These are presentation fixtures, not a network or physics
parity certification. Workspace format/build/tests and the Linux release build
passed. An isolated offscreen Vulkan session mounted, moved and turned a stock
tauntaun; a recorded demo confirmed mounted state throughout all 53 snapshots.
Owner testing found continued severe jitter. The follow-up investigation found
that the vehicle's snapshot body remained in its own prediction collision list:
movement started all-solid, then corrected at the next server snapshot. The
adapter now applies vehicle skip/ownership exclusions. Rider-based error decay
also incorrectly treated gait motion as a prediction miss; it now measures the
vehicle root, as stock does. Presentation samples the animated driver seat every
frame instead of holding the snapshot-time offset.

The unmodified OpenJK `CG_VehicleClipCheck` confirmed pilot/own-vehicle exclusion
and foreign-vehicle collision. A production collision-adapter fixture reproduced
all-solid before the fix and clear motion afterward across 4,000 hull sweeps at
8/7/4/3 ms; foreign ownership and dismount restore collision. Another 4,000 frames
using the installed tauntaun's real skeleton reduced the maximum seat step from
5.8814 units (snapshot-held) to 0.9411 (frame-sampled), with no gait-induced
vehicle prediction correction. Seat evaluation measured about 2 microseconds per
frame in the optimized harness; this is not a populated-server performance claim.
In isolated Linux Vulkan play, the same eight-second riding/turning input sequence
went from 19 logged corrections above eight units (maximum 27.78) to none after
the self-collision fix. The final demo confirmed mounting in all 183 snapshots.
Workspace format/build/tests passed. This closes the reproduced self-collision
fault; owner playtesting and broader vehicle/mod coverage remain open.

The exhausted-boost follow-up found another reset: snapshot reseeding copied the
fresh vehicle template over client-only runtime state, losing turbo expiry and
recharge. The same ride now retains that state, as multiplayer cgame retains its
`Vehicle_t`; a changed entity, pilot or definition resets it. An external harness
compiled OpenJK's unmodified `AnimalNPC.c` `ProcessMoveCommands` and compared
27,237 production prediction steps with 50 ms reseeds at 8/7/4/3 ms. Held boost
through expiry/recharge, with alternating primary attack, matched all speed
samples within 0.001 units/second. Separate identity checks verified retention
for the same ride and resets for changed vehicle, pilot and definition.

A silent, headless Linux Vulkan client/server check at 1280x720 and 125 FPS held
boost while turning and attacking for 32 seconds. Logged corrections above eight
units fell from 75 (maximum 17.23) to one (17.10, at a boost transition). Demos
confirmed mounted state in all 696 baseline and 697 corrected snapshots, with
180 boosted snapshots in each. This fixes the repeated exhausted-boost mismatch;
it does not establish zero correction at boost boundaries or full vehicle parity.
Workspace format/build/tests and the owner release build passed. Automated checks
used isolated zero-volume settings and an ALSA null sink from startup.

## Vehicle assets, boarding and native sand-creature AI

Local work on `dc36792`, verified on Linux on 2026-10-05:

- The appearance loader now uses the shared `.veh` parser. A commented example
  in retail `template.veh` previously selected an X-wing for `tie-fighter`.
  Direct lookup and native Vulkan play now load `models/players/tie_fighter`.
- Vehicle weapon indices now resolve `.vwp` EFX and rigid projectile models.
  The local AT-ST firing check produced 243 primary and 86 alternate missile
  samples, selecting `atst/shot_red` and `atst/side_alt_shot` respectively.
  Replaying that capture at 8/7/4/3 ms exercised 6,345 presentation frames with
  correct authored model selection and no default rocket model on laser shots.
  Optimized effect dispatch averaged 66 ns/frame on that small capture; this
  does not establish populated-map performance. Vehicle flight-loop audio is
  not changed in this pass.
- Landing/standing boarding follows multiplayer conditions. The original C
  landing branch matched 20,090 eligibility cases. Production prediction at
  8/7/4/3 ms emitted one authoritative request per landing and none for client
  prediction. A loopback capture confirmed the player boarded the tauntaun by
  landing without a use-key press. Broader mod vehicles remain unverified.
- The reported missing glider/minemonster meshes were absent community content
  in the newer local installation. Mounting the existing pack restored their
  own models. Its malformed glider animation remains a content limitation;
  other actors retain the existing error isolation.
- Native sand-creature AI is enabled only outside stock-rules mode, as described
  in [server.md](server.md#vehicle-boarding-and-sand-creatures). An isolated native
  capture confirmed hidden pursuit, `BOTH_WALK2` breach, both attack animations,
  a normal player death and a successful visible respawn. A second server with
  stock rules enabled retained visible generic NPC behavior throughout all 154
  post-spawn snapshots, with no native ambush. This is a separate
  server extension inspired by SP, not a change to multiplayer class IDs or a
  claim of full single-player AI parity.

Automated native checks used headless Gamescope/Vulkan, isolated settings, all
volumes zero and an ALSA null sink. No public server, owner profile or running
owner game was used for these checks. Formatting, locked workspace build/tests
and optimized client/server builds passed. The workspace has no bundled gameplay
tests; the external checks above provide the focused evidence. Wire encode/decode
paths are unchanged.

## Actor animation error isolation

Local fix based on `3a70c22` (2026-10-04): a custom glider's run clip ends at
frame 235 in a 180-frame skeleton. Its evaluation error previously returned
before the shared joint upload, freezing otherwise healthy actors until the NPC
disappeared. Preparation and upload now contain errors per actor, retain its last
uploaded pose, suppress failed-frame animation audio and log once per failure
episode. Valid animation selection, timing, movement and wire code are unchanged.
The malformed custom clip is still rejected; no content files are modified.

An external headless Vulkan check on the RX 9060 XT loaded the installed glider
and Kyle, exercised failure, recovery and removal, and compared both healthy
actors' uploaded palettes against independently evaluated controls. All 576
comparisons matched exactly at 8/7/4/3 ms with one/four evaluation lanes. The bad
actor kept its previous palette and emitted no active animation-audio request;
only one diagnostic was emitted per failure episode. Native playtesting remains
open. See [rendering](rendering.md#actor-animation-failures).

## Distributable builds

Windows x64 and Linux x64 release ZIPs for merged source `3a70c22` were built
and checked on 2026-10-04 using the
[GameData packages workflow](https://github.com/Bishop-R/JKR/actions/runs/37194839545).
Both native jobs extracted their archives, started/stopped an isolated loopback
dedicated server, and verified client discovery and portable settings with
synthetic assets. The source snapshots match on both platforms; checkout and
archiving preserve embedded shader line endings.

Final inspection checked archive CRCs, binary hashes, source revision and
dependency notices. Linux binaries require at most glibc 2.35; Windows imports
contain only system DLLs, with no separate VC++ or MinGW runtime DLL requirement.
No retail data, personal settings or debug symbols are packaged. These checks do
not cover Windows graphical gameplay. See [packages.md](packages.md) for layout,
runtime requirements and the repeatable build procedure.

### Simplified release archives

The `dc36792` Windows/Linux playtest archives were repacked on 2026-10-04
with exactly four files: the two executables, `README.txt` and `LICENSES.txt`.
Binary bytes and executable permissions are preserved. All 317 Linux and 306
Windows original license/attribution sections are retained in the consolidated
text. Build manifests are separate release assets, covered by the updated
checksums; the source archive is unchanged. GitHub asset digests match the local
archives and manifests.

The extracted Linux archive passed synthetic adjacent-asset discovery, portable
configuration and isolated loopback dedicated-server startup/shutdown. Archive
CRCs, four-file contents, binary hashes and notice preservation passed for both
platforms. The updated dependency collector also matched all 315 entries emitted
by the previous Linux collector. Windows execution was not repeated for this
packaging-only update; its executables are identical to the previous release.

## Drop-in client installation

Local change based on `da8adc9` (2026-10-04): the client discovers game data
beside its executable (or in its `GameData` subdirectory), independently of the
working directory. This takes priority over saved paths; explicit positional
paths and `JKR_GAME_DATA` remain overrides. Direct `--connect HOST:PORT` also
supports discovery. See [client launch](client.md#launch) for the full order.

Linux verification: 19 external startup checks passed with synthetic asset-file
markers, covering unrelated working directories, spaces/non-ASCII paths,
discovery precedence, invalid/incomplete locations, known-install fallbacks and
both direct-connect syntaxes. The unmodified optimized binary, placed beside
synthetic `base/` assets and launched from elsewhere with isolated configuration,
found and attempted to read those archives; it then exited on the intentionally
invalid content before creating a window. No retail files were copied. Formatting,
locked workspace build/tests and the optimized client build passed. Windows
double-click/shortcut behavior and full rendering from a drop-in install have not
been exercised by these checks.

The same local work now defaults generated client files to `GameData/jkr/`.
Storage is selected before the console, browser or HUD is created, so settings,
marks, screenshots, recordings, favorites, friends and identity share one root.
A one-time, non-overwriting import copies supported files from the old user
folder and leaves originals intact. Unwritable installations use the existing
per-user profile. Downloaded content keeps its separate cache.

Eleven external Linux storage scenarios passed using synthetic profiles:
first-run creation, supported-file import and byte preservation, identity-file
permissions, PK3/link exclusion, existing destination conflicts, repeated launches,
real permission-denied fallback, both roots unwritable, obstructing files, failed
import/retry, linked destinations, saved installation hints and same-root aliases.
The checks execute the production storage module in separate processes; they do
not migrate the owner's profile. The unmodified release binary also passed
first-launch, repeat-launch and read-only-installation checks with isolated
synthetic content: imported settings were loaded, autosave used the selected
root, marks/key bytes survived, and originals remained unchanged during portable
launches. Intentionally invalid PK3s stopped these runs before window creation.
Formatting, locked workspace build/tests and the optimized client build passed.
Windows ACLs and native Windows launch remain unverified.

## Console editing and command browser

The console includes the command/cvar browser contributed in PR #6 and the
caret/output-selection controls from PR #33. Browser Apply/Cancel pointer actions
match the keyboard, the footer Filter control works, and underlying menu shapes,
text and FPS output are suppressed while browsing. Printable opening shortcuts
become text when the console is open. Dead-key `^` inserts a literal colour-code
prefix; toggling the console clears pending accent composition so the next
command letter is not changed or swallowed. Escape and non-text toggle bindings
still close it. Gameplay and wire code are unchanged.

Linux verification (2026-10-04, based on `7155455`): 22 temporary checks passed
for caret motion, selection/copying, UTF-8 byte limits, glyph alignment and browser
footer pointer actions at 960×540, 1920×1080 and 3840×2160. Test infrastructure
remains outside the repository. A release X11/Vulkan desktop run exercised
browser opening, search and edit mode. Native keyboard probes reproduced and
corrected the pending-accent problem; the owner confirmed that fix and accepted
the console preview. A separate-profile release run typed a dead-circumflex
followed by `1Bishop` and saved exactly `^1Bishop`, with the console still open.
Formatting, locked workspace build/tests and the release build passed. Clipboard
round trips, drag behavior and platform/layout combinations are not exhaustively
verified.

## Accepted client improvements

The owner approved publishing the current playtest improvements on 2026-10-04.
These changes retain the accepted rendering defaults. Each topic is verified
and published separately; platform/content coverage limits below still apply.

## Current transition policy

Local gameplay continuation during match-end intermission and server map changes
is suspended at the owner's request. Intermission uses the real server's camera,
scores, chat and ready controls; map changes show a loading notice with gameplay
paused. The native continuation adapter and the viewer's dedicated-server
dependency have been removed. Background loading, shared GPU context, archive
inventory optimization, gate-world adoption and matching same-map reuse remain.
Earlier continuation results below describe the historical implementation, not
current enabled behavior. Fast joining and map loading are the current priority;
no universal loading-time target has been verified.

Verification of this policy (local change based on `7155455`): formatting,
locked workspace build/tests and the optimized Linux client build passed. An
external release/Vulkan run against isolated loopback TaystJK exercised a natural
FFA3 timelimit exit into FFA1 and a same-map restart. It observed 1,666 normal
intermission frames and 2,023 loading frames, asserted that no local simulation
started, checked that attempted movement/mouse input could not move the loading
camera, and verified return to the remote session and disconnect to the menu.
Captures confirmed the scoreboard, loading notice and absence of the local body
at the intermission camera. The visibility rule follows codemp `CG_Player`;
scripted NPC and vehicle intermission scenes were not separately exercised.

On RX 9060 XT at 960×540, this final run took about 7.9 seconds from connection
request to playable FFA3 (excluding initial menu construction) and 9.3 seconds
for FFA1 map preparation/adoption, of which 0.52 seconds was CPU map preparation.
These are individual observations, not a controlled speedup or cold-cache result.
Native-window owner playtesting and Windows runtime checks remain pending.

### Loading optimization verification

Local loading changes based on `7155455` plus the suspended-continuation policy
reduce emission-mask preparation, lamp patch searches and serial mip generation.
An interleaved optimized/baseline/optimized Linux release run on RX 9060 XT,
Vulkan, 960×540 and an isolated loopback TaystJK server measured:

| Operation | Baseline | Optimized runs |
| --- | --- | --- |
| Connection request to playable FFA3, including gate animation | 6.82 s | 4.75 / 4.76 s |
| Natural FFA3 → FFA1 map preparation/adoption | 7.88 s | 3.82 / 3.64 s |

Initial menu construction is excluded. These are warm-machine observations from
one host, not cold-cache guarantees or internet-server latency measurements. All
three runs checked ordinary intermission, paused loading, same-map restart,
remote-session adoption and disconnect to the menu.

External reference checks matched all generated lamp-source float bits and
ordering on FFA3 (978 sources), FFA1 (3,562) and `t2_rancor` (4,333). Mip pixels
matched the original algorithm in 12 dimension/layer cases; changed content,
concurrent reuse and byte/entry eviction checks passed. Emission reduction
matched every float bit in 54 rectangular, power-of-two and NPOT cases.
Before/after 1280×720 Vulkan captures retained the scene appearance; animated
materials and temporal rendering mean whole screenshots are not bit-identical.
The isolated checks live outside the source tree and do not add a regression
suite. Gameplay rules, command quantization and protocol encoding are unchanged.
Formatting, locked workspace build/tests and the optimized Linux client build
passed. The owner accepted the faster loading in native playtesting.

## Server content references

Local fix based on `7155455` (2026-10-03): downloading and world content selection
now accept the common prefix of unequal pak-name/checksum lists, matching OpenJK
codemp `FS_PureServerSetReferencedPaks`. Previously both rejected such lists and
prevented joining some servers. External checks compared 441 list-length cases
with the actual OpenJK `1a6a643` C function (whitespace tokenization stubs), and
exercised both production consumers for equal, unequal and absent lists,
installed/duplicate content, malformed checksums, retail-pack exclusions,
unsafe download paths and the reference-count limit. Formatting, locked workspace
build/tests and the optimized Linux client build passed. No wire codec changed.
The owner's EFF retry exposed a second assumption: references were treated as
mandatory archives even with server downloads disabled. The follow-up now skips
UDP transfers when disabled by either side, skips unrequestable/unsafe/retail
download names, and permits absent optional references during mounting. External
checks using the full production storage/selection modules loaded installed FFA1
with missing and malformed references; covered server on/off/absent flags,
client downloads off, non-UTF-8 hostname bytes and a valid community request;
and retained missing-map and wrong-map-checksum rejection. The matching BSP
checksum passed. Workspace checks and the optimized build passed again.
EFF's read-only status advertised stock FFA1 and downloads disabled. Its complete
join with this follow-up remains unverified; no public server was joined for
these checks.

## Chat player menu preview

Local preview `chat5` (2026-10-04) adds a compact square-edged dropdown left of
chat with only `whisper`, `ignore`, `friend`, and `copy`. It follows the clicked
name, stays above typing controls, and leaves chat positions unchanged. At a
narrow left margin it falls back inside the right edge of the chat lane, clear
of the scoreboard. Active ignore/friend toggles are highlighted. There are no
headers, descriptions, standing hints, or success notices; failures still show.
Friends have a small five-point star before the name. Name hover fits the glyph
bounds; each dropdown highlight matches its button rectangle without the wider
menu-row sweep. The dropdown starts without a selected action and switches
cleanly between mouse hover and keyboard focus, so whisper is not permanently
highlighted. See [chat player actions](client.md#chat-player-actions).

Whispers preserve drafts and send only on Enter. Ignores hide messages for the
current map without muting gameplay sounds; friends are saved as local name
bookmarks. Copy preserves name colour codes. Draft editing shares the console's
caret and glyph metrics, including clipboard shortcuts, word motion/deletion,
Shift/mouse selection, double-click token selection and literal dead-key `^`.

Earlier focused checks covered identity reuse, persistence failures, selection,
Unicode, draft limits and pointer actions. An offline X11 probe with an isolated
clipboard adapter verified a `^1Alice^7` clipboard round trip and word selection/
cut. Those checks predate the compact layout; no windows or game instances are
launched to verify this layout revision, per owner preference. Visual playtesting
remains with the owner. Formatting, locked workspace build/tests and the release
build passed.

## Leader HUD placement preview

Local preview `leader1` moves the portrait and leader/opponent name/score from the
old minimum 230-unit vertical offset to a 32-unit top margin. The existing right
margin and sizes remain. Optional inventory/snapshot readouts and the default
team-overlay placement follow below the visible block; explicit team coordinates
are preserved. Server selection, scores, visibility and asset resolution are unchanged.
No windows or game instances are launched for this layout-only revision; visual
playtesting remains with the owner. Formatting, locked workspace build/tests
and the optimized build passed.

## Manual slider entry preview

Local preview `sliders1` (2026-10-04, based on `7155455`) adds direct numeric
entry to every Settings slider, both sabers' RGB sliders, and all Shot sliders.
Click the value or press Enter on its row; Enter applies, Escape cancels.
Bounds are enforced without drag-step quantization. Drafts stay attached to
their original row, and invalid values leave the previous setting intact.

Eleven temporary offline checks passed on Linux, covering actual pointer routing,
all numeric settings/cvar types, all six saber channels, Shot preview actions,
sub-step values, cancellation, bounds, malformed/non-finite input, caret editing,
and value targets at 1280×720, 1920×1080 and 3440×1440. The temporary checks are
not bundled with the source. No windows, game instances or servers were opened;
visual playtesting remains with the owner. Formatting, locked workspace build/tests
and the optimized build passed.

## Client devmap preview

Local `devmap1` preview (2026-10-04, based on `7155455`) exposes `devmap <map>`
in the client console and completion catalogue. It launches a fresh, owned,
loopback-only FFA server with `--cheats`, no bots and no match limits, then uses
the existing automatic join path. Invalid names/missing mounted maps are rejected
before session replacement. Create game keeps cheats disabled. See
[development maps](client.md#development-maps) for current server-command limits.

Four temporary offline/headless checks passed: map-name parsing and usage,
console action handoff, private launch arguments/cheat opt-in, and loading
`mp/ffa3`, joining over loopback and observing `give health 77` in a snapshot.
The reference for devmap cheat policy was OpenJK multiplayer `SV_Map_f`.
No gameplay or protocol codec changes were made. The test child stopped cleanly;
no windows were opened and the owner's running game was untouched. Visual
transition playtesting remains open. Formatting, locked workspace build/tests and
the optimized build passed.

## Noclip and talk balloons preview

Local `playfeatures1` preview (2026-10-04, based on `7155455`) integrates
PR #31 (`60533c8`) and PR #30 (`379aaa2`) into the current client/server sources,
retaining the later upright billboard correction and current UI changes.
Native `noclip` requires cheats and a living player; spawning clears it.
Talk/connection icons use stock priority, placement and visibility rules. See
[development maps](client.md#development-maps), [talk balloons](client.md#talk-balloons)
and [server noclip](server.md#noclip).

On Linux, eleven temporary contributed checks passed for movement, talk flags,
sprite selection and billboard orientation. An external harness compared 640
noclip states against unmodified OpenJK multiplayer movement at 8/7/4/3 ms:
origin, velocity and talk flags matched exactly, including vertical-only input.
Headless loopback checks passed for flying, toggling, respawn reset, normal-server
cheat rejection, and a second client receiving the talk flag and selecting the
balloon. No chat messages were sent. Test servers stopped cleanly; the owner's
running game was untouched. Temporary checks are not bundled in the repository.
Visual owner acceptance and populated-match performance remain unverified.
Formatting, locked workspace build/tests and both optimized binaries passed.


Owner follow-up `bubbleopacity1` fixes status/item icons fading like smoke near
geometry when soft particles are enabled. The new icon instance kind bypasses
only that depth fade; authored texture alpha, depth testing and bounded draw
regions remain intact. An offscreen Vulkan check on the RX 9060 XT compared the
production vertex/fragment shaders at 1/8/32-unit depth gaps with texture alpha
0/0.4/1. All nine icon outputs matched the plain shader byte-for-byte; ordinary
particles still faded at close gaps. No windows were opened. Native visual
confirmation remains with the owner. Formatting, locked workspace build/tests
and the optimized client build passed.


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

Optional dust (`jkr_dust`, default off) is restricted to local godray scattering,
with colour and visibility sampled at each mote's depth. It requires active
volumetrics and follows their shadows and clarity. Linux workspace/release
checks, GPU sampling probes and native HDR/SDR captures passed on 2026-10-02
(Ryzen 5 5500 / RX 9060 XT). The earlier everywhere-dust preview was superseded
after owner feedback. See [rendering](rendering.md) for
current verification and remaining visual/readability limits.

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

Natural intermission and chat (2026-10-03, local changes based on `a993436`):
the previous forced-map checks missed the frozen scoreboard phase at normal
match end. The viewer now starts full local continuation before presenting that
snapshot, while the real remote session retains scores and communication.
A Linux/RADV 960×540 release check against an isolated loopback TaystJK server
expired its timelimit, displayed authoritative scores over the local character,
used the stock ready button and followed `nextmap` from FFA3 to FFA1. The final
run observed 1,404 scoreboard frames and 2,450 loading frames with exactly one
local actor and continuing movement, then adopted the destination. Local render
calls measured 0.92 ms median, 2.37 ms p99 and 309.76 ms maximum; an earlier run
under concurrent build pressure reached 3.11 seconds. This is not hitch-free or
a populated-match performance certification.

Socket-free mock endpoints verified global/team/private composer dispatch and
console chat routing without sending test messages to any server. Incoming
server announcements continued during intermission. They now go to the console
exclusively: the owner requested a general separation of console prints from
chat after observing TaystJK's `PrintStats` table in the conversation overlay.
Global/team/private chat and separate center-print HUD notices are preserved;
no server-specific table filter is used. An external offline check confirmed
that 101 console prints, including a stats table, could not enter or displace
chat history or close its composer; chat/team messages and a center notice still
reached their intended presentation. Workspace build/tests, formatting and the
updated production release build passed. This routing check sent no messages
to a server. Captures cover the composer
beside real scores and a synthetic 32-player team board; 576 geometry cases cover
1–32 rows, FFA/team layouts and nine viewports from 640×480 through 4K, including
ultrawide and portrait. These checks do not establish human-to-human delivery or
all mod/platform behavior. Client changes leave gameplay rules and wire codecs
untouched. External compiled OpenJK on-foot/force-jump checks passed again at
8/7/4/3 ms (560 cases / 72,275 commands). Workspace build/tests, formatting and
the production release build passed. The owner accepted the combined preview
for publication; broader mod and platform coverage remains open.

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

Volumetric silhouette correction (local changes based on `a993436`): excluded
depth samples no longer dilute the visible-air lighting estimate. External
Linux/RADV release checks reproduced and removed the sampled FFA5 player fringe
without increasing grid resolution. Paired 720p/4K timings showed no material
change in the tested scene; captures also cover FFA3, a Rancor interior and 24
camera turns. See [volumetric coverage](rendering.md#volumetric-silhouette-coverage)
for measurements and reproduction limits. Formatting, workspace build/tests and
the production release build passed. Owner acceptance and broad content coverage
remain open.

- Complete server/gameplay parity remains unverified. Audit concrete scenarios
  across game types, combat, vehicles, NPCs, scripting and map transitions before
  marking individual capabilities complete.
- Model animation sounds now follow `animevents.cfg` frames. Local release checks
  against OpenJK `3e465e7c`'s extracted `CG_PlayerAnimEvents` predicate matched
  238,328 frame-crossing cases. Walk/run and blue-style gesture cues were stable
  at 8/7/4/3 ms steps; a six-second gesture produced its ten authored spin cues
  at every cap, and held frames did not replay them. Include overrides, material
  selection and missing voice-family fallbacks passed external checks. A
  32-actor cursor-only microbenchmark averaged 0.44 microseconds per iteration
  with zero measured heap allocations; this excludes bone queries, collision
  traces, mixing and rendering.
  An isolated loopback TaystJK `5802c99` run produced stone/metal running steps
  and blue-taunt spin cues through a real decoder and null-output mixer, with
  zero decode failures or missing handles. Authored custom saber sound fields
  also passed an external parsing check. Formatting, locked workspace build/tests
  and the optimized Linux client build passed. Native owner listening, broader
  custom-model coverage and animation effect/footprint rendering remain open.
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
