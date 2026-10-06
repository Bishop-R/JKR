# Client

Build instructions are in [development.md](development.md). JKR needs an installed
Jedi Academy `GameData` directory with the retail `base/assets*.pk3` files.

## Launch

Put `jkr-viewer` (`jkr-viewer.exe` on Windows) inside the installed game's
`GameData` folder, beside `base/`, then launch it to open the main menu. A shortcut
can use any working directory. No path settings are required. Keep
`jkr-dedicated` (`jkr-dedicated.exe` on Windows) beside the client for Create game
and local `devmap`.

From that folder:

```sh
./jkr-viewer
```

Join a server directly:

```sh
./jkr-viewer --connect 127.0.0.1:29071
```

Without an explicit positional GameData argument, discovery checks these locations
in order and uses the first containing `base/assets0.pk3` and `base/assets3.pk3`:

1. `JKR_GAME_DATA`, if set and nonempty.
2. The executable's directory, then its `GameData` subdirectory.
3. The saved `fs_gameData` setting.
4. The working directory, its `GameData` subdirectory, then its
   `Star Wars Jedi Knight - Jedi Academy/GameData` subdirectory.
5. The existing Linux Steam and `~/Games/Jedi Academy/GameData` locations.

Thus a drop-in installation wins over a saved location from another installation,
while an explicit environment or positional path still overrides it. Invalid
discovery candidates are skipped; an invalid explicit positional path is an error.
Discovery does not change the working directory or move any game data.

For a binary kept separately, `JKR_GAME_DATA=/path/to/GameData ./jkr-viewer` opens
the main menu. Positional launch also accepts a map path and optional player-model
directory (`jkr-viewer /path/to/GameData maps/mp/ffa3.bsp`); it remains a direct
world/viewer launch, whereas no arguments opens the main menu. Demo playback
retains its explicit GameData argument.
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

## Third-person camera

Third-person framing follows OpenJK multiplayer `CG_OffsetThirdPersonView`.
The camera uses a four-unit collision hull against solid/terrain/player-clip
surfaces and presented inline models, including moving doors and platforms.
Player and vehicle bodies do not obstruct this camera trace. When geometry
collapses the camera onto its target, the view uses the intended forward direction
instead of constructing an undefined look-at matrix.

Pitch limits, pitch-offset direction and turn-dependent damping follow the
multiplayer reference. View changes, teleports, followed-player changes and
mounting/dismounting reset the presentation history. The ordinary range, height,
angle and damping cvars remain available; `cg_thirdPersonHorzOffset` controls the
stock sideways offset.

Vehicle appearances cache their `.veh` camera settings when loaded. Mounted
views use the authored range, height, pitch and sideways offsets, including the
pitch-dependent and fighter-strafe adjustments. Vehicle targets follow immediately
and camera damping follows the stock vehicle policy. These are presentation
changes; movement, vehicle physics and protocol serialization are unchanged.

The locally piloted vehicle is drawn at the same predicted origin and angles as
its movement, following `CG_AddPacketEntities` and `CG_CalcEntityLerpPositions`.
Its rider seat uses that same root with the current animated driver bolt. This
avoids the camera advancing ahead while the mount and rider trail behind on
snapshot interpolation. Remote vehicles, passengers without local vehicle
prediction, and demo playback retain their snapshot presentation. The camera's
animated seat is evaluated every rendered frame. Prediction-error smoothing uses
the vehicle root while piloting, avoiding false corrections from rider animation.
The movement collision adapter excludes the piloted vehicle's own snapshot body
and its owned objects, matching the stock skip/ownership rules; dismounted and
foreign vehicles remain solid.
Snapshot replay also retains the same ride's local vehicle timers, including turbo
expiry and recharge, while refreshing its networked state. A different vehicle,
pilot or definition starts with fresh local state. This prevents exhausted boost
input from predicting a new burst after every snapshot.

## Animation sounds and voice variants

Footsteps and authored swing/spin sounds follow the evaluated lower/upper Ghoul2
frames and the model's `animevents.cfg`, including the shared skeleton table and
`include` directives. The client reads supported sound assets during appearance
loading and queues decoding on the audio worker; ordinary frame playback performs
no file reads. A world build shares the skeleton and event assets across matching
appearances. Active actors release their prefetched encoded bytes after registration.

Ground-contact events trace beneath the animated foot and select the authored
walk/run sound bank for the surface material. `cg_footsteps 0` mutes them. Frame
latches prevent repeated playback while an animation frame is held; absent actors,
teleports, backwards seeks and paused map changes reset the cursor. First-person
local actors use the same evaluated timing. This restores the blue-stance taunt's
spin sounds and authored melee/kick swing cues. Custom saber `spinSound` and
`swingSound1`–`3` override the standard animation samples.

Taunt, flourish and gloat voice choices advance per accepted event, with fallbacks
based on samples that actually resolved. The expanded taunt bank is used in FFA
as in TaystJK, rather than restricting ordinary FFA taunts to `taunt.wav`. Selection
is replay-stable but is not the legacy global random stream; repeats remain
possible. Animation selection, movement, saber timing and network events are
unchanged. Animation-driven effect/footprint marks and gameplay event actions
remain outside this audio adapter.

## Slider values

Every slider in Settings, the saber RGB controls (including the second saber),
and the Shot panel supports direct numeric entry. Click its displayed value or
select the row and press Enter, then type a replacement. Enter applies it;
Escape cancels. Left/Right, Home/End, Backspace and Delete edit the draft.
Clicking another control discards an unfinished draft. Hovering does not move
an edit to another setting.
Text settings such as the master server follow the same rule, and Enter
writes the setting whose edit was opened.

Manual values respect the slider bounds but do not snap to its drag increment:
for example, the FPS cap accepts 142 and FOV accepts 97.5. Decimal points and
commas are accepted; integer controls require whole numbers. Invalid or empty
input stays open with a red underline and does not change the setting. Dragging
and arrow adjustment outside editing retain their existing behavior.
Values they write are rounded to the step's decimals, so a 0.05-step slider
stores 0.35 rather than 0.35000000000000003.

## Development maps

Run `devmap mp/ffa3` in the client console to start and join an owned local
FFA server with cheats enabled, no bots and no match limits. Other installed
maps work too, including `devmap t2_rancor`; `maps/` and `.bsp` are optional.
The command appears in console completion/help. It uses the same `jkr-dedicated`
binary lookup as Create game (`JKR_DEDICATED` overrides the adjacent binary).

This starts a fresh game on loopback, without master-server advertising. Once
launched, it replaces the current connection; it never asks a remote server to
change maps or allow cheats. Missing map names are reported before leaving the
current game. Disconnecting, cancelling the join, or exiting stops the owned
server. Ordinary Create game launches still leave cheats disabled.

The native server implements `noclip`, `give`, `setviewpos`, and `t_use` for
development. Run `noclip` again to return to ordinary movement; spawning again
clears it. Normal servers still require their own cheat permission. `god` remains
unimplemented on the native server.

## Talk balloons

Opening chat, the console or a menu sends the stock talk button and disables
other movement input while that keyboard catcher is active. Players carrying
the talk flag have a chatbubble over their heads; the connection-trouble icon
takes priority when the server marks a lost connection. These are upright frame
billboards using the existing [sprite orientation](rendering.md#billboard-icons).
Their texture opacity is preserved near walls even with soft particles enabled.
Your own bubble is visible in third person, not the first-person view. Mind-tricked
players, NPC talk flags and intermission do not show talk balloons. Siege voice
command icons remain unimplemented. See
[player_sprites.rs](../crates/jkr-viewer/src/player_sprites.rs) and
[pmove_talk.rs](../crates/jkr-game-jka/src/pmove_talk.rs).

## Joining and changing maps

The menu's FFA3 gate opens onto the prepared destination world. Map preparation
and connection run independently: when the world is ready first, you can walk,
jump and crouch locally while the connection finishes. A matching verified world
is reused when the server session becomes ready; joining does not build it twice.
The opening and crossing animation takes about 2.1 seconds once the destination
is ready. The gate stays closed while required content is unavailable. The
existing connection notice shows the server address/status and Cancel action
over the gate during joining; it disappears when the destination is entered.

On a server map change, gameplay pauses and a loading notice is shown over the
previous view until the destination and a fresh active snapshot are ready. The
client then adopts the server's spawn state. Match-end intermission uses the
server's normal camera, scoreboard and ready-to-exit controls. Player bodies and
vehicles are hidden there, matching codemp; scripted non-vehicle NPCs remain visible. Local gameplay
continuation on the old map is suspended while that feature is developed further.
The viewer no longer prepares an in-process native game for every loaded world.

CPU map preparation and GPU resource installation remain on background workers,
using the existing GPU context. Archive checksum inventory reads ZIP directories
without decompressing every asset. Matching same-map restarts can still reuse
the prepared world, and the gate adopts its already-built destination. The
waiting connection sends neutral commands during map loading and is kept separate
from the displayed old world, so new-map entities cannot appear in the wrong BSP.
Texture mip preparation and lamp extraction use bounded CPU workers; repeated
texture loads can reuse a bounded process-local mip cache. See
[load-time rendering preparation](rendering.md#load-time-texture-and-light-preparation)
for cache limits and unchanged output semantics. The gate animation and server
readiness still contribute to the time before play begins.

First entry through the gate before a server player exists retains movement-only
exploration with frozen brush collision. The gate's through-door view uses the
existing lightweight rendering path; full lighting begins when the destination
becomes the active world.

Server console output (`print`) goes exclusively to the console, including match
statistics, command replies and server announcements. It never enters chat history.
Each line of a print becomes its own console row, without the empty row a
server's closing newline would leave. Global, team and private chat retain their
conversation overlay, and are also kept in the console scrollback but not among
its notify lines, as stock cgame echoes chat with the `*` print prefix that
`CL_ConsolePrint` keeps out of the notify area. Center-print
gameplay notices keep their separate HUD presentation. The scoreboard continues
to use structured server scores rather than parsing printed statistics tables.

Chat remains connected to the real server during intermission, including
global/team/private composer messages and console/bound chat commands. Messages
received during background loading are retained; its loading notice hides the HUD. The scoreboard reserves a separate left column for messages and the
composer, temporarily overriding chat position/width while scores are visible.
Chat visibility/lifetime settings still apply. Typing captures gameplay input as
usual, and the ordinary chat layout returns after the scoreboard closes.

Escape opens the normal menu during exploration. Disconnect/cancel abandons the
pending connection and restores the retained main-menu world. Connection and asset
errors still show an error message. Downloads retain their existing policy and
limits; progress is available in the console. This does not eliminate disk,
shader, network or download latency, and direct command-line startup is separate
from the already-open menu's transition path.

Implementation: [resident worlds](../crates/jkr-viewer/src/resident_world.rs),
[early exploration](../crates/jkr-viewer/src/resident_walk.rs),
[world handoff](../crates/jkr-viewer/src/session_transition.rs) and
[gate destination](../crates/jkr-viewer/src/portal.rs).

Losing window focus releases held gameplay controls and discards gameplay actions
already queued for that frame. Synthetic key events on refocus cannot re-press a
held modifier such as Alt. This allows a saber throw already sent to the server
to finish normally after Alt+Tab.

### Chat player actions

Open the chat composer with your chat binding (`messagemode`), then click a
sender's name. The cursor is free while composing. The player menu offers:

- **whisper:** keeps the current draft and addresses the selected player using
  the stock `tell` command. Nothing is sent until Enter.
- **ignore:** hides that player's existing and incoming
  messages locally for the current map. Opening chat shows a hidden-message row
  whose name can be clicked to undo the ignore. It does not change server policy
  or suppress footsteps, saber effects, or other gameplay sounds.
- **friend:** saves a local name bookmark and adds a
  small five-point star to the left of that player's name. Bookmarks survive restarts in
  `chat-friends.txt`, beside `config.cfg`. Names ignore colour codes but otherwise
  match exactly; these are name bookmarks, not authenticated accounts.
- **copy:** copies the complete name, including its colour escapes.

The dropdown opens without a highlighted action. Hover follows the pointer;
keyboard navigation highlights only its current row until the pointer moves.
Arrow keys/Tab navigate the player menu; Enter selects and Escape dismisses it
without discarding the draft. The compact square-edged dropdown sits to the left
of chat, aligned with the clicked name and kept above the composer. It has only
four labels, no title or description, and never moves the conversation. If the
left margin is too narrow, it uses the right edge inside the chat lane to avoid
the scoreboard. Highlighted `ignore`/`friend` rows indicate active toggles; clicking
again undoes them. Name hover fits the visible username glyph bounds, excluding the star. Dropdown
row highlights use exactly the same rectangle as their clickable button. There is no
standing player-options hint or success notice. Opening the composer exposes history even when
passive chat is hidden with `cg_chatbox 0`.

The draft shares the console's UTF-8 caret and selection rules: arrows/Home/End,
Ctrl+arrows and Ctrl+Backspace/Delete, Shift-selection, Ctrl+A/C/X/V,
Ctrl+Insert to copy and Shift+Insert to paste. Click places the caret, drag selects,
and double-click selects a token. Selected text is highlighted; typing/pasting
replaces it. Clipboard text keeps colour escapes, strips controls and obeys the
existing chat byte limit. Pasting never sends a message. The `^` dead key inserts
a literal colour prefix without affecting the next character.

Actions use server-provided sender slots and current roster generations. Old
messages cannot address a replacement after an observed departure/name change;
an invalid whisper recipient leaves the draft open. Unattributed server messages
remain unclickable rather than guessing a destination from displayed text.
Legacy servers provide no authenticated account identity; unobserved same-name
slot reuse cannot be distinguished. No transport or protocol encoding changed.

The leader/opponent portrait and its name/score occupy the top-right corner,
with a 32-unit top margin and the existing 40-unit right margin at 1080p (scaled
with the HUD). Optional snapshot diagnostics, inventory and the automatically
positioned team overlay flow below that block. Explicit team-overlay coordinates
remain authoritative. Visibility and server-selected leader/opponent rules are
unchanged.

## Configuration and content

The default writable client folder is `GameData/jkr/`, under the selected game
installation. It is independent of the executable's location and working
directory. The client creates it automatically. Important files include:

| File or folder | Contents |
| --- | --- |
| `config.cfg` | Settings and key bindings |
| `marks.txt` | Marked map positions, views and notes |
| `favorites.json`, `chat-friends.txt` | Favorite servers and friend names |
| `jakey` | Persistent client identity key |
| `hud.json` | Optional custom HUD |
| `screenshots/`, `demos/` | Screenshots and recordings |
| `chatlogs/`, `qconsole.log` | Logs when enabled |

On first use, existing files from the previous per-user JKR folder are copied
into this folder. Root-level `.cfg` files and `configs/` are included. Files
already present in the destination win; the originals are never deleted. A
`.user-data-imported` marker prevents repeated imports, including restoration of
files subsequently deleted by the player. Imports skip source links and PK3s.
An incomplete import reports an error and can be retried without overwriting
completed files.

If `GameData/jkr/` cannot be written, the client uses its per-user folder:
`$XDG_CONFIG_HOME/jkr/` (otherwise `~/.config/jkr/`) on Linux, `%APPDATA%\jkr\`
on Windows, or `~/Library/Application Support/jkr/` on macOS. The chosen folder
is shared by all profile consumers for the entire session. Startup reports it;
the console's `path` command also lists it. The fallback uses the profile in
that per-user folder; the old and portable profiles are not continuously synced.
See [platform.rs](../crates/jkr-viewer/src/platform.rs) and
[storage.rs](../crates/jkr-viewer/src/platform/storage.rs).
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

Server reference lists use OpenJK's positional common-prefix rule: extra pak
names or checksums without a counterpart are ignored, including when one list
is empty. Download comparison and session cache selection share
[the compatibility parser](../crates/jkr-client/src/referenced_paks.rs), so a
connection cannot pass one check only to fail the other on list length.
Paired checksums, download paths and file contents remain validated; advertised
BSP checksums are still enforced. This does not change pure-server proofs.

References are not a mandatory client install manifest. When the server sets
`sv_allowDownload 0`, or the client sets `cl_allowDownload 0`, UDP transfers are
skipped and joining proceeds with available content. Missing directory names,
unsafe download names and retail packs never produce a download request.
Unavailable referenced archives do not abort world mounting; only locally
available checksum matches are selected from the cache. The actual map must
still exist and match its advertised BSP checksum. HTTP downloading remains
unsupported, and this policy does not disable pure-server admission checks.

## Key names

Key names are shown in capitals, as retail's controls menu shows them
(`BindingFromName` upper-cases with `Q_strupr`): the key binding editor, the
vote prompt and the console's `bind`, `unbind` and `bindlist` output read `W`,
`SPACE`, `MOUSE1`. Only ASCII letters change, so layout names such as `é` keep
their character. `config.cfg` keeps the saved spelling (`bind "w" ...`), and
`bind` accepts names in any case. See
[key_names.rs](../crates/jkr-shell/src/key_names.rs).

## Colour codes

Text draws `^0` to `^9` as OpenJK's ten-entry colour table does: `^0`–`^7` are
the retail colours, `^8` is orange and `^9` grey (retail wrapped them onto black
and red). The table is `quake_color` in [text.rs](../crates/jkr-viewer/src/text.rs).

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

## Vehicle and creature assets

Vehicle appearances use the shared multiplayer `.veh` parser, including its
comment handling, model/skin selection and camera fields. Example definitions
inside block comments cannot replace the real vehicle model.

Vehicle projectile trails resolve their vehicle-weapon index through `.vwp`
definitions in model-registration order. Their authored EFX and optional rigid
models are preloaded when the vehicle is registered; an effect-only laser does
not acquire the ordinary rocket model. Custom vehicle flight-loop sounds remain
a separate audio gap.

Community NPCs need their model PK3 mounted by both the server and client. An NPC
definition alone cannot supply a missing mesh. JKR does not distribute those
packs. The animation-error isolation described in [rendering.md](rendering.md#actor-animation-failures)
protects other actors from malformed custom clips, but does not repair the clip.

## Shader remap controls

Server map recolors and material replacements are enabled by default. Use
`cg_remaps 0` to disable them, `cg_remaps 1` for Tayst's default policy excluding
player-texture configstring remaps, or `cg_remaps 2` to include those textures.
`listRemaps` lists enabled server replacements and temporary local overrides.
`remapShader <old> <new>` replaces a shader locally for the loaded map; remapping
it to itself restores the original. See [rendering](rendering.md#server-shader-remaps)
for scope and remaining limitations. These controls do not edit map geometry.
