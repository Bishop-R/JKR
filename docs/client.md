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

`ui_menuStyle` (Settings, GAME tab, "Menu style") picks the layout of the main
and in-game menus: `modern` (default) or `classic`, which is close to the retail
multiplayer menus in layout and flow without porting their `.menu` scripts. The
retail 640x480 layout is fitted to the window height and centred.

The classic main menu has the retail pages, entries and order:

- Main: Play, Profile, Controls and Setup in two columns, Exit below. Exit and
  Escape ask before quitting.
- Play: Solo Game, Join Server, Create Server, Play Demo and Rules. Solo Game
  and Create Server both open Create game, which hosts a local match with bots.
- Controls: Movement, Interaction, Weapons, Force Powers 1 and 2 and Other open
  the key-binding editor on that tab (both Force pages on its one Force tab);
  Mouse/Joystick opens Settings on the CONTROLS tab. The editor opened this way
  closes back to the classic page.
- Setup: Video and More Video open Settings on VIDEO, Sound on AUDIO and Game
  Options on GAME; HUD and Network follow as JKR additions.
- Every sub-page repeats the retail navigation row (Play, Profile, Controls,
  Setup) and has Back and Exit. Profile opens the Player screen.

Retail entries JKR has no screen for yet (Play Demo, Rules, Mods, Defaults) are
shown dimmed, and their description line says so.

The classic in-game menu (Escape during a match) is the retail top bar: About,
Join, Profile, Add Bot, Controls, Setup, Vote, Call Vote and Exit. Each opens a
pop-up under it or the matching screen. About shows the server info. Join picks
a team, or opens the class list in Siege. Vote is Yes/No. Call Vote opens the
call-vote lists. Exit offers Main Menu, Restart Match and Quit Program, each
with a Yes/No confirmation. Profile, Controls and Setup open the Player screen
and Settings. Siege swaps in Objectives and V Chat as retail does. Add Bot,
Objectives, V Chat and Restart Match are dimmed with a note, because the client
cannot add bots or restart a match it does not host. Left and Right move along
the bar; Escape closes a pop-up, then the menu. The JKR-only Server browser and
Shot controls entries are in the modern style only.

With the player's retail game data mounted, the classic menus draw its own
artwork: the backdrop, side glyph columns, ring, windows, logo, sub-page frames,
button glow, in-game bar and pop-up boxes from `gfx/menus`. The art is decoded
once on a worker thread the first time the classic style is used, and the UI
renderer uploads it into one texture per image, separate from the shared UI icon
atlas. Its bind group changes only between draw runs that need a different
texture, so layer order is kept. Additively blended retail images (glow, title
band, bar) are converted to alpha at decode time. Animated retail stages (ring
rotation, scrolling glyphs, logo glint, the logo video) are drawn still. A
missing image falls back to JKR's own shapes. Retail assets are never bundled.

The code is in [menu/classic.rs](../crates/jkr-viewer/src/menu/classic.rs): the
page tables are in [pages.rs](../crates/jkr-viewer/src/menu/classic/pages.rs),
types and geometry in [layout.rs](../crates/jkr-viewer/src/menu/classic/layout.rs)
and drawing in [view.rs](../crates/jkr-viewer/src/menu/classic/view.rs). The
in-game version is in
[ingame_menu/classic.rs](../crates/jkr-viewer/src/ingame_menu/classic.rs), with
[classic_view.rs](../crates/jkr-viewer/src/ingame_menu/classic_view.rs) and
[classic_actions.rs](../crates/jkr-viewer/src/ingame_menu/classic_actions.rs).
The artwork is loaded in [menu/art.rs](../crates/jkr-viewer/src/menu/art.rs) and
bound in [ui_renderer/art.rs](../crates/jkr-viewer/src/ui_renderer/art.rs). The
style is read in [style.rs](../crates/jkr-viewer/src/menu/style.rs). Shared exits
are in [destination.rs](../crates/jkr-viewer/src/menu/destination.rs).

Planned follow-ups, each a new page or screen module, following the retail
`ui/jamp` menus:

- Classic versions of the screens the classic pages still open in the modern
  style: Join Server (`joinserver`, `serverinfo`, `findplayer`, `password`,
  `createfavorite`), Create Server (`createserver`, `advancedcreateserver`),
  Solo Game (`quickgame`), Profile (`player`, `player2`, `saber`), the
  controls and setup option panels, and the in-game `ingame_player`,
  `ingame_controls` and `ingame_setup`.
- The screens with no JKR equivalent yet: Play Demo (`demo`), Rules
  (`rules*`), Mods, Defaults, Add Bot (`ingame_addbot`), Siege objectives and
  voice chat, and the connect and error screens (`connect`, `error`).
- The retail fonts (`ui_gameFont`, a separate change) and the animated art
  stages.

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
