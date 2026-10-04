# Architecture

JKR implements the client and server as native Rust programs. The current game
is Jedi Academy multiplayer; engine services use owned worlds and explicit
interfaces rather than a process-wide legacy "current map". JKR implements game
rules itself rather than hosting the original game DLLs.

## Boundaries

1. Authoritative simulation and client presentation have different owners and
   lifetimes. A rendered world can remain resident during a network transition.
2. Protocol 26 is an adapter. Wire client/entity numbers, configstrings and
   fixed packet limits must not become universal engine identities or capacities.
3. JKA rules and format constraints belong in compatibility crates. The generic
   server owns worlds and entity handles; the dedicated bridge supplies gameplay
   and maps those handles to the legacy endpoint.
4. Content paths are normalized, case-insensitive virtual paths. Asset ownership
   is explicit; loading another map must not silently replace global asset state.
5. Platform code and GPU resources stay at application/integration boundaries.
   UI layout and audio mixing have independent engine interfaces.
6. Downloaded content is handled through bounded storage operations. Remote paths
   must not become unrestricted local filesystem paths.

These are maintenance constraints. Existing compatibility dependencies do not
justify spreading JKA-specific constants into unrelated engine services.

## Resident client worlds

The viewer separates its displayed world from a connection waiting for a map.
`session_transition::resident::State` parks that transport outside `live_session`,
so snapshot consumers cannot render new-map entities against the retained BSP.
For a departed live world, `resident_game` owns an in-process `NativeGame` from
`jkr-dedicated`. Its socket-free `ClientSession` uses `LocalSimulation` and the
same game-host snapshot projection as the network server. Normal prediction,
actor presentation, HUD and effects consume these snapshots. Reusable projection
storage is allocated at activation. Player skeletons are precached during
background world preparation, and a same-map reattachment reclaims the native
game for reuse. The pre-gamestate gate path still uses the movement-only predictor.

The local command clock advances monotonically without network drift correction.
The parked remote transport sends neutral commands on a separate timer, and
continues receiving lifecycle events and communication. At match-end intermission,
local gameplay starts before the frozen snapshot reaches presentation. Scores and
chat retain the remote endpoint; only stock ready-to-exit attack/use buttons use
the remote snapshot clock until its map change, after which commands are neutral.
Only that transport owns its socket; local movement, aim and simulation state
never cross into it. A retired gamestate/snapshot is
captured once at transition, while the resident world retains the latest playable
player state for intermission recovery. Reattachment invalidates presentation
configstrings so native resource indices cannot leak into remote presentation.
Wire codecs remain unchanged. Remote hidden game state is unavailable; see the
[continuation limits](client.md#joining-and-changing-maps).

CPU preparation and GPU installation own immutable destination inputs. Superseding
transitions discard their channels; GPU construction checks cancellation between
build stages. Only a completed world is adopted. A prepared gamestate's content
selection must match before attaching a session without rebuilding, and a restart
must receive a fresh snapshot before attachment. The FFA3 gate hands over its
actual prepared world rather than constructing a duplicate at entry. The parked
menu world remains separately owned for cancellation/disconnection.

PK3 checksum inventory uses the validated ZIP central directory, retaining archive
entry order, CRCs and zero-length filtering. It does not visit/decompress every
payload during connection; normal asset reads remain responsible for payload and
local-header validation. See [pk3_fingerprint.rs](../crates/jkr-vfs/src/pk3_fingerprint.rs).

## Source map

All 20 workspace crates are listed in [Cargo.toml](../Cargo.toml).

| Crate | Responsibility |
| --- | --- |
| [jkr-viewer](../crates/jkr-viewer/src/main.rs) | Client executable, GPU, window/input, menus and integration |
| [jkr-dedicated](../crates/jkr-dedicated/src/main.rs) | Server executable, operator console and game bridge |
| [jkr-server](../crates/jkr-server/src/lib.rs) | Generic authoritative world/entity ownership |
| [jkr-runtime](../crates/jkr-runtime/src/lib.rs) | Engine-native world and presentation state |
| [jkr-game-jka](../crates/jkr-game-jka/src/lib.rs) | JKA movement, combat and game behavior |
| [jkr-client](../crates/jkr-client/src/lib.rs) | Client sessions, prediction and snapshot presentation data |
| [jkr-network](../crates/jkr-network/src/lib.rs) | Transport, discovery and legacy client/server sessions |
| [jkr-protocol](../crates/jkr-protocol/src/lib.rs) | Wire codecs and compatibility data, without sockets |
| [jkr-vfs](../crates/jkr-vfs/src/lib.rs) | Loose-file and PK3 virtual filesystem |
| [jkr-bsp](../crates/jkr-bsp/src/lib.rs) | Owned RBSP map data and collision queries |
| [jkr-scene](../crates/jkr-scene/src/lib.rs) | Renderer-neutral scene construction |
| [jkr-model](../crates/jkr-model/src/lib.rs) | MD3 and Ghoul2 model/animation data |
| [jkr-shader](../crates/jkr-shader/src/lib.rs) | Legacy shader-script parsing and resolution |
| [jkr-entity](../crates/jkr-entity/src/lib.rs) | Map entity dictionaries |
| [jkr-effect](../crates/jkr-effect/src/lib.rs) | Raven effect definitions |
| [jkr-nav](../crates/jkr-nav/src/lib.rs) | Navigation graphs and queries with game-supplied world access |
| [jkr-icarus](../crates/jkr-icarus/src/lib.rs) | Script interpretation with host-provided game operations |
| [jkr-audio](../crates/jkr-audio/src/lib.rs) | Sound storage, spatialization and mixing |
| [jkr-ui](../crates/jkr-ui/src/lib.rs) | Retained widgets, layout, input and draw commands |
| [jkr-shell](../crates/jkr-shell/src/lib.rs) | Cvars, bindings and command processing |

## Main flows

Client: network session → decoded snapshots → client/game compatibility →
owned presentation state → viewer rendering, UI and audio. Local prediction uses
shared movement rules; it does not replace server authority.

Server: UDP → legacy endpoint →
[game bridge](../crates/jkr-dedicated/src/bridge.rs) → game simulation and native
world storage → per-client legacy replication.

Assets: VFS → compatibility parsers → owned map/model/shader data → scene and
application resources. BSP geometry remains the collision input for JKA maps.
