use super::{LegacyReliableState, Lifetime, ReliableError, text::CommandText};
use jkr_protocol::MessageReader;

/// Session-supplied inputs to stock reliable-command flood control.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClientCommandPolicy {
    /// Server time in the legacy adapter's integer milliseconds.
    pub now: i32,
    /// Client has entered the active state, rather than only primed/downloading.
    pub active: bool,
    /// Stock listen-server exemption (`com_cl_running`).
    pub local_client_running: bool,
    /// Zero disables; one means 1,000 ms; other values are the configured interval.
    pub flood_protect: i32,
    /// Update the flood timestamp even for a flood-limited command (OpenJK's
    /// `sv_floodProtectSlow`, TaystJK's `sv_newfloodProtect 1`).
    pub slow: bool,
    /// TaystJK's `sv_newfloodProtect` above 1: `score`, `say` and `kill` each keep a window
    /// of their own, `amTele` is never held back, and every other command shares one.
    pub per_command: bool,
}

/// Which of TaystJK's per-command flood windows a command falls in (`SV_ClientCommand`,
/// `sv_client.cpp:1564-1593`), by its `Q_stricmpn` tests: `score` and `kill` alone,
/// `say ` and `amTele` as prefixes.
fn flood_window(command: &[u8]) -> Option<usize> {
    let starts = |prefix: &[u8]| {
        command.len() >= prefix.len() && command[..prefix.len()].eq_ignore_ascii_case(prefix)
    };
    if command.eq_ignore_ascii_case(b"score") {
        Some(1)
    } else if starts(b"say ") {
        Some(2)
    } else if command.eq_ignore_ascii_case(b"kill") {
        Some(3)
    } else if starts(b"amtele") {
        None
    } else {
        Some(0)
    }
}

/// Result of one ordered reliable client command (not its game-specific effect).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientCommandDecision {
    /// Already dispatched; no callback, timestamp or key changes.
    Duplicate,
    /// Dispatched to the engine; the boolean controls permission for game commands.
    Dispatched { game_commands_allowed: bool },
}

impl LegacyReliableState {
    /// Read and dispatch one `clc_clientCommand` body after its command tag.
    ///
    /// The callback always receives engine commands, even when flood policy disables
    /// game commands. It must implement actual engine/game dispatch and any session
    /// termination. It may queue server replies through the supplied reliable state.
    /// Counters/key update after callback return, matching stock execution order.
    /// No usercmd or simulation behavior is implemented here.
    pub fn read_client_command(
        &mut self,
        reader: &mut MessageReader<'_>,
        policy: ClientCommandPolicy,
        execute: impl FnOnce(&mut Self, &[u8], bool),
    ) -> Result<ClientCommandDecision, ReliableError> {
        if self.is_closed() {
            return Err(ReliableError::Closed);
        }
        let sequence = reader.read_i32()?;
        let command = CommandText::read(reader);
        if self.counters.client_sequence >= sequence {
            return Ok(ClientCommandDecision::Duplicate);
        }
        if i64::from(sequence) > i64::from(self.counters.client_sequence) + 1 {
            self.lifetime = Lifetime::Closing;
            return Err(ReliableError::ClientGap);
        }
        let mut allowed = true;
        if !policy.local_client_running && policy.active && policy.flood_protect != 0 {
            let interval = if policy.flood_protect == 1 {
                1000
            } else {
                policy.flood_protect
            };
            if policy.per_command {
                if let Some(window) = flood_window(command.as_bytes()) {
                    let last = &mut self.flood_windows[window];
                    if policy.now < last.wrapping_add(interval) {
                        allowed = false;
                    } else {
                        *last = policy.now;
                    }
                }
            } else {
                if policy.now < self.counters.last_reliable_time.wrapping_add(interval) {
                    allowed = false;
                } else {
                    self.counters.last_reliable_time = policy.now;
                }
                if policy.slow {
                    self.counters.last_reliable_time = policy.now;
                }
            }
        }
        execute(self, command.as_bytes(), allowed);
        self.counters.client_sequence = sequence;
        self.last_client = command;
        Ok(ClientCommandDecision::Dispatched {
            game_commands_allowed: allowed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_protocol::MessageWriter;

    /// Each command, its server time, and whether the game may run it.
    fn run(policy: ClientCommandPolicy, commands: &[(&str, i32)]) -> Vec<bool> {
        let mut state = LegacyReliableState::new();
        commands
            .iter()
            .enumerate()
            .map(|(index, &(command, now))| {
                let mut writer = MessageWriter::new(1024);
                writer.write_i32(index as i32 + 1).unwrap();
                writer.write_c_string(command.as_bytes()).unwrap();
                let bytes = writer.finish().unwrap();
                let mut reader = MessageReader::new(&bytes);
                let policy = ClientCommandPolicy { now, ..policy };
                match state
                    .read_client_command(&mut reader, policy, |_, _, _| {})
                    .unwrap()
                {
                    ClientCommandDecision::Dispatched {
                        game_commands_allowed,
                    } => game_commands_allowed,
                    ClientCommandDecision::Duplicate => panic!("{command} duplicated"),
                }
            })
            .collect()
    }

    fn policy() -> ClientCommandPolicy {
        ClientCommandPolicy {
            active: true,
            flood_protect: 1,
            ..ClientCommandPolicy::default()
        }
    }

    #[test]
    fn a_held_command_restarts_the_window_only_when_slow() {
        let commands = [("a", 10_000), ("b", 10_500), ("c", 11_200), ("d", 12_300)];
        assert_eq!(run(policy(), &commands), [true, false, true, true]);
        let slow = ClientCommandPolicy {
            slow: true,
            ..policy()
        };
        assert_eq!(run(slow, &commands), [true, false, false, true]);
    }

    #[test]
    fn taystjk_keeps_a_window_per_command() {
        let per_command = ClientCommandPolicy {
            per_command: true,
            // Ignored in this mode, as TaystJK's never restarts a held command's window.
            slow: true,
            ..policy()
        };
        let commands = [
            ("score", 10_000),
            ("say hi", 10_100),
            ("kill", 10_200),
            ("follow 1", 10_300),
            ("SCORE", 10_400),
            ("amTeleMark", 10_500),
            ("amtele", 10_600),
            ("say again", 10_700),
            ("say_team x", 10_800),
            ("say again", 11_100),
            ("scores", 11_200),
        ];
        assert_eq!(
            run(per_command, &commands),
            [
                true, true, true, true, false, true, true, false, false, true, false
            ]
        );
    }
}
