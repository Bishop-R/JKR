//! Shared presentation-side observation of newly accepted snapshots.
//!
//! Live networking and deterministic demo playback both enter through this
//! module. This mirrors cgame's event observation rule: every accepted
//! snapshot is observed once, even when one rendered frame advances across
//! several snapshots.

use super::*;

/// Cumulative snapshot-presentation counters used by capture evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Counters {
    pub(crate) snapshots: u64,
    pub(crate) impact_events: u64,
    pub(crate) impact_visual_requests: u64,
    pub(crate) saber_flare_latches: u64,
}

/// Mutable presentation sinks shared by live networking and demo playback.
pub(crate) struct Sinks<'a> {
    pub(crate) audio: &'a mut Option<GameAudio>,
    pub(crate) vfs: &'a VirtualFileSystem,
    pub(crate) impacts: &'a mut impacts::Pool,
    pub(crate) clash_flare: &'a mut LegacySaberClashFlare,
    pub(crate) particles: &'a mut Vec<Particle>,
    pub(crate) auxiliary: &'a mut effect_aux::Runtime,
    pub(crate) effects: &'a mut EffectLibrary,
    pub(crate) damage_feedback: &'a mut damage_feedback::Feedback,
    pub(crate) first_person_view: &'a mut first_person_view::Tracker,
    pub(crate) previous_particle_events: &'a mut HashMap<u16, u16>,
    pub(crate) muzzle_effects: &'a mut jkr_client::LegacyMuzzleEffects,
    pub(crate) force_overlays: &'a mut jkr_client::LegacyForceOverlayTracker,
    pub(crate) obituaries: &'a mut jkr_client::ObituaryTracker,
    pub(crate) lagometer: &'a mut jkr_client::LagometerSamples,
    pub(crate) console: &'a mut Option<console::ViewerConsole>,
    pub(crate) localization: &'a Localization,
    pub(crate) counters: &'a mut Counters,
}

/// Observe one accepted snapshot through every snapshot-driven presentation
/// subsystem.
pub(crate) fn observe(
    snapshot: &Snapshot,
    game_state: &GameState,
    local: impact_spawn::LocalView,
    predicting_local_view: bool,
    visual_now: Instant,
    sinks: &mut Sinks<'_>,
) {
    if let Some(audio) = sinks.audio.as_mut() {
        audio.observe_snapshot(snapshot, game_state, sinks.vfs);
    }
    let impact = impact_spawn::observe_snapshot(
        sinks.impacts,
        sinks.clash_flare,
        snapshot,
        local,
        impact_spawn::Sinks {
            particles: sinks.particles,
            auxiliary: sinks.auxiliary,
            effects: sinks.effects,
            vfs: sinks.vfs,
            audio: sinks.audio,
        },
        visual_now,
    );
    sinks
        .damage_feedback
        .observe(&snapshot.player, snapshot.server_time);
    sinks
        .first_person_view
        .observe(snapshot, predicting_local_view);
    effect_runtime::spawn_snapshot_particles(
        sinks.particles,
        sinks.auxiliary,
        sinks.previous_particle_events,
        snapshot,
        game_state,
        sinks.vfs,
        sinks.effects,
        sinks.audio,
        visual_now,
    );
    sinks.muzzle_effects.observe(snapshot);
    sinks.force_overlays.observe_snapshot(snapshot);
    let obituary_before = sinks.obituaries.decoded();
    sinks.obituaries.observe(snapshot, game_state);
    let obituary_count = sinks.obituaries.decoded() - obituary_before;
    if let Some(audio) = sinks.audio.as_mut() {
        for offset in (0..obituary_count as usize).rev() {
            if let Some(event) = sinks.obituaries.feed().newest(offset) {
                audio.kill_cue(event, snapshot);
            }
        }
    }
    if let Some(console) = sinks.console.as_mut() {
        for offset in (0..obituary_count as usize).rev() {
            if let Some(event) = sinks.obituaries.feed().newest(offset) {
                console.push_log(format_obituary(event, game_state, sinks.localization));
            }
        }
    }
    let ping = snapshot
        .server_time
        .wrapping_sub(snapshot.player.command_time())
        .max(0);
    sinks
        .lagometer
        .add_received(snapshot.message_sequence, ping, snapshot.flags);
    sinks.counters.snapshots += 1;
    sinks.counters.impact_events += impact.events as u64;
    sinks.counters.impact_visual_requests += impact.visual_requests as u64;
    sinks.counters.saber_flare_latches += impact.flare_latches as u64;
}

fn format_obituary(
    event: jkr_client::ObituaryEvent,
    game: &GameState,
    localization: &Localization,
) -> String {
    use std::fmt::Write as _;
    let target = client_name(game, event.target);
    let key = event.attacker_message.unwrap_or(event.message);
    let phrase = localization.strings.get(key).map_or(key, String::as_str);
    let mut line = String::with_capacity(128);
    if event.attacker_message.is_some() {
        let attacker = client_name(game, event.attacker);
        let _ = write!(line, "{target} {phrase} {attacker}");
    } else {
        let _ = write!(line, "{target} {phrase}");
    }
    line
}

fn client_name(game: &GameState, client: u16) -> &str {
    let Some(info) = game.config_string(1_131 + usize::from(client)) else {
        return "noname";
    };
    let Ok(info) = std::str::from_utf8(info) else {
        return "noname";
    };
    let mut fields = info.trim_start_matches('\\').split('\\');
    while let (Some(key), Some(value)) = (fields.next(), fields.next()) {
        if matches!(key, "n" | "name") {
            return value;
        }
    }
    "noname"
}

impl GpuState {
    pub(crate) fn update_demo_playback(
        &mut self,
        game_audio: &mut Option<GameAudio>,
        visual_now: Instant,
    ) {
        let Some(mut session) = self.demo_session.take() else {
            return;
        };
        let relative = session.paced_relative_millis(self.ui_epoch.elapsed());
        let relative = self.console.as_ref().map_or(relative, |console| {
            console.demo_time(
                relative,
                (session.current_server_time() - i64::from(session.first_server_time())).max(0)
                    as u64,
            )
        });
        let advance = session.advance_with_config(relative, |snapshot, game_state, changes| {
            if let (Some(audio), Some(vfs)) = (game_audio.as_mut(), &self.vfs) {
                audio.prepare_config_strings(changes, game_state, vfs);
            }
            let local = impact_spawn::LocalView {
                client_num: snapshot.player.client_num(),
                third_person: self.third_person,
                flash_point: self.last_first_person_flash,
            };
            observe(
                snapshot,
                game_state,
                local,
                false,
                visual_now,
                &mut Sinks {
                    audio: game_audio,
                    vfs: self
                        .vfs
                        .as_ref()
                        .expect("demo playback retains its mounted VFS"),
                    impacts: &mut self.impacts,
                    clash_flare: &mut self.saber_clash_flare,
                    particles: &mut self.particles,
                    auxiliary: &mut self.effect_aux,
                    effects: &mut self.effects,
                    damage_feedback: &mut self.damage_feedback,
                    first_person_view: &mut self.first_person_view,
                    previous_particle_events: &mut self.previous_particle_events,
                    muzzle_effects: &mut self.muzzle_effects,
                    force_overlays: &mut self.force_overlays,
                    obituaries: &mut self.obituaries,
                    lagometer: &mut self.lagometer,
                    console: &mut self.console,
                    localization: &self.localization,
                    counters: &mut self.snapshot_observations,
                },
            );
        });
        if let Err(error) = advance {
            eprintln!("demo playback stopped: {error}");
            self.quit_requested = true;
            self.demo_session = Some(session);
            return;
        }
        if session.ended() {
            self.quit_requested = true;
        }
        update_demo_camera(self, &session);
        self.demo_session = Some(session);
    }
}

impl GpuState {
    /// Stand the live presentation on `snapshot`: the shell's team-menu
    /// reflex, entity poses, the presentation and server clocks and the
    /// local view. `observe_events` runs the once-per-snapshot event
    /// observation (`snapshot_presentation::observe`); a snapshot the
    /// previous world already observed passes `false`.
    pub(crate) fn present_live_snapshot(
        &mut self,
        snapshot: &Snapshot,
        observe_events: bool,
        game_audio: &mut Option<GameAudio>,
        visual_now: Instant,
    ) {
        let Some(session) = self.live_session.as_ref() else {
            return;
        };
        let intermission = jkr_client::IntermissionView::from_player_state(&snapshot.player);
        if self.live_presentation_ready() {
            self.world_materials.areas.update(&snapshot.area_mask);
        }
        // A server-owned frozen selection scene handles admission itself. Once
        // entered, later freecam elimination must not open the ordinary join UI.
        if snapshot.player.movement_type() == 6 {
            self.auto_opened_team_menu = true;
        }
        if snapshot.player.is_spectator() && intermission.is_none() && !self.auto_opened_team_menu {
            self.game_menu = true;
            self.game_menu_page = GameMenuPage::Team;
            self.game_menu_row = 0;
            self.auto_opened_team_menu = true;
            self.gameplay_input.release_keys();
        }
        if observe_events {
            observe(
                snapshot,
                session.game_state(),
                impact_spawn::LocalView {
                    client_num: snapshot.player.client_num(),
                    third_person: self.third_person,
                    flash_point: self.last_first_person_flash,
                },
                !snapshot.player.is_spectator(),
                visual_now,
                &mut Sinks {
                    audio: game_audio,
                    vfs: self
                        .vfs
                        .as_ref()
                        .expect("live sessions retain their mounted VFS"),
                    impacts: &mut self.impacts,
                    clash_flare: &mut self.saber_clash_flare,
                    particles: &mut self.particles,
                    auxiliary: &mut self.effect_aux,
                    effects: &mut self.effects,
                    damage_feedback: &mut self.damage_feedback,
                    first_person_view: &mut self.first_person_view,
                    previous_particle_events: &mut self.previous_particle_events,
                    muzzle_effects: &mut self.muzzle_effects,
                    force_overlays: &mut self.force_overlays,
                    obituaries: &mut self.obituaries,
                    lagometer: &mut self.lagometer,
                    console: &mut self.console,
                    localization: &self.localization,
                    counters: &mut self.snapshot_observations,
                },
            );
        }
        if let Some(adapter) = &mut self.legacy_world_adapter {
            adapter.apply_snapshot(snapshot, session.game_state(), &mut self.live_world);
            let event = self
                .presentation_clock
                .receive_snapshot(snapshot.server_time, Instant::now());
            self.net_timing.adjustment(event);
        }
        self.server_clock
            .observe_snapshot(snapshot.server_time, Instant::now());
        if let Some(view) = intermission {
            self.local_prediction.stop(0.0);
            self.camera_position = Vec3::from_array(view.origin);
            self.camera_pitch = -view.angles[0].to_radians();
            self.camera_yaw = view.angles[1].to_radians();
        } else if !crate::local_prediction::predicts_local_view(snapshot.player.movement_flags())
            || crate::prediction_preview::interpolated::server_synchronous(session.game_state())
        {
            // Follow and synchronous movement are interpolated each rendered frame.
            let view_height = snapshot.player.view_height() as f32;
            self.local_prediction.stop(view_height);
            self.camera_position =
                Vec3::from_array(snapshot.player.origin()) + Vec3::Z * view_height;
        } else {
            self.local_prediction
                .resolve_ride(snapshot, session.game_state());
            self.camera_position =
                self.local_prediction
                    .apply_snapshot(snapshot, &self.bsp, &mut self.trace_scratch);
            self.net_timing
                .prediction(self.local_prediction.last_sample());
        }
        // After the replay: a predicted local view follows the predicted `delta_angles`.
        super::prediction_preview::observe_view(
            self,
            &snapshot.player,
            intermission.is_none()
                && crate::local_prediction::predicts_local_view(snapshot.player.movement_flags()),
        );
        self.net_timing.player_state(
            snapshot.player.movement_type(),
            snapshot.player.is_spectator(),
        );
    }

    /// A freshly installed world was built from the snapshot its load
    /// started with, seconds before the cut; stand it on the newest snapshot
    /// so its first frame does not show that stale state until the next
    /// snapshot arrives.
    pub(crate) fn present_latest_live_snapshot(&mut self, game_audio: &mut Option<GameAudio>) {
        let Some(snapshot) = self
            .live_session
            .as_ref()
            .map(|session| session.latest_snapshot().clone())
        else {
            return;
        };
        self.present_live_snapshot(&snapshot, false, game_audio, Instant::now());
    }
}

fn update_demo_camera(gpu: &mut GpuState, session: &demo_playback::Session) {
    let time = session.current_server_time();
    let snapshot = session.snapshot_at_or_before(time as i32);
    gpu.world_materials.areas.update(&snapshot.area_mask);
    let player = &snapshot.player;
    let local_client = u64::try_from(session.game_state().client_num).unwrap_or_default();
    let local_id = EntityId::new(local_client + 1);
    let presented = session.world().entity(local_id);
    let mut origin = presented
        .map(|entity| entity.sample(time).translation)
        .unwrap_or_else(|| player.origin());
    origin[2] += player.view_height() as f32;
    let angles = presented
        .and_then(|entity| entity.sample_pose(time))
        .map(|pose| pose.view_angles_degrees)
        .unwrap_or_else(|| player.view_angles());
    gpu.camera_position = Vec3::from_array(origin);
    gpu.detached_camera = session.camera().detached();
    gpu.camera_pitch = -angles[0].to_radians();
    gpu.camera_yaw = angles[1].to_radians();
    if let demo_playback::Camera::FirstPersonPitch(pitch) = session.camera() {
        gpu.camera_pitch = -f32::from(pitch).to_radians();
    }
    gpu.local_prediction
        .set_view_height(player.view_height() as f32);
    if let demo_playback::Camera::Spectate(client) = session.camera()
        && let Some(entity) = session.world().entity(EntityId::new(u64::from(client) + 1))
    {
        let transform = entity.sample(time);
        let rotation = Quat::from_array(transform.rotation);
        gpu.camera_position = Vec3::from_array(transform.translation) + Vec3::Z * 40.0;
        let forward = rotation * Vec3::X;
        gpu.camera_yaw = forward.y.atan2(forward.x);
        gpu.camera_pitch = forward.z.asin();
    }
    if let demo_playback::Camera::Free { origin, yaw, pitch } = session.camera() {
        gpu.camera_position = Vec3::from_array(origin);
        gpu.camera_yaw = yaw.to_radians();
        gpu.camera_pitch = -pitch.to_radians();
        return;
    }
    let (number, offset) = match session.camera() {
        demo_playback::Camera::LookAt(number) => (number, None),
        demo_playback::Camera::Orbit(number) => (number, Some(Vec3::new(-110.0, -80.0, 50.0))),
        _ => return,
    };
    if let Some(entity) = session.world().entity(EntityId::new(u64::from(number) + 1)) {
        let target = Vec3::from_array(entity.sample(time).translation) + Vec3::Z * 40.0;
        if let Some(offset) = offset {
            gpu.camera_position = target + offset;
        }
        let to_target = target - gpu.camera_position;
        gpu.camera_yaw = to_target.y.atan2(to_target.x);
        gpu.camera_pitch = (to_target.z / to_target.length().max(1.0)).asin();
    }
}
