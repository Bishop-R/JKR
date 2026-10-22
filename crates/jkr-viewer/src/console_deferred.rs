//! EternalJK's commands that leave part of their work to a later frame through its one
//! deferred script (`cg.doVstr`, run once `cg.time` passes `cg.doVstrTime`): `do <vstr>
//! <delay>`, `lowjump` (a jump tapped for one frame) and `+duck`/`-duck` (a crouch that
//! walks, so it never rolls). A jaPRO server can forbid the deferral (`restricts` bit 9)
//! and racing forbids it, and then the taps go through `wait`s instead
//! (`cg_consolecmds.c:764-1001`).

use super::*;
use std::time::{Duration, Instant};

/// `RESTRICT_DO`: the serverinfo's `restricts` bit that forbids `do`.
const RESTRICT_DO: i32 = 1 << 9;
/// jaPRO's `STAT_RACEMODE`.
const STAT_RACEMODE: usize = 11;
/// `do`'s longest delay: an hour.
const LONGEST_DELAY_MS: u64 = 60 * 60 * 1000;

impl ViewerConsole {
    /// `script` to run once `due` has passed, in place of any still waiting.
    pub(crate) fn defer_script(&mut self, script: String, due: Instant) {
        self.deferred = Some((script, due));
    }

    /// The deferred script, queued once its time has passed (`CG_DrawActiveFrame`).
    pub(super) fn run_deferred(&mut self, now: Instant) {
        if self.deferred.as_ref().is_some_and(|(_, due)| now > *due)
            && let Some((script, _)) = self.deferred.take()
        {
            self.queue_script(&script);
        }
    }
}

impl crate::GpuState {
    /// `do`, `lowjump`, `+duck` and `-duck`.
    pub(super) fn deferred_command(
        &mut self,
        name: &str,
        args: &[String],
    ) -> Result<Vec<String>, String> {
        let restricted = self.deferral_forbidden();
        let console = self.console.as_mut().ok_or("Console unavailable")?;
        let now = Instant::now();
        match name {
            "do" => {
                let [vstr, delay] = args else {
                    return Ok(vec!["Usage: /do <vstr> <delay>".to_owned()]);
                };
                if restricted {
                    return Ok(Vec::new());
                }
                let delay = jkr_game_jka::userinfo::atoi(delay.as_bytes()).max(0) as u64;
                let delay = Duration::from_millis(delay.min(LONGEST_DELAY_MS));
                console.defer_script(format!("vstr {vstr}\n"), now + delay);
            }
            "lowjump" if restricted => console.queue_script("+moveup;wait 2;-moveup\n"),
            "lowjump" => {
                console.queue_script("+moveup\n");
                console.defer_script("-moveup\n".to_owned(), now);
            }
            "+duck" if restricted => {
                console.queue_script("+speed;wait 2;-moveup;+movedown;-speed\n");
            }
            "+duck" => {
                console.queue_script("+speed;-moveup\n");
                console.defer_script("+movedown;-speed\n".to_owned(), now);
            }
            "-duck" if restricted => console.queue_script("-movedown\n"),
            _ => console.defer_script("-movedown;-speed\n".to_owned(), now),
        }
        Ok(Vec::new())
    }

    /// The serverinfo's `restricts` forbids `do`, or the player races on a jaPRO server.
    fn deferral_forbidden(&self) -> bool {
        let Some(session) = &self.live_session else {
            return false;
        };
        let info = session.game_state().config_string(0).unwrap_or_default();
        let restricts =
            jkr_protocol::info_value(info, b"restricts").map_or(0, jkr_game_jka::userinfo::atoi);
        let japro = jkr_client::ServerMod::from_serverinfo(info) == jkr_client::ServerMod::JaPro;
        let racing = japro && session.latest_snapshot().player.stats[STAT_RACEMODE] != 0;
        restricts & RESTRICT_DO != 0 || racing
    }
}
