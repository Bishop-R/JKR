//! TaystJK's server options beyond OpenJK's (`sv_init.cpp:1060-1070`): `sv_pingFix`, which
//! the endpoint applies, and hibernation — an empty server's frame rate drops to
//! `sv_hibernateFPS` once `sv_hibernateTime` milliseconds have passed since the last
//! human left (`SV_Frame`), and is back the moment a client is let in (`SV_DirectConnect`).

use super::NativeGame;
use crate::cvars::{CVAR_ARCHIVE, CVAR_NODEFAULT};
use std::time::{Duration, Instant};

/// TaystJK's options, its defaults and its descriptions.
const OPTIONS: [(&[u8], &[u8], &[u8]); 3] = [
    (
        b"sv_pingFix",
        b"1",
        b"TaystJK: improved scoreboard client ping calculation",
    ),
    (
        b"sv_hibernateTime",
        b"0",
        b"TaystJK: time after which server will enter hibernation mode",
    ),
    (
        b"sv_hibernateFPS",
        b"2",
        b"TaystJK: FPS during hibernation mode",
    ),
];

/// `svs.hibernation`.
#[derive(Debug)]
pub(super) struct Hibernation {
    pub(super) enabled: bool,
    /// `lastTimeDisconnected`: when the last human left (the process's start before any).
    last_left: Instant,
}

impl Default for Hibernation {
    fn default() -> Self {
        Self {
            enabled: false,
            last_left: Instant::now(),
        }
    }
}

impl NativeGame {
    /// TaystJK's options, archived but for their defaults (`CVAR_ARCHIVE_ND`).
    pub(super) fn register_taystjk_server(&mut self) {
        for (name, value, about) in OPTIONS {
            self.cvars
                .get(name, value, CVAR_ARCHIVE | CVAR_NODEFAULT, Some(about));
        }
        self.cvars
            .check_range(b"sv_hibernateFPS", 1.0, 1000.0, true, &mut |_| {});
    }

    /// `SV_Frame`'s look: with nobody but bots connected for `sv_hibernateTime`
    /// milliseconds, the server hibernates.
    pub(super) fn check_hibernation(&mut self) {
        let after = self.cvars.integer(b"sv_hibernateTime");
        if after == 0 || self.hibernation.enabled || self.humans_connected() {
            return;
        }
        let waited = self.hibernation.last_left.elapsed();
        if waited >= Duration::from_millis(u64::try_from(after).unwrap_or(0)) {
            self.hibernation.enabled = true;
            println!("Server entered hibernation mode");
        }
    }

    /// `SV_DirectConnect`, the game having let a client in: hibernation ends.
    pub(super) fn wake_from_hibernation(&mut self) {
        if std::mem::take(&mut self.hibernation.enabled) {
            println!("Server restored from hibernation");
        }
    }

    /// `SV_DropClient`: the time the last human left.
    pub(super) fn note_client_left(&mut self) {
        if !self.humans_connected() {
            self.hibernation.last_left = Instant::now();
        }
    }

    /// The hibernating server's frame length (`1000 / sv_hibernateFPS`).
    pub(super) fn hibernation_frame_msec(&self) -> Option<i32> {
        self.hibernation
            .enabled
            .then(|| 1000 / self.cvars.integer(b"sv_hibernateFPS").max(1))
    }

    /// Whether anyone but a bot is connected.
    fn humans_connected(&self) -> bool {
        (0..self.players.places()).any(|client| self.peer(client).is_some_and(|peer| !peer.bot))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Identity;
    use super::*;

    /// With nobody connected past `sv_hibernateTime` the server runs at
    /// `sv_hibernateFPS`; a client let in brings `sv_fps` back.
    #[test]
    fn an_empty_server_hibernates_until_a_client_comes() {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        game.register_taystjk_server();
        assert_eq!(game.cvars.integer(b"sv_pingFix"), 1);
        game.check_hibernation();
        assert!(
            !game.hibernation.enabled,
            "sv_hibernateTime 0 never hibernates"
        );
        game.set_cvar("sv_hibernateTime", "1000");
        game.hibernation.last_left = Instant::now() - Duration::from_secs(2);
        game.check_hibernation();
        assert!(game.hibernation.enabled);
        assert_eq!(game.frame_msec(), 500);
        game.wake_from_hibernation();
        assert_eq!(game.frame_msec(), 50);
    }
}
