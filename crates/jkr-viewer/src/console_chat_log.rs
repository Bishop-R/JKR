//! Opt-in chat persistence: TaystJK's engine log (`cl_logChat`, `chatlogs/chat.log`) and
//! EternalJK's cgame log (`cg_logChat`, `chatlogs/cg_<yy>-<Mon>.log`). Only incoming
//! server text performs I/O, never HUD frames.
use super::*;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// Owned log handle and sticky failure latch; retry after disabling logging.
#[derive(Default)]
pub(super) struct ChatLog {
    file: Option<File>,
    failed: bool,
    /// EternalJK's log.
    cgame: CgameLog,
}

/// EternalJK's `cg_logChat` bits (`JAPRO_CHATLOG_*`): on, each line synchronized, the
/// level's clock instead of the date, and the server's prints and centre prints too.
const CG_LOG_ENABLE: i64 = 1;
const CG_LOG_SYNC: i64 = 2;
const CG_LOG_LEVEL_TIME: i64 = 4;
const CG_LOG_PRINTS: i64 = 8;
/// `strftime`'s `%b`.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// EternalJK's log (`CG_OpenLog`, `CG_LogPrintf`): the month's file, and the level whose
/// "Start log" header has been written.
#[derive(Default)]
struct CgameLog {
    file: Option<(String, File)>,
    level: Option<i32>,
    failed: bool,
}

/// What a line of EternalJK's log is stamped with: the level it belongs to (its start
/// time, `CS_LEVEL_START_TIME`) and the match clock.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LevelClock {
    pub(crate) level_start: i32,
    pub(crate) clock: jkr_client::MatchClock,
}

/// TaystJK cl_main.cpp:3456 and cl_cgame.cpp:563-580, 664-675; EternalJK `cg_logChat`.
pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        "cl_logChat",
        0_i64,
        CvarFlags::ARCHIVE,
        "Append received chat to chatlogs/chat.log; 2 synchronizes each entry",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_logChat",
        0_i64,
        CvarFlags::ARCHIVE,
        "EternalJK chat log, chatlogs/cg_<yy>-<Mon>.log: 1 on, +2 sync, +4 level clock, +8 prints too",
    ))
}

impl ViewerConsole {
    /// Log live chat before display filtering, with no audio or network side effects.
    pub(crate) fn log_chat(
        &mut self,
        kind: &jkr_client::ServerEventKind,
        text: &str,
        level: LevelClock,
    ) {
        use jkr_client::ServerEventKind::{Chat, TeamChat};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // EternalJK's: chat, and the prints and centre prints with its bit 8.
        let cgame = self.integer_cvar("cg_logchat").unwrap_or(0);
        if cgame & CG_LOG_ENABLE != 0
            && (matches!(kind, Chat | TeamChat) || cgame & CG_LOG_PRINTS != 0)
        {
            let directory = self.config_directory.clone();
            if let Err(error) = self
                .chat_log
                .cgame
                .append(&directory, cgame, timestamp, level, text)
            {
                self.push_log(format!("cg_logChat disabled after write error: {error}"));
            }
        }
        if !matches!(kind, Chat | TeamChat) {
            return;
        }
        let mode = self.integer_cvar("cl_logchat").unwrap_or(0);
        if let Err(error) = self
            .chat_log
            .append(&self.config_directory, mode, timestamp, text)
        {
            self.push_log(format!("Chat logging disabled after write error: {error}"));
        }
    }
}

impl CgameLog {
    /// `CG_LogPrintf` for one line of server text at Unix time `time`.
    fn append(
        &mut self,
        directory: &std::path::Path,
        mode: i64,
        time: u64,
        level: LevelClock,
        text: &str,
    ) -> std::io::Result<()> {
        if self.failed {
            return Ok(());
        }
        let result = self.write(directory, mode, time, level, text);
        if result.is_err() {
            self.failed = true;
            self.file = None;
        }
        result
    }

    fn write(
        &mut self,
        directory: &std::path::Path,
        mode: i64,
        time: u64,
        level: LevelClock,
        text: &str,
    ) -> std::io::Result<()> {
        let (year, month, day) = civil_date(time);
        let name = format!("cg_{:02}-{}.log", year % 100, MONTHS[(month - 1) as usize]);
        if self.file.as_ref().is_none_or(|(open, _)| *open != name) {
            let directory = directory.join("chatlogs");
            std::fs::create_dir_all(&directory)?;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(directory.join(&name))?;
            self.file = Some((name, file));
            self.level = None;
        }
        let (_, file) = self.file.as_mut().expect("opened above");
        // `cg.log.started`: a header for each level, as each cgame starts the log anew.
        if self.level != Some(level.level_start) {
            self.level = Some(level.level_start);
            file.write_all(
                b"Start log\n--------------------------------------------------------------\n\n",
            )?;
        }
        if mode & CG_LOG_LEVEL_TIME != 0 {
            write!(file, "{}:{:02} ", level.clock.minutes, level.clock.seconds)?;
        } else {
            write!(
                file,
                "[{year:04}-{month:02}-{day:02}] [{:02}:{:02}:{:02}] ",
                time / 3600 % 24,
                time / 60 % 60,
                time % 60
            )?;
        }
        // `Q_StripColor`, one line per row of text.
        for row in text.trim_end_matches('\n').split('\n') {
            let mut cleaned = [0_u8; 4096];
            let length = clean(row.as_bytes(), &mut cleaned);
            file.write_all(&cleaned[..length])?;
            file.write_all(b"\n")?;
        }
        if mode & CG_LOG_SYNC != 0 {
            file.sync_data()?;
        }
        Ok(())
    }
}

/// The UTC calendar date of a Unix time: year, month (1-12) and day.
pub(crate) fn civil_date(seconds: u64) -> (i64, i64, i64) {
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days / 146_097;
    let day = days - era * 146_097;
    let year = (day - day / 1_460 + day / 36_524 - day / 146_096) / 365;
    let ordinal = day - (365 * year + year / 4 - year / 100);
    let month = (5 * ordinal + 2) / 153;
    let date = ordinal - (153 * month + 2) / 5 + 1;
    let month = month + if month < 10 { 3 } else { -9 };
    (year + era * 400 + i64::from(month <= 2), month, date)
}

impl ChatLog {
    fn append(
        &mut self,
        directory: &std::path::Path,
        mode: i64,
        time: u64,
        text: &str,
    ) -> std::io::Result<()> {
        if mode == 0 {
            *self = Self::default();
            return Ok(());
        }
        if self.failed {
            return Ok(());
        }
        let result = self.write(directory, mode, time, text);
        if result.is_err() {
            self.failed = true;
            self.file = None;
        }
        result
    }

    fn write(
        &mut self,
        directory: &std::path::Path,
        mode: i64,
        time: u64,
        text: &str,
    ) -> std::io::Result<()> {
        if self.file.is_none() {
            let directory = directory.join("chatlogs");
            std::fs::create_dir_all(&directory)?;
            self.file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(directory.join("chat.log"))?,
            );
        }
        // Bounded UTF-8-safe cleaning; no per-message string or texture allocations.
        let mut cleaned = [0_u8; 4096];
        let length = clean(text.as_bytes(), &mut cleaned);
        let file = self.file.as_mut().expect("opened above");
        write!(file, "[{time}] ")?;
        file.write_all(&cleaned[..length])?;
        file.write_all(b"\n")?;
        if mode == 2 {
            file.sync_data()?;
        }
        Ok(())
    }
}

fn clean(text: &[u8], output: &mut [u8]) -> usize {
    let (mut source, mut written) = (0, 0);
    while source < text.len() && written < output.len() {
        if text[source] == b'^' && text.get(source + 1).is_some_and(u8::is_ascii_digit) {
            source += 2;
            continue;
        }
        let byte = text[source];
        source += 1;
        if byte < 32 || byte == 127 {
            continue;
        }
        output[written] = byte;
        written += 1;
    }
    while std::str::from_utf8(&output[..written]).is_err() && written > 0 {
        written -= 1;
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EternalJK's log: the month's file, a header per level, the date or the level's
    /// clock before each line, colours stripped and a line per row.
    #[test]
    fn the_cgame_log_writes_eternaljk_lines() {
        let directory = std::env::temp_dir().join(format!("jkr-cg-log-{}", std::process::id()));
        let mut log = CgameLog::default();
        let level = |level_start, minutes, seconds| LevelClock {
            level_start,
            clock: jkr_client::MatchClock { minutes, seconds },
        };
        // 2026-10-09 06:07:08 UTC.
        let time = 1_791_526_028;
        log.append(&directory, 1, time, level(500, 1, 2), "^1Kyle^7: hello")
            .unwrap();
        log.append(&directory, 1 | 4, time, level(500, 3, 4), "two\nrows\n")
            .unwrap();
        log.append(&directory, 1, time, level(900, 0, 0), "next level")
            .unwrap();
        let written =
            std::fs::read_to_string(directory.join("chatlogs").join("cg_26-Oct.log")).unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        let header =
            "Start log\n--------------------------------------------------------------\n\n";
        assert_eq!(
            written,
            format!(
                "{header}[2026-10-09] [06:07:08] Kyle: hello\n3:04 two\nrows\n{header}[2026-10-09] [06:07:08] next level\n"
            )
        );
        assert_eq!(civil_date(0), (1970, 1, 1));
    }
}
