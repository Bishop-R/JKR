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

## Menu style

`ui_menuStyle` (Settings, GAME tab, "Menu style") picks the main-menu layout:
`modern` (default) or `classic`, which is close to the retail multiplayer menus
in layout and flow without porting their `.menu` scripts. The classic main menu
has the retail entries and order: Play, Profile, Controls and Setup in two
columns, Exit below. Play opens the retail "start playing" page (Join Server,
Create Server); Exit and Escape ask before quitting. The retail 640x480 layout is
fitted to the window height and centred. Only JKR's own shapes and text are
drawn, over the dimmed menu map.

Only the main menu has a classic version so far. Its entries open the modern
screens: the server browser, Create game, the Player screen, and Settings on the
CONTROLS tab (Controls) or the VIDEO tab (Setup). The code is in
[menu/classic.rs](../crates/jkr-viewer/src/menu/classic.rs): the pages and entries
are in [layout.rs](../crates/jkr-viewer/src/menu/classic/layout.rs), drawing is
in [view.rs](../crates/jkr-viewer/src/menu/classic/view.rs) and shared exits are
in [destination.rs](../crates/jkr-viewer/src/menu/destination.rs). The style is
read in [style.rs](../crates/jkr-viewer/src/menu/style.rs).

Planned follow-ups, each a new page or screen module beside the main menu,
following the retail `ui/jamp` menus:

- Start playing: Solo Game (`quickgame`), Play Demo (`demo`), Rules (`rules*`).
- Join Server (`joinserver`, `serverinfo`, `findplayer`, `password`,
  `createfavorite`) and Create Server (`createserver`, `advancedcreateserver`).
- Profile (`player`, `player2`, `saber`), Controls (`controls`), and Setup
  (`setup`: video, sound, game options, mods, defaults).
- The in-game menus (`ingame*`, `siege_class`) and the connect and error
  screens (`connect`, `error`).
- Optionally the player's own retail menu artwork (logo, window frames) from
  game data, with the current drawing as fallback. Retail assets are never
  bundled.

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

For graphics controls and diagnostics, see [rendering.md](rendering.md).
