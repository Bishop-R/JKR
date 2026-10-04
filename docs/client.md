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

## Joining and changing maps

The menu's FFA3 gate opens onto the prepared destination world. Map preparation
and connection run independently: when the world is ready first, you can walk,
jump and crouch locally while the connection finishes. A matching verified world
is reused when the server session becomes ready; joining does not build it twice.
The opening and crossing animation takes about 2.1 seconds once the destination
is ready. The gate stays closed while required content is unavailable. The
existing connection notice shows the server address/status and Cancel action
over the gate during joining; it disappears when the destination is entered.

On a server map change, other players disappear while your player continues in
an in-process native game on the old map. The ordinary client pipeline still
presents your model, saber, weapons, HUD, animations and effects. Movement,
attacks, pickups and doors use the existing dedicated-server gameplay. The last
predicted player state seeds the continuation. At normal match end, local play
starts before the first frozen intermission snapshot is presented, preserving the
pre-intermission character and camera. The real server's scoreboard remains
visible over local play and disappears when the server changes maps; your player
continues until the destination is ready. Visible brush-door trajectories are
carried across.
The handoff retains the local actor's animation tracks and uses one monotonic
local command/presentation timeline, without network drift adjustments. Remote
adoption starts fresh entity samples; a backwards server timestamp also retires
samples from the provisional server clock before applying the new snapshot.

The remote connection has its own command timer and stays separate from this
local authority. Local movement, aim and simulation time never go to the waiting
server. During intermission only, attack/use buttons retain the stock ready-to-exit
behavior using the remote snapshot clock; once loading begins, commands are neutral.
Once the destination and a fresh, active remote snapshot are ready, the client adopts
that world and the server's spawn state. Matching same-map restarts can reuse the
resident world. These paths have no separate loading screen.

This is a temporary local game, not a copy of a remote mod's hidden state. It
uses standard native game rules. Unreplicated script state, entity timers,
remote projectiles and other players are not imported; unseen map entities start
from their authored state, and an already-thrown saber returns to the hand at
handoff. Vehicles and custom scripted interactions need further verification.
Demo recording is unavailable during local continuation. First entry through
the gate before any server player exists still uses movement-only exploration
with frozen brush collision. If native world preparation fails, map-change
continuation also falls back to that path and reports the error in the console.
The gate's through-door view still uses the existing lightweight rendering path;
the full lighting path begins when the destination becomes the active world.

Server console output (`print`) goes exclusively to the console, including match
statistics, command replies and server announcements. It never enters chat history.
Global, team and private chat retain their conversation overlay. Center-print
gameplay notices keep their separate HUD presentation. The scoreboard continues
to use structured server scores rather than parsing printed statistics tables.

Chat remains connected to the real server during intermission and background
loading, including global/team/private composer messages and console/bound chat
commands. The scoreboard reserves a separate left column for messages and the
composer, temporarily overriding chat position/width while scores are visible.
Chat visibility/lifetime settings still apply. Typing captures gameplay input as
usual, and the ordinary chat layout returns after the scoreboard closes.
If local authority cannot be prepared, intermission keeps the standard remote
scoreboard/camera rather than switching to movement-only exploration.

Escape opens the normal menu during exploration. Disconnect/cancel abandons the
pending connection and restores the retained main-menu world. Connection and asset
errors still show an error message. Downloads retain their existing policy and
limits; progress is available in the console. This does not eliminate disk,
shader, network or download latency, and direct command-line startup is separate
from the already-open menu's transition path.

Implementation: [resident worlds](../crates/jkr-viewer/src/resident_world.rs),
[local authority](../crates/jkr-viewer/src/resident_game.rs),
[early exploration](../crates/jkr-viewer/src/resident_walk.rs),
[world handoff](../crates/jkr-viewer/src/session_transition.rs) and
[gate destination](../crates/jkr-viewer/src/portal.rs).

Losing window focus releases held gameplay controls and discards gameplay actions
already queued for that frame. Synthetic key events on refocus cannot re-press a
held modifier such as Alt. This allows a saber throw already sent to the server
to finish normally after Alt+Tab.

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

Printable console shortcuts open the console but type normally once it is open;
Escape and non-text toggle bindings can still close it. `^` is a literal colour
prefix in the console and its browser, including on layouts that report it as a
dead key, so `set name "^1Bishop"` does not close the console or lose the digit.
Console transitions and literal dead-key `^` input clear the window's pending
accent composition. Other dead keys retain normal accent composition.

`connect host:port`, `disconnect` and `reconnect` control the session.
`record`, `stoprecord`, `demo` and `playdemo` control demos.
`screenshot` and `screenshotJPEG` request captures; `condump filename` saves
console output. See [console registration](../crates/jkr-viewer/src/console_session.rs)
and [file commands](../crates/jkr-viewer/src/console_files.rs) for argument handling.

The console input line has a caret, drawn as stock's underscore: Left and Right
move it, Ctrl+Left and Ctrl+Right by word, Home and End to either end, and Shift
with any of them selects. Backspace and Delete remove a character, or a word with
Ctrl; words end at anything but a letter or digit, so `cg_drawFPS` and
`127.0.0.1` are edited a piece at a time. Ctrl+A selects the line, Ctrl+X cuts,
and Ctrl+V or Shift+Insert pastes, replacing a selection. Typing inserts at the
caret, a long line scrolls sideways to keep it in view, and Enter runs the whole
line. Dragging the mouse over console output selects it, a double click selects
one whitespace-separated word (a whole `host:port`), Shift+click extends a
selection and a click elsewhere clears it; in the input line the mouse places the
caret and selects the same way. Ctrl+C (or Ctrl+Insert) copies selected output
without colour codes, else the selected input, else the whole input line, or the
last `viewpos` or `mark` answer when the line is empty. Up and Down stay history.
See [console_editing.rs](../crates/jkr-viewer/src/console_editing.rs) and
[console_selection.rs](../crates/jkr-viewer/src/console_selection.rs). The input
line and output rows are drawn and measured through one
[ConsoleText](../crates/jkr-viewer/src/console_text.rs) per text size, so the
caret, highlights and mouse hits follow the drawn glyphs at any size or letter
spacing.

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
Escape cancels an active edit first; otherwise Escape or F3 returns to the console.
The clickable Apply, Cancel and Filter controls follow the same actions as the
keyboard. Read-only cvars are listed but not edited. The
browser covers the whole frame: underlying menu shapes/text, chat and the FPS
counter are suppressed, including both font batches. See
[console_browser.rs](../crates/jkr-viewer/src/console_browser.rs).

For graphics controls and diagnostics, see [rendering.md](rendering.md).
