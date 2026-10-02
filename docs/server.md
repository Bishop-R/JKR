# Dedicated server

`jkr-dedicated` is JKR's native headless server. It does not launch another engine.
Build it alongside the client as described in [development.md](development.md).

## Local game

Use an unused loopback port for development:

```sh
./target/release/jkr-dedicated \
  --game-data /path/to/GameData \
  --map mp/ffa3 \
  --bind 127.0.0.1:29071 \
  --hostname "JKR local" \
  --gametype ffa \
  --team-auto-join \
  +set dedicated 1
```

Then connect the client to `127.0.0.1:29071`. `dedicated 1` selects LAN operation
without master-server advertising. Keep automated checks local and free of human
players. For remote hosting, explicitly choose an externally reachable bind
address and open the chosen UDP port.

Always supply `--game-data` for a playable map. Without it the process can
advertise a map name but does not load that map's content.

## Options and configuration

The authoritative option parser and usage text are in
[main.rs](../crates/jkr-dedicated/src/main.rs).

| Option | Meaning |
| --- | --- |
| `--map NAME` | Initial map, such as `mp/ffa3` |
| `--maps NAME,NAME` | Rotating list of subsequent maps |
| `--gametype NAME` | `ffa`, `holocron`, `jm`, `duel`, `powerduel`, `team`, `siege`, `ctf`, `cty` |
| `--fraglimit N`, `--timelimit MINUTES` | Match limits |
| `--wire-clients N` | Protocol-26 client slots, 0–32 |
| `--peers N` | Native player capacity, separate from wire slots |
| `--team-auto-join` | Place newcomers in play without the initial spectator selection |
| `--home DIRECTORY` | Writable server config home |
| `--set NAME VALUE` | Set a console variable |
| `--quit-on-eof` | Stop when a supervising parent's stdin pipe closes |
| `--cheats` | Turn on `sv_cheats` so the cheat commands below work |

These are implemented option surfaces, not a guarantee of complete parity for
every game type or map. See [status.md](status.md).

`+set` and other `+COMMAND` arguments are supported. Startup applies `+set`
values before loading `mpdefault.cfg`, `jkr_server.cfg` and `autoexec.cfg`, then
runs the command-line `+` commands in order. With `--home`, `exec` checks its
`base` directory first and archived variables are saved in `base/jkr_server.cfg`.
Without a home, this archive is not written.

Type commands such as `status`, `map mp/ffa3` and `quit` on stdin. Remote console
commands require an explicitly configured `rconpassword`; never commit that
password. The console implementation lives in
[bridge_console.rs](../crates/jkr-dedicated/src/bridge_console.rs) and the server's
[command buffer](../crates/jkr-dedicated/src/command_buffer.rs).

The [networking page](networking.md) explains the boundary between native entity
ownership, game behavior and the legacy endpoint.

## Cheat commands

With `sv_cheats` on (`--cheats`, or `devmap` instead of `map` on the server
console), players can use the cheat commands `give`, `t_use`, `setviewpos` and
`noclip` from the client console; otherwise the server answers with the stock
"cheats are not enabled" message. Except `setviewpos`, they also require a
living player in the game.

`noclip` toggles flying through the world as in OpenJK (`Cmd_Noclip_f`). The
player moves in `PM_NOCLIP`, which the client predicts with the same shared
movement code. It does not touch triggers or items, does not drown and takes no
damage. Respawning or changing team turns it off, and it is refused during the
intermission. See [bridge_cheats.rs](../crates/jkr-dedicated/src/bridge_cheats.rs)
and [noclip.rs](../crates/jkr-game-jka/src/noclip.rs).
