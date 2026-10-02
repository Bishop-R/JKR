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

## Source map

All 21 workspace crates are listed in [Cargo.toml](../Cargo.toml).

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
| [jkr-materialgen](../crates/jkr-materialgen/src/lib.rs) | Offline tool: local material maps from installed textures |

## Main flows

Client: network session → decoded snapshots → client/game compatibility →
owned presentation state → viewer rendering, UI and audio. Local prediction uses
shared movement rules; it does not replace server authority.

Server: UDP → legacy endpoint →
[game bridge](../crates/jkr-dedicated/src/bridge.rs) → game simulation and native
world storage → per-client legacy replication.

Assets: VFS → compatibility parsers → owned map/model/shader data → scene and
application resources. BSP geometry remains the collision input for JKA maps.
