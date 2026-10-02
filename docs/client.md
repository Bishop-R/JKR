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

For graphics controls and diagnostics, see [rendering.md](rendering.md).

## Text size and spacing

The Settings screen's TEXT tab holds four archived cvars. Menu text draws as
before at the defaults; the console's rows are closer together than before.

| Cvar | Default | Range | Effect |
| --- | --- | --- | --- |
| `ui_textScale` | 1 | 0.8 to 1.2 | Text size on menu screens and the in-game menu |
| `ui_letterSpacing` | 0 | -0.05 to 0.15 | Extra space after each letter in menus and the console, as a fraction of the text size |
| `con_scale` | 1 | above 0 (menu: 0.5 to 2) | Size of the whole console: text, margins and rows |
| `con_lineSpacing` | 1.15 | 1 to 2 | Console history and notify row pitch as a multiple of the text size; 1 makes rows touch |

Menu text grows or shrinks about the centre of its line without moving the
layout, so the range is limited to what menu rows can hold; that style is
applied where retained text commands become glyph quads, see
[text/style.rs](../crates/jkr-viewer/src/text/style.rs). The console sizes its
own text with `con_scale` and puts the letter spacing into its layout
([console_view.rs](../crates/jkr-viewer/src/console_view.rs)), so anything that
measures console text, such as a caret, sees the spacing it is drawn with.
Chat, the scoreboard and the HUD are not affected.

Console defaults, compared with stock at 1080p: stock draws 8 x 16 px cells, so
its rows are 16 px apart. JKR's console text is 14 px Inter (11.6 px em, 8.4 px
capitals), and its old 22 px pitch was 1.9 em, loose for a log. A pitch of 1.15
times the text size gives 16.1 px at 1080p, stock's row pitch, and about 1.4 em
of leading. `con_maxLines` defaults to 32 so the default-height console fills
with rows (26 fit at 1080p) instead of stopping at the old 18. Letter spacing
stays 0: Inter's average advance relative to its x-height (0.89) is already
close to stock's cells (0.8), Inter's own size-specific tracking at this size
is +0.002 em, and tighter text would run digits and `il1` together. See
[console_options.rs](../crates/jkr-viewer/src/console_options.rs).
