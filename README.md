# JKR

JKR is a Rust engine for **Star Wars Jedi Knight: Jedi Academy** multiplayer.
It includes a graphical client and a dedicated server, with support for the
legacy protocol, PK3 content, maps, models, movement and combat.

The renderer uses wgpu. The client includes a server browser, console,
configurable controls, HUD, audio, screenshots and demo playback.

## Build

Install Rust 1.88 or newer, Cargo, a C/C++ compiler, CMake and pkg-config.
On Linux, development packages for ALSA, Wayland and XKB are required, along
with working Vulkan or OpenGL graphics drivers.

```sh
cargo build --release -p jkr-viewer -p jkr-dedicated
```

Linux is the currently verified platform. The code also includes Windows
platform support; Windows runtime behavior has not yet been verified.

## Game data

A legally obtained Jedi Academy installation is required. Point JKR at its
`GameData` directory, containing `base/assets0.pk3` and the other retail PK3s.
Game data is not included in this repository.

## Client

```sh
./target/release/jkr-viewer /path/to/GameData
```

Connect directly to a server:

```sh
./target/release/jkr-viewer /path/to/GameData --connect 127.0.0.1:29070
```

The client also supports launch without arguments when it can locate an
installation or has a saved game-data path. Use the in-game menus for controls,
graphics, audio and player settings.

## Dedicated server

```sh
./target/release/jkr-dedicated \
  --game-data /path/to/GameData \
  --map mp/ffa3 \
  --bind 0.0.0.0:29070 \
  --hostname "JKR server"
```

Server configuration uses familiar cvars and commands, including `+set` launch
arguments. For example, append `+set g_gametype 0 +set fraglimit 20` for a
free-for-all match. UDP port 29070 must be reachable for remote players to join.

## Source layout

All source is under `crates/`:

- `jkr-viewer`: graphical client and platform integration.
- `jkr-dedicated`: dedicated server and game integration.
- `jkr-game-jka`: shared Jedi Academy game rules and movement.
- `jkr-client`, `jkr-network`, `jkr-protocol`: client state and legacy networking.
- `jkr-bsp`, `jkr-scene`, `jkr-runtime`: maps, scene data and world state.
- `jkr-materialgen`: offline generator of local material maps from installed
  textures (see [rendering](docs/rendering.md#generating-material-maps)).
- Remaining crates provide formats, content loading, collision, navigation,
  scripting, audio, UI and the console.

Legacy formats and game-specific behavior stay in compatibility modules;
engine services use their own data structures.

## Documentation

The [project wiki](docs/README.md) covers architecture, development, the client,
the dedicated server, rendering and networking. Start with
[current status and priorities](docs/status.md) for verified behavior and open work.
[AGENTS.md](AGENTS.md) contains shared contributor and AI guidance, including
updating relevant documentation alongside code changes.

## License

GPL-2.0-only; see [LICENSE](LICENSE). OpenJK and TaystJK are compatibility
references for Jedi Academy behavior. The bundled Inter fonts retain their
[license](crates/jkr-viewer/assets/fonts/LICENSE.txt). Other dependencies retain
their respective licenses. The code license does not grant rights to retail
game assets or third-party PK3 content.
