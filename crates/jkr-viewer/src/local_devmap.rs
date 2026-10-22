//! Console devmap launches use the ordinary owned server and join lifecycle.
use super::*;

/// A private, empty FFA map with no match limits for development testing.
fn settings(map: String) -> HostSettings {
    HostSettings {
        map,
        gametype: "ffa",
        hostname: "JKR development".into(),
        bots: Vec::new(),
        bot_skill: 3,
        fraglimit: 0,
        timelimit: 0,
        capturelimit: 0,
        allow_lan: false,
        cheats: true,
        team_auto_join: true,
    }
}

impl crate::GpuState {
    /// Validate the local request before replacing a session; never forward it
    /// to the connected remote server or change that server's cheat policy.
    pub(crate) fn start_development_map(&mut self, map: String) {
        let path = format!("maps/{map}.bsp");
        let error = if self.client_menu.is_none() {
            Some("The client menu is unavailable".to_owned())
        } else if !self
            .vfs
            .as_ref()
            .is_some_and(|vfs| vfs.contains(&path).unwrap_or(false))
        {
            Some(format!("Can't find installed map {path}"))
        } else if !local_server::locate_server_binary().is_some_and(|path| path.is_file()) {
            Some(
                "jkr-dedicated was not found; install it beside the client or set JKR_DEDICATED"
                    .to_owned(),
            )
        } else {
            None
        };
        if let Some(error) = error {
            if let Some(console) = &mut self.console {
                console.push_log(format!("^1devmap: {error}"));
            }
            return;
        }
        self.start_local_game(settings(map));
    }
}

#[cfg(test)]
mod tests {
    /// A console devmap starts the server with the cheats on and the player straight in
    /// the match.
    #[test]
    fn devmap_turns_cheats_on_and_joins_at_once() {
        let settings = super::settings("mp/ffa3".into());
        let arguments = settings.arguments(std::path::Path::new("GameData"), 0);
        for flag in ["--cheats", "--team-auto-join"] {
            assert!(arguments.iter().any(|argument| argument == flag), "{flag}");
        }
    }
}
