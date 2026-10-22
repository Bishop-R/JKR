//! EternalJK's automatic captures. `cg_autoRecordDemo` records every level to a demo from
//! its first frame outside a warmup countdown (`CG_SetInitialSnapshot`,
//! `cg_snapshot.c:151`, and again after a `map_restart`, `cg_servercmds.c:1221`), named by
//! the time, the game type, the map and the player. `cg_autoScreenshot` takes a screenshot
//! a second into each intermission (`CG_DrawIntermission`, `cg_draw.c:8583`) into
//! `screenshots/auto/`. Times are UTC, as the client's other files.

use crate::GpuState;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// `gametypeStringShort`.
const GAMETYPES: [&str; 10] = [
    "FFA", "HOLO", "JM", "1v1", "2v1", "SP", "TDM", "SAGA", "CTF", "CTY",
];
/// `CS_SERVERINFO`, `CS_WARMUP`, `CS_LEVEL_START_TIME`, `CS_PLAYERS`.
const CS_SERVERINFO: usize = 0;
const CS_WARMUP: usize = 5;
const CS_LEVEL_START_TIME: usize = 21;
const CS_PLAYERS: usize = 1131;
/// The characters `Q_strstrip` takes out of a demo's name.
const STRIPPED: &[u8] = b"\n\r;:?*<>|\"\\/ ";

/// What has been captured of the current level.
#[derive(Debug, Default)]
pub(crate) struct AutoCapture {
    /// The level (its `CS_LEVEL_START_TIME`) whose recording was started.
    recorded: Option<i32>,
    /// The level whose intermission screenshot is due, and when.
    shot_due: Option<(i32, Instant)>,
    /// The level whose intermission screenshot was taken.
    shot_taken: Option<i32>,
}

/// The two options, archived, off.
pub(crate) fn register(cvars: &mut jkr_shell::CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    use jkr_shell::{CvarDefinition, CvarFlags};
    cvars.register(CvarDefinition::new(
        "cg_autoRecordDemo",
        0_i64,
        CvarFlags::ARCHIVE,
        "EternalJK: record every level to a demo named by time, game type, map and player",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_autoScreenshot",
        0_i64,
        CvarFlags::ARCHIVE,
        "EternalJK: take a screenshot of every intermission (screenshots/auto)",
    ))
}

impl GpuState {
    /// Once a rendered frame: the captures the options ask for, now due.
    pub(crate) fn run_auto_capture(&mut self) {
        let Some(console) = self.console.as_ref() else {
            return;
        };
        let on = |name: &str| console.integer_cvar(name).is_some_and(|value| value != 0);
        let (record, shoot) = (on("cg_autoRecordDemo"), on("cg_autoScreenshot"));
        if !(record || shoot) || self.demo_session.is_some() {
            return;
        }
        let Some(session) = self.live_session.as_ref() else {
            return;
        };
        let game = session.game_state();
        let snapshot = session.latest_snapshot();
        let level = config_i32(game, CS_LEVEL_START_TIME);
        let intermission = jkr_client::IntermissionView::from_player_state(&snapshot.player);
        let client = usize::from(snapshot.player.client_num());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let name = (record
            && self.auto_capture.recorded != Some(level)
            && config_i32(game, CS_WARMUP) <= 0
            && snapshot.server_time != 0)
            .then(|| demo_name(game, client, now));
        if let Some(name) = name {
            self.auto_capture.recorded = Some(level);
            self.stop_console_recording();
            self.start_console_recording(Some(&name));
        }
        if !shoot {
            return;
        }
        let capture = &mut self.auto_capture;
        match (intermission.is_some(), capture.shot_due) {
            (true, None) if capture.shot_taken != Some(level) => {
                capture.shot_due = Some((level, Instant::now() + Duration::from_secs(1)));
            }
            (true, Some((due_level, due))) if due_level == level && Instant::now() >= due => {
                capture.shot_due = None;
                capture.shot_taken = Some(level);
                let name = format!("auto/shot{}", stamp(now));
                self.screenshots
                    .request(crate::screenshot::Request::automatic(name));
            }
            (false, Some(_)) => capture.shot_due = None,
            _ => {}
        }
    }
}

/// `%Y-%m-%d_%H-%M-%S` of a Unix time.
fn stamp(seconds: u64) -> String {
    let (year, month, day) = crate::console::civil_date(seconds);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}-{:02}-{:02}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// The demo's name: `<time>_<game type>_<map>_<player>`, without the characters a file
/// name cannot hold, nor colours.
fn demo_name(game: &jkr_protocol::GameState, client: usize, now: u64) -> String {
    let info = game.config_string(CS_SERVERINFO).unwrap_or_default();
    let gametype = jkr_protocol::info_value(info, b"g_gametype")
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|gametype| GAMETYPES.get(gametype))
        .copied()
        .unwrap_or("FFA");
    let map = jkr_protocol::info_value(info, b"mapname").unwrap_or_default();
    let player = game
        .config_string(CS_PLAYERS + client)
        .and_then(|info| jkr_protocol::info_value(info, b"n"))
        .unwrap_or_default();
    let name = [
        stamp(now).as_bytes(),
        b"_",
        gametype.as_bytes(),
        b"_",
        map,
        b"_",
        player,
    ]
    .concat();
    let kept: Vec<u8> = name
        .into_iter()
        .filter(|byte| !STRIPPED.contains(byte))
        .collect();
    String::from_utf8_lossy(&jkr_game_jka::client_view::strip_colours(&kept)).into_owned()
}

fn config_i32(game: &jkr_protocol::GameState, index: usize) -> i32 {
    game.config_string(index)
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}
