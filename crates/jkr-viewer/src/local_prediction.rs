//! Local-player movement prediction state of a live session.
//!
//! Owns the `Predictor`, the window of not-yet-acknowledged user commands
//! that is replayed on every snapshot (`CG_PredictPlayerState`,
//! `cg_predict.c:1020-1114`), the predicted view height and the prediction
//! error smoother (`cg_predict.c:1188-1237`, `cg_view.c:1597-1608`). The
//! renderer reads the predicted eye position from here; the model root uses
//! the un-offset origin exactly like `cent->lerpOrigin`.

use std::collections::VecDeque;
use std::sync::Arc;

use glam::Vec3;
use jkr_bsp::{Bsp, TraceScratch};
use jkr_client::pmove::{MovementConfig, MovementState, Predictor};
use jkr_client::{
    AnimationLengthTable, AnimationLengths, EF_TELEPORT_BIT, PredictionErrorDecay,
    legacy_saber_movement,
};
use jkr_model::AnimationConfig;
use jkr_protocol::{GameState, Snapshot, UserCommand};
use jkr_vfs::VirtualFileSystem;

use crate::movement_collision::BspMovementCollision;

#[path = "prediction_movers.rs"]
pub(crate) mod movers;
#[path = "local_prediction_reconcile.rs"]
mod reconcile;
#[path = "prediction_miss.rs"]
pub(crate) mod telemetry;

/// `CMD_BACKUP` (`qcommon/q_shared.h`): commands kept for re-prediction.
const COMMAND_BACKUP: usize = 64;

/// How the server's `ps.commandTime` relates to the commands this client sent
/// (`ClientThink_real` clamps a stamp to `[level.time - 1000, level.time + 200]`
/// and drops one that does not advance `ps.commandTime`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CommandStatus {
    /// `ps.commandTime` is a stamp still in the pending window.
    #[default]
    Acknowledged,
    /// `ps.commandTime` advanced to a stamp this client never sent.
    Rewritten,
    /// `ps.commandTime` did not advance: no command ran for this snapshot.
    Stalled,
}

impl CommandStatus {
    /// Classify a snapshot's `ps.commandTime` against the previous one and
    /// the stamps of the commands still pending.
    pub(crate) fn classify(
        acknowledged: i32,
        previous: i32,
        mut pending_stamps: impl Iterator<Item = i32>,
    ) -> Self {
        if acknowledged == previous {
            Self::Stalled
        } else if pending_stamps.any(|stamp| stamp == acknowledged) {
            Self::Acknowledged
        } else {
            Self::Rewritten
        }
    }
}

/// What one snapshot told us about the local prediction, for the
/// `cl_showtimedelta` window (`net_timing`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PredictionSample {
    pub(crate) status: CommandStatus,
    /// Commands still unacknowledged after this snapshot.
    pub(crate) pending: usize,
    /// Distance between the previous prediction and the re-prediction at the
    /// same command time (`cg_predict.c:1188-1237`), in units.
    pub(crate) miss_units: f32,
    /// Replay work and endpoint-matched large-miss correlations for this snapshot.
    pub(crate) counts: telemetry::Counts,
}

pub(crate) struct LocalPrediction {
    predict_items: bool,
    pending_events: jkr_client::predicted_events::PredictedEvents,
    predictor: Option<Predictor>,
    /// The committed predictor advanced through this frame's not-yet-sent
    /// input; what the frame presents (see [`Self::preview_command`]).
    preview: Option<Predictor>,
    presented: Option<MovementState>,
    physics_movers: movers::Movers,
    render_movers: movers::Movers,
    presentation_time: i32,
    pending: VecDeque<UserCommand>,
    latest_input: Option<UserCommand>,
    view_height: f32,
    error: PredictionErrorDecay,
    /// `(eFlags, clientNum)` of the previous snapshot, for teleport detection.
    previous_player: Option<(u32, u16)>,
    frame_millis: i32,
    previous_frame_millis: i32,
    animation_lengths: Option<Arc<dyn AnimationLengths>>,
    movement_config: MovementConfig,
    last_acknowledged: i32,
    /// Presentation-only support for the unadvanced snapshot seed. Commands
    /// still start with the untouched server ground flag and run normal Pmove.
    seed_ground: Option<u16>,
    last_sample: PredictionSample,
    miss_log: telemetry::MissLog,
    /// The vehicle the local player pilots, predicted with it.
    rides: ride::Rides,
}

impl LocalPrediction {
    /// Prediction state primed from an optional first snapshot.
    pub(crate) fn new(
        snapshot: Option<&Snapshot>,
        animation_config: Option<&AnimationConfig>,
        game_state: Option<&GameState>,
        vfs: &VirtualFileSystem,
    ) -> Self {
        let animation_lengths = animation_config.map(|config| {
            Arc::new(AnimationLengthTable::from_animation_config(config))
                as Arc<dyn AnimationLengths>
        });
        let view_height = snapshot.map_or(36.0, |snapshot| snapshot.player.view_height() as f32);
        let mut movement_config =
            game_state.map_or_else(MovementConfig::default, MovementConfig::from_game_state);
        if let Some(game_state) = game_state {
            let client = snapshot.map_or(game_state.client_num as u16, |s| s.player.client_num());
            let (no_rolls, scales, anim_scales) = legacy_saber_movement(vfs, game_state, client);
            movement_config.roll_rules.saber_forbids_rolls = no_rolls;
            movement_config.saber_speed_scales = scales;
            movement_config.saber_anim_speed_scales = anim_scales;
        }
        let predictor = snapshot.and_then(|snapshot| {
            (predicts_local_view(snapshot.player.movement_flags())
                && !game_state
                    .is_some_and(crate::prediction_preview::interpolated::server_synchronous))
            .then(|| {
                let mut predictor = Predictor::from_player_state(&snapshot.player, movement_config);
                if let Some(lengths) = &animation_lengths {
                    predictor.set_animation_lengths(Arc::clone(lengths));
                }
                predictor
            })
        });
        let mut physics_movers = movers::Movers::new();
        let mut render_movers = movers::Movers::new();
        physics_movers.set_permanents(game_state);
        render_movers.set_permanents(game_state);
        if let Some(snapshot) = snapshot {
            physics_movers.update(snapshot, snapshot.server_time);
            render_movers.update(snapshot, snapshot.server_time);
        }
        Self {
            predict_items: true,
            pending_events: Default::default(),
            predictor,
            preview: None,
            presented: None,
            physics_movers,
            render_movers,
            presentation_time: snapshot.map_or(0, |s| s.server_time),
            pending: VecDeque::with_capacity(COMMAND_BACKUP),
            latest_input: None,
            view_height,
            error: PredictionErrorDecay::default(),
            previous_player: None,
            frame_millis: 0,
            previous_frame_millis: 0,
            animation_lengths,
            movement_config,
            last_acknowledged: snapshot.map_or(0, |snapshot| snapshot.player.command_time()),
            seed_ground: None,
            last_sample: PredictionSample::default(),
            miss_log: telemetry::MissLog::default(),
            rides: ride::Rides::new(vfs),
        }
    }

    /// The bookkeeping of the most recent [`Self::apply_snapshot`].
    pub(crate) fn last_sample(&self) -> PredictionSample {
        self.last_sample
    }

    /// Refresh server policy while preserving saber restrictions and prediction history.
    pub(crate) fn refresh_roll_rules(&mut self, game_state: &GameState) {
        self.movement_config.refresh_game_state(game_state);
        for predictor in self.predictor.iter_mut().chain(self.preview.iter_mut()) {
            predictor.set_config(self.movement_config);
        }
        self.physics_movers.set_duel_isolation(Some(game_state));
        self.render_movers.set_duel_isolation(Some(game_state));
    }

    /// Reload local equipment policy when CS_PLAYERS changes, never per frame.
    pub(crate) fn refresh_sabers(
        &mut self,
        game: &GameState,
        vfs: &VirtualFileSystem,
        client: u16,
    ) {
        let local = self
            .predictor
            .as_ref()
            .map(|predictor| predictor.state().client_num)
            .unwrap_or(game.client_num as u16);
        if client != local {
            return;
        }
        let (no_rolls, scales, anim_scales) = legacy_saber_movement(vfs, game, client);
        self.movement_config.roll_rules.saber_forbids_rolls = no_rolls;
        self.movement_config.saber_speed_scales = scales;
        self.movement_config.saber_anim_speed_scales = anim_scales;
        for predictor in self.predictor.iter_mut().chain(self.preview.iter_mut()) {
            predictor.set_config(self.movement_config);
        }
    }

    /// Predicted movement state, when prediction is active
    /// (`cg.predictedPlayerState`).
    pub(crate) fn predicted_state(&self) -> Option<&MovementState> {
        self.presented.as_ref().or_else(|| {
            self.preview
                .as_ref()
                .or(self.predictor.as_ref())
                .map(Predictor::state)
        })
    }

    /// Predicted view height above the origin.
    pub(crate) fn view_height(&self) -> f32 {
        self.view_height
    }

    /// Override the view height (demo playback follows the snapshot).
    pub(crate) fn set_view_height(&mut self, view_height: f32) {
        self.view_height = view_height;
    }

    /// Update `cg_errorDecay`.
    pub(crate) fn set_error_decay_millis(&mut self, decay_millis: f32) {
        self.error.set_decay_millis(decay_millis);
    }

    /// Advance the frame clock (`cg.time` / `cg.oldTime`).
    pub(crate) fn begin_frame(&mut self, frame_millis: i32) {
        self.previous_frame_millis = self.frame_millis;
        self.frame_millis = frame_millis;
    }

    /// View-origin offset for the current frame (`cg_view.c:1597-1608`).
    pub(crate) fn view_offset(&mut self) -> Vec3 {
        Vec3::from_array(self.error.view_offset(self.frame_millis))
    }

    /// Forget every pending command (map change, intermission, spectating).
    /// Commands not yet acknowledged by a snapshot, for diagnostics.
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn clear_pending(&mut self) {
        self.latest_input = None;
        self.pending_events.clear();
        self.pending.clear();
        self.presented = None;
    }

    /// Drop prediction entirely; the view follows the snapshot.
    pub(crate) fn stop(&mut self, view_height: f32) {
        self.error = PredictionErrorDecay::new(self.error.decay_millis());
        self.latest_input = None;
        self.pending_events.clear();
        self.pending.clear();
        self.predictor = None;
        self.preview = None;
        self.presented = None;
        self.rides.committed = None;
        self.rides.preview = None;
        self.view_height = view_height;
    }

    /// Which vehicle the local player pilots in `snapshot`, for the next
    /// [`Self::apply_snapshot`] to predict with it.
    pub(crate) fn resolve_ride(&mut self, snapshot: &Snapshot, game_state: &GameState) {
        self.rides.resolve(snapshot, game_state);
    }

    /// Queue and predict a freshly issued user command; returns the eye
    /// position when prediction is active.
    pub(crate) fn apply_command(
        &mut self,
        command: UserCommand,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
    ) -> Option<Vec3> {
        self.latest_input = Some(command);
        if self.pending.len() == COMMAND_BACKUP {
            self.pending.pop_front();
        }
        self.pending.push_back(command);
        let predictor = self.predictor.as_mut()?;
        let collision =
            BspMovementCollision::with_movers(bsp, scratch, &self.physics_movers.colliders);
        ride::predict(
            predictor,
            self.rides.committed.as_mut(),
            command,
            &collision,
        );
        self.rides.preview = None;
        touch_triggers(
            predictor,
            &self.physics_movers.triggers,
            bsp,
            self.presentation_time,
            self.movement_config.roll_rules.gametype,
            self.predict_items,
        );
        predictor.emit_events(|event| self.pending_events.push(event));
        self.preview = None;
        self.presented = None;
        self.view_height = predictor.state().view_height as f32;
        Some(Vec3::from_array(predictor.state().origin) + Vec3::Z * self.view_height)
    }

    /// Advance the committed prediction through the input of a frame that
    /// sends no packet, without queueing it. The stock client builds a
    /// usercmd every frame and predicts through all of them
    /// (`cl_input.cpp` `CL_CreateNewCommands`, `cg_predict.c:1124-1237`)
    /// while packets leave at `cl_maxpackets`; the JKR wire sends one
    /// command per packet, so between packets the presented state would
    /// otherwise stand still and then jump. Returns the eye position.
    pub(crate) fn preview_command(
        &mut self,
        command: UserCommand,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
    ) -> Option<Vec3> {
        self.latest_input = Some(command);
        let committed = self.predictor.as_ref()?;
        if command.server_time <= committed.state().command_time {
            return None;
        }
        // `clone_from` reuses the preview's storage: no per-frame allocation.
        let preview = match &mut self.preview {
            Some(preview) => {
                preview.clone_from(committed);
                preview
            }
            None => self.preview.insert(committed.clone()),
        };
        let collision =
            BspMovementCollision::with_movers(bsp, scratch, &self.physics_movers.colliders);
        ride::predict(preview, self.rides.begin_preview(), command, &collision);
        touch_triggers(
            preview,
            &self.physics_movers.triggers,
            bsp,
            self.presentation_time,
            self.movement_config.roll_rules.gametype,
            self.predict_items,
        );
        preview.emit_events(|event| self.pending_events.push(event));
        self.view_height = preview.state().view_height as f32;
        Some(Vec3::from_array(preview.state().origin) + Vec3::Z * self.view_height)
    }

    /// Latest fully constructed input, including mouse movement and strafe modifiers.
    pub(crate) fn guide_input(&self) -> Option<[i8; 2]> {
        self.latest_input.map(|c| [c.forward_move, c.right_move])
    }

    /// Deliver this frame's committed, replayed and preview events without allocating.
    pub(crate) fn drain_events(
        &mut self,
        mut sink: impl FnMut(jkr_client::predicted_events::PredictedEvent),
    ) {
        for event in self.pending_events.iter() {
            sink(event);
        }
        self.pending_events.clear();
    }
}

#[path = "prediction_ride.rs"]
mod ride;
#[path = "prediction_triggers.rs"]
mod triggers;

use triggers::touch_triggers;

/// cg_predict.c:952: only following another player bypasses live prediction.
pub(crate) fn predicts_local_view(movement_flags: u16) -> bool {
    movement_flags & 4096 == 0 // PMF_FOLLOW, not the spectator team/type.
}
