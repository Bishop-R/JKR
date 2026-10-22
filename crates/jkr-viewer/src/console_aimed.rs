//! Stock cgame commands aimed at another player, and the client list: `tell_target` and
//! `tell_attacker` (`CG_TellTarget_f`, `CG_TellAttacker_f`) send a private message to the
//! player under the crosshair or the last attacker, `tcmd` sends that player an order
//! (`gc`, `CG_TargetCommand_f`), and `clientlist` (`CG_ClientList_f`) prints every player
//! with its team. Without a target they do nothing, as stock's.

use crate::GpuState;

impl GpuState {
    /// The player under the crosshair (`CG_CrosshairPlayer`) or the last attacker
    /// (`CG_LastAttacker`), if there is one.
    fn aimed_client(&self, attacker: bool) -> Option<u16> {
        let snapshot = self.live_session.as_ref()?.latest_snapshot();
        if attacker {
            self.damage_feedback
                .has_attacker()
                .then(|| u16::try_from(snapshot.player.persistent[6]).ok())
                .flatten()
        } else {
            self.crosshair_scan.chat_client(snapshot.server_time)
        }
    }

    /// `tell_target`, `tell_attacker` and `tcmd`.
    pub(super) fn aimed_command(
        &mut self,
        name: &str,
        args: &[String],
    ) -> Result<Vec<String>, String> {
        let Some(client) = self.aimed_client(name == "tell_attacker") else {
            return Ok(Vec::new());
        };
        let command = if name == "tcmd" {
            // `Cmd_Argv(1, test, 4)`: at most three characters, read as a number.
            let order: String = args
                .first()
                .map_or(String::new(), |order| order.chars().take(3).collect());
            let order = jkr_game_jka::userinfo::atoi(order.as_bytes());
            format!("gc {client} {order}")
        } else {
            format!("tell {client} {}", args.join(" "))
        };
        self.live_session
            .as_mut()
            .ok_or("Not connected to a server.")?
            .send_reliable_command(command.as_bytes())
            .map_err(|error| error.to_string())?;
        Ok(Vec::new())
    }

    /// `clientlist`: each player's number, team (free, red, blue, spectator), name and
    /// whether it is a bot, then the count.
    pub(super) fn client_list(&self) -> Result<Vec<String>, String> {
        let session = self
            .live_session
            .as_ref()
            .ok_or("Not connected to a server.")?;
        let game = session.game_state();
        let mut lines = Vec::new();
        for client in 0..32 {
            let Some(info) = game
                .config_string(1_131 + client)
                .filter(|info| !info.is_empty())
            else {
                continue;
            };
            let value = |key: &[u8]| jkr_protocol::info_value(info, key);
            let name = String::from_utf8_lossy(value(b"n").unwrap_or_default());
            let team = match value(b"t") {
                Some(b"0") => "F",
                Some(b"1") => "R",
                Some(b"2") => "B",
                _ => "S",
            };
            let bot = if value(b"skill").is_some_and(|skill| !skill.is_empty()) {
                " (bot)"
            } else {
                ""
            };
            lines.push(format!("{client:2} ^3{team}   ^7{name}^7{bot}"));
        }
        lines.push(format!("Listed {:2} clients", lines.len()));
        Ok(lines)
    }
}
