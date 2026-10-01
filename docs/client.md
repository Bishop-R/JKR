# Client

Build instructions are in [development.md](development.md). JKR needs an installed
Jedi Academy `GameData` directory with the retail `base/assets*.pk3` files.

## Launch

Open the main menu using an explicit content environment:

```sh
JKR_GAME_DATA=/path/to/GameData ./target/release/jkr-viewer
```

Join a server directly:

```sh
./target/release/jkr-viewer /path/to/GameData --connect 127.0.0.1:29071
```

The no-argument launch also checks the saved game-data location and known install
locations. Positional launch accepts a map path and optional player-model directory;
it is a direct world/viewer launch, whereas no arguments opens the main menu.
See [launch.rs](../crates/jkr-viewer/src/launch.rs) and
[app_launch.rs](../crates/jkr-viewer/src/app_launch.rs).

The client has a server browser, player and settings screens, in-game menus,
console, HUD, screenshots, demo recording/playback and a Create game flow.
Presence here describes implemented surfaces; validation limits are in
[status.md](status.md).

Create game starts a child `jkr-dedicated`, normally found beside the client.
Set `JKR_DEDICATED` to its executable path if installed elsewhere. The child
lifetime is managed by the client and defaults to local access; see
[local_server.rs](../crates/jkr-viewer/src/local_server.rs).

## Configuration and content

The Linux configuration is `$XDG_CONFIG_HOME/jkr/config.cfg`, falling back to
`~/.config/jkr/config.cfg`. The source also defines Windows `%APPDATA%` and macOS
Application Support locations in [platform.rs](../crates/jkr-viewer/src/platform.rs).
Edit settings through the client, or edit the file while the client is stopped
so autosaving cannot overwrite your changes.

`fs_game`, `fs_basegame` and `fs_homepath` configure content search paths; restart
the client after changing them. Search precedence and shader protection are owned
by [asset_search_paths.rs](../crates/jkr-viewer/src/asset_search_paths.rs).

Downloaded content is stored separately from the retail installation and config.
On Linux the default is `$XDG_DATA_HOME/jkr/downloads/base`, falling back to
`~/.local/share/jkr/downloads/base`. `JKR_DOWNLOAD_HOME` overrides the download
root (the implementation appends `base`). See
[download_store.rs](../crates/jkr-viewer/src/download_store.rs).

## Useful console commands

`connect host:port`, `disconnect` and `reconnect` control the session.
`record`, `stoprecord`, `demo` and `playdemo` control demos.
`screenshot` and `screenshotJPEG` request captures; `condump filename` saves
console output. See [console registration](../crates/jkr-viewer/src/console_session.rs)
and [file commands](../crates/jkr-viewer/src/console_files.rs) for argument handling.

Tab completes the command or cvar name being typed, after a leading `/` or `\` and
after the last `;`. A unique name completes with a trailing space; otherwise the
input extends to the longest shared prefix and the matching commands and cvars,
with cvar values, are listed. Up to 16 matches also show their descriptions;
longer listings end with the match count instead. Enter strips one leading `/` or
`\` from the line, then applies the same completion while `cl_allowEnterCompletion`
is set, without listing when the input is already a full name. Nothing strips a
slash on a command after `;`, so completing that command drops it. Command
boundaries follow the shell's quote and escape rules, and no completion happens
inside an open quote. Arguments are not completed. See
[shell_completion.rs](../crates/jkr-shell/src/shell_completion.rs).

F3 in the open console, or the bindable `consolebrowser` command, opens a browser of
every command and cvar with its description, and each cvar's value and default. Typing
searches names, then descriptions; Tab cycles All, Commands, Cvars and Changed (cvars
away from their default). Enter edits the selected cvar in place and applies it, or
starts a console line with the selected command; Delete restores a cvar's default;
Escape or F3 returns to the console. Read-only cvars are listed but not edited. See
[console_browser.rs](../crates/jkr-viewer/src/console_browser.rs).

For graphics controls and diagnostics, see [rendering.md](rendering.md).
