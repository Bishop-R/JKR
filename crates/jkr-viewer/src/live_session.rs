//! Per-frame live network polling and command submission.

use super::{GameAudio, GpuState, server_commands, session_transition};
use std::time::{Duration, Instant};

#[path = "emplaced_view.rs"]
mod emplaced_view;

/// Consume queued snapshots in order, stopping on timeout/failure or after eight.
pub(crate) fn drain_snapshots(mut receive_and_present: impl FnMut() -> bool) {
    for _ in 0..8 {
        if !receive_and_present() {
            break;
        }
    }
}

impl GpuState {
    /// Only the installed live world shares the session's snapshot time domain.
    pub(crate) fn live_presentation_ready(&self) -> bool {
        self.live_session.is_some()
            && self.live_map_installed
            && !self.is_menu_world
            && !self.pending_map_reload
            && self.world_load_task.is_none()
            && self.world_install_task.is_none()
    }

    /// Drain queued snapshots, surface lifecycle failures, and submit due input.
    pub(crate) fn update_live_session(
        &mut self,
        game_audio: &mut Option<GameAudio>,
        visual_now: Instant,
        timing: &mut crate::frame_pacing::budget::Timer,
    ) {
        use crate::frame_pacing::budget::Phase;
        timing.mark(Phase::Snapshot);
        if let Some(audio) = game_audio.as_mut() {
            audio.set_muted_players(self.chat.muted_players());
        }
        drain_snapshots(|| {
            let Some(session) = &mut self.live_session else {
                return false;
            };
            timing.mark(Phase::Receive);
            let received = session.receive_snapshot(Duration::ZERO).cloned();
            timing.mark(Phase::Snapshot);
            // svc_gamestate may have been installed even if the following
            // snapshot times out. Consume its clock reset before sending input.
            let reactions = session_transition::consume(session.drain_transitions());
            if received.is_ok()
                && !reactions.reload_world
                && !self.pending_map_reload
                && self.world_load_task.is_none()
                && self.world_install_task.is_none()
                && let (Some(audio), Some(vfs)) = (game_audio.as_mut(), &self.vfs)
            {
                audio.prepare_config_strings(
                    session.config_string_changes(),
                    session.game_state(),
                    vfs,
                );
            }
            if reactions.reload_world {
                if let Some(audio) = game_audio.as_mut() {
                    audio.begin_map_change();
                }
                // Before presenting: the server reports inflated snapshot
                // times until it sees this client acknowledge the new
                // `serverId` (`sv_snapshot.cpp:190-198`), and a command
                // stamped from that era outranks every later one once
                // `SV_ClientEnterWorld` latches it (`sv_client.cpp:576`).
                // Stock stamps the whole map load 0 (`CL_ClearState`); the
                // world built for the new timeline anchors the clock again.
                self.server_clock.restart();
                self.pending_map_reload = true;
                self.local_prediction.clear_pending();
            }
            if let Some(reason) = reactions.disconnect_reason {
                self.session_disconnected(reason);
                return false;
            }
            let snapshot = match received {
                Ok(snapshot) => snapshot,
                Err(error) if error.is_timeout() => return false,
                Err(error) => {
                    self.session_disconnected(error.to_string());
                    return false;
                }
            };
            self.net_timing.snapshot_received(snapshot.server_time);
            self.present_live_snapshot(&snapshot, true, game_audio, visual_now);
            true
        });
        timing.mark(Phase::Commands);
        let talking = self.key_catcher_active();
        let Some(session) = &mut self.live_session else {
            return;
        };
        if let Some(silence) = self.net_timing.silence_to_report(Instant::now()) {
            // A session with no snapshots reports nothing else, so say what
            // the socket sees: a quiet server, packets arriving and being
            // discarded, or packets going out to nobody.
            let traffic = session.connection_traffic();
            let expected = session.server();
            let rejected = session
                .last_rejected_packet()
                .map(|(source, bytes)| format!("{source} {:?}", String::from_utf8_lossy(bytes)));
            crate::log::progress(format_args!(
                "no snapshots for {:.1} s: sent={} received={} stale={} wrong-source={} \
                 out-of-band={} expected={expected} last-rejected={}",
                silence.as_secs_f64(),
                traffic.sent,
                traffic.received,
                traffic.stale,
                traffic.wrong_source,
                traffic.out_of_band,
                rejected.as_deref().unwrap_or("none"),
            ));
        }
        server_commands::consume(
            session,
            &self.localization,
            &mut self.chat,
            self.console.as_mut(),
            self.legacy_world_adapter.as_mut(),
            &mut self.clientinfo_watch,
        );

        if Instant::now() < self.network_command_due {
            return;
        }
        let intermission =
            session.latest_snapshot().player.movement_type() == jkr_client::PM_INTERMISSION;
        if !intermission {
            self.intermission_score_request_time = None;
        }
        if intermission
            && self
                .intermission_score_request_time
                .is_none_or(|requested| {
                    session
                        .latest_snapshot()
                        .server_time
                        .wrapping_sub(requested)
                        > 2_000
                })
        {
            if let Err(error) = session.send_reliable_command(b"score") {
                eprintln!("failed to request intermission scoreboard: {error}");
            }
            self.intermission_score_request_time = Some(session.latest_snapshot().server_time);
        }
        let snapshot = session.latest_snapshot();
        let delta_angles = self
            .gameplay_input
            .view_authority
            .command_delta()
            .unwrap_or(snapshot.player.delta_angles());
        self.gameplay_input.selection.sync(
            &snapshot.player,
            self.server_clock.server_time(Instant::now()),
        );
        // At an emplaced gun, a view past its arc is turned back (`CG_EmplacedView`).
        // The camera's yaw is in radians, the game's in degrees.
        if let Some(yaw) = emplaced_view::forced_yaw(snapshot, self.camera_yaw.to_degrees()) {
            self.camera_yaw = yaw.to_radians();
        }
        let mut command = self.gameplay_input.user_command(
            self.server_clock.server_time(Instant::now()),
            self.camera_pitch,
            self.camera_yaw,
            delta_angles,
            self.selected_weapon
                .unwrap_or_else(|| snapshot.player.weapon()),
            snapshot.player.selected_force_power(),
            self.pending_generic_command,
        );
        command.buttons = jkr_game_jka::pmove_talk::command_buttons(command.buttons, talking);
        if let Some(console) = &self.console {
            session.set_packet_dup(console.packet_dup());
        }
        if let Err(error) = session.send_command(&command) {
            self.session_disconnected(error.to_string());
            return;
        }
        self.gameplay_input.finish_command();
        if !intermission
            && let Some(position) =
                self.local_prediction
                    .apply_command(command, &self.bsp, &mut self.trace_scratch)
        {
            self.camera_position = position;
        }
        self.pending_generic_command = 0;
        self.network_command_due += Duration::from_millis(25);
        let now = Instant::now();
        if self.network_command_due + Duration::from_millis(100) < now {
            self.network_command_due = now;
        }
    }
}
