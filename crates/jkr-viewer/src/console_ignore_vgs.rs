//! EternalJK's `ignoreVGS` (`CG_IgnoreVGS_f`): the players whose voice commands go unheard,
//! listed, toggled one by one or all at once (`-1`); and `chatlog` (`CG_ChatLogSettings_f`):
//! `cg_logChat`'s settings listed by number and toggled one at a time.


/// `CS_PLAYERS`.
const CS_PLAYERS: usize = 1_131;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: usize = 32;
/// EternalJK's `chatLog[]`: what each `cg_logChat` bit is called.
const CHAT_LOG_SETTINGS: [&str; 5] = [
    "Enable",
    "Log Sync",
    "Legacy Timestamps",
    "Log Console Prints",
    "Log Center Prints",
];

impl crate::GpuState {
    /// `ignoreVGS [player|-1]`.
    pub(super) fn ignore_vgs(
        &mut self,
        args: &[String],
        audio: &mut Option<crate::GameAudio>,
    ) -> Result<Vec<String>, String> {
        let session = self
            .live_session
            .as_ref()
            .ok_or("Not connected to a server.")?;
        let adapter = audio
            .as_mut()
            .and_then(crate::GameAudio::sound_adapter_mut)
            .ok_or("Sound is off.")?;
        let game = session.game_state();
        let names: Vec<Option<&[u8]>> = (0..MAX_CLIENTS)
            .map(|client| {
                game.config_string(CS_PLAYERS + client)
                    .filter(|info| !info.is_empty())
                    .map(|info| jkr_protocol::info_value(info, b"n").unwrap_or_default())
            })
            .collect();
        let ignored = adapter.ignored_voice_commands();
        let Some(player) = args.first() else {
            let mut lines = vec!["VGS Ignored client(s):".to_owned()];
            for (client, name) in names.iter().enumerate() {
                if let Some(name) = name
                    && ignored & (1 << client) != 0
                {
                    lines.push(format!(
                        "^5{client:2}^3: ^7{}",
                        String::from_utf8_lossy(name)
                    ));
                }
            }
            return Ok(lines);
        };
        if player == "-1" {
            let (players, said) = if ignored != 0 {
                (0, "VGS Unignored all")
            } else {
                (u32::MAX, "VGS Ignored all")
            };
            adapter.set_ignored_voice_commands(players);
            return Ok(vec![said.to_owned()]);
        }
        let crosshair = self
            .crosshair_scan
            .chat_client(session.latest_snapshot().server_time)
            .map(usize::from);
        let client =
            match jkr_client::player_lookup::client_number_from_string(player, &names, crosshair) {
                Ok(client) => client,
                Err(message) => return Ok(message.into_iter().collect()),
            };
        let bit = 1 << client;
        adapter.set_ignored_voice_commands(ignored ^ bit);
        let said = if ignored & bit != 0 {
            "VGS Unignored"
        } else {
            "VGS Ignored"
        };
        Ok(vec![format!("{said} {player}")])
    }

    /// `chatlog [setting]`.
    pub(super) fn chat_log_settings(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let console = self.console.as_mut().ok_or("Console unavailable")?;
        let bits = console.integer_cvar("cg_logChat").unwrap_or(0);
        let Some(word) = args.first() else {
            return Ok(CHAT_LOG_SETTINGS
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let mark = if bits & (1 << index) != 0 { 'X' } else { ' ' };
                    format!("{index:2} [{mark}] {name}")
                })
                .collect());
        };
        let index = jkr_game_jka::userinfo::atoi(word.as_bytes());
        let Some(name) = usize::try_from(index)
            .ok()
            .and_then(|index| CHAT_LOG_SETTINGS.get(index))
        else {
            let last = CHAT_LOG_SETTINGS.len() - 1;
            return Ok(vec![format!("chatlog: Invalid range: {index} [0, {last}]")]);
        };
        let mask = (1 << CHAT_LOG_SETTINGS.len()) - 1;
        let bits = (1 << index) ^ (bits & mask);
        console.set_cvar("cg_logChat", &bits.to_string());
        let state = if bits & (1 << index) != 0 {
            "^2Enabled"
        } else {
            "^1Disabled"
        };
        Ok(vec![format!("{name} {state}^7")])
    }
}
