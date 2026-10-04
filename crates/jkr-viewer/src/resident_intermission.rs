//! Playable match-end presentation with scores and messages owned by the server.
use crate::{GpuState, input::GameButton};
use jkr_client::{ClientSession, PM_INTERMISSION};
use jkr_protocol::UserCommand;

impl GpuState {
    /// Take over before the frozen intermission snapshot hides the player's actor.
    /// Failure leaves ordinary remote intermission intact rather than using a freecam.
    pub(crate) fn begin_playable_intermission(&mut self) -> bool {
        if self.resident.session.is_some()
            || self.resident.prepared_game.is_none()
            || self.resident.last_playing.is_none()
            || !self.live_map_installed
            || !self.live_session.as_ref().is_some_and(|s| {
                !s.is_local() && s.latest_snapshot().player.movement_type() == PM_INTERMISSION
            })
        {
            return false;
        }
        self.resident.intermission = true;
        self.resident.session = self.live_session.take();
        if !self.start_local_game_continuation() {
            self.live_session = self.resident.session.take();
            self.resident.intermission = false;
            return false;
        }
        self.particles.clear();
        self.effect_aux = crate::effect_aux::Runtime::default();
        self.intermission_score_request_time = None;
        self.consume_resident_messages();
        true
    }

    pub(crate) fn resident_scoreboard_visible(&self) -> bool {
        self.resident.intermission
    }

    /// The communication endpoint remains remote during local continuation.
    pub(crate) fn communication_session_mut(&mut self) -> Option<&mut ClientSession> {
        self.resident
            .session
            .as_mut()
            .or(self.live_session.as_mut())
    }

    pub(super) fn consume_resident_messages(&mut self) {
        let Some(session) = &mut self.resident.session else {
            return;
        };
        crate::server_commands::consume_messages(
            session,
            &self.localization,
            &mut self.chat,
            self.console.as_mut(),
        );
        if !self.resident.intermission {
            return;
        }
        let snapshot = session.latest_snapshot();
        // Mods can resume a round without a gamestate or map_restart command.
        if crate::live_session::active_snapshot(snapshot)
            && snapshot.player.movement_type() != PM_INTERMISSION
        {
            self.resident.intermission = false;
            self.resident.reuse_pending = true;
            return;
        }
        let time = snapshot.server_time;
        if self
            .intermission_score_request_time
            .is_none_or(|last| time.wrapping_sub(last) > 2_000)
        {
            if let Err(error) = session.send_reliable_command(b"score") {
                eprintln!("failed to request intermission scoreboard: {error}");
            }
            self.intermission_score_request_time = Some(time);
        }
    }

    pub(super) fn resident_wait_command(&self) -> UserCommand {
        let mut command = UserCommand::default();
        if self.resident.intermission
            && let Some(session) = &self.resident.session
            && crate::live_session::active_snapshot(session.latest_snapshot())
            && session.latest_snapshot().player.movement_type() == PM_INTERMISSION
        {
            command.server_time = session.latest_snapshot().server_time;
            // codemp ClientIntermissionThink latches attack/use as ready-to-exit.
            // Never forward movement, aim, weapons or the local simulation clock.
            command.buttons = u16::from(self.gameplay_input.held(GameButton::Button(0)))
                | (u16::from(self.gameplay_input.held(GameButton::Button(2))) << 2);
        }
        command
    }
}
