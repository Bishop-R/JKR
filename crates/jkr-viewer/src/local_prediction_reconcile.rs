//! Snapshot replay and endpoint-aware prediction-error measurement.

use super::*;

impl LocalPrediction {
    /// Replay against solids at the prediction snapshot's time, measuring
    /// the old command endpoint even when the pending queue is empty.
    pub(crate) fn apply_snapshot(
        &mut self,
        snapshot: &Snapshot,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
    ) -> Vec3 {
        let player = &snapshot.player;
        let current = (player.entity_flags(), player.client_num());
        let teleported = self
            .previous_player
            .replace(current)
            .is_some_and(|(flags, client)| {
                (flags ^ current.0) & EF_TELEPORT_BIT != 0 || client != current.1
            });
        if teleported {
            // A teleport can skip the old command endpoint entirely. Clear
            // existing view error now, and do not smooth this discontinuity.
            self.error = PredictionErrorDecay::new(self.error.decay_millis());
        }
        let acknowledged = player.command_time();
        let status = CommandStatus::classify(
            acknowledged,
            self.last_acknowledged,
            self.pending
                .iter()
                .map(|cmd| self.movement_config.command_time(cmd.server_time)),
        );
        self.pending
            .retain(|command| command.server_time > acknowledged);
        // Compare committed commands, not the disposable sub-packet preview:
        // a preview's variable-msec integration was never sent to the server.
        let previous = self
            .predictor
            .as_ref()
            .filter(|_| !teleported)
            .map(|predictor| {
                let state = predictor.state();
                (
                    state.command_time,
                    telemetry::Endpoint::new(state),
                    self.render_movers.adjust(
                        state.origin,
                        self.presentation_ground(state),
                        &self.physics_movers,
                        self.presentation_time,
                    ),
                )
            });
        self.physics_movers.update(snapshot, self.presentation_time);
        self.physics_movers
            .present_boxes(&self.render_movers, self.presentation_time);
        self.last_acknowledged = acknowledged;
        let seed_ground =
            self.physics_movers
                .supported_ground(bsp, player.origin(), player.ground_entity_num());
        self.seed_ground = Some(seed_ground);
        let mut predictor = self.predictor.as_ref().map_or_else(
            || Predictor::from_player_state(player, self.movement_config),
            |old| old.reseed_player_state(player, self.movement_config),
        );
        if let Some(lengths) = &self.animation_lengths {
            predictor.set_animation_lengths(Arc::clone(lengths));
        }
        self.rides
            .seed(snapshot, acknowledged, self.movement_config);
        let collision =
            BspMovementCollision::with_movers(bsp, scratch, &self.physics_movers.colliders);
        let mut miss_units = 0.0;
        let server = telemetry::Endpoint::new(predictor.state());
        let mut endpoints = None;
        let mut counts = telemetry::Counts::default();
        // Stock checks before the next command. JKR receives snapshots before
        // creating that frame's command, so the final replay endpoint is also
        // a comparison boundary (including a fully acknowledged empty queue).
        let mut measure = |state: &MovementState| {
            if let Some((time, old, old_origin)) = previous
                && state.command_time == time
            {
                let adjusted = self.render_movers.adjust(
                    state.origin,
                    if state.command_time == acknowledged {
                        seed_ground
                    } else {
                        state.ground_entity_number
                    },
                    &self.physics_movers,
                    self.presentation_time,
                );
                miss_units = Vec3::from_array(old_origin).distance(adjusted.into());
                endpoints = Some((old, telemetry::Endpoint::new(state)));
                self.error.record_miss(
                    old_origin,
                    adjusted,
                    self.frame_millis,
                    self.previous_frame_millis,
                );
            }
        };
        measure(predictor.state());
        for command in &self.pending {
            let deferred = predictor.state().saber_special_deferred;
            ride::predict(
                &mut predictor,
                self.rides.committed.as_mut(),
                *command,
                &collision,
            );
            let state = predictor.state();
            counts.replayed += 1;
            counts.deferred += u64::from(state.saber_deferred_active);
            counts.frozen += u64::from(state.input_freeze_active);
            counts.deferred_slices += u64::from(state.saber_special_deferred - deferred);
            touch_triggers(
                &mut predictor,
                &self.physics_movers.triggers,
                bsp,
                self.presentation_time,
                self.movement_config.roll_rules.gametype,
                self.predict_items,
            );
            predictor.emit_events(|event| self.pending_events.push(event));
            measure(predictor.state());
        }
        if let Some((old, replay)) = endpoints {
            self.miss_log
                .observe(miss_units, old, replay, server, &mut counts);
        }
        self.view_height = predictor.state().view_height as f32;
        let position = Vec3::from_array(predictor.state().origin) + Vec3::Z * self.view_height;
        self.predictor = Some(predictor);
        self.preview = None;
        self.presented = None;
        self.last_sample = PredictionSample {
            status,
            pending: self.pending.len(),
            miss_units,
            counts,
        };
        position
    }

    /// Carry only the display copy to the exact time used to draw platforms.
    /// The raw predictor stays at physicsTime for subsequent command replay.
    pub(crate) fn present_frame(&mut self, snapshot: &Snapshot, time: i32) -> Option<Vec3> {
        self.presentation_time = time;
        self.render_movers.update(snapshot, time);
        self.physics_movers.set_angle_time(time);
        self.physics_movers.present_boxes(&self.render_movers, time);
        let raw = self.preview.as_ref().or(self.predictor.as_ref())?.state();
        let mut presented = raw.clone();
        if raw.team != 3 && raw.movement_type != 4 {
            presented.origin = self.render_movers.adjust(
                raw.origin,
                self.presentation_ground(raw),
                &self.physics_movers,
                time,
            );
        }
        let eye = Vec3::from_array(presented.origin) + Vec3::Z * presented.view_height as f32;
        self.presented = Some(presented);
        Some(eye)
    }

    fn presentation_ground(&self, state: &MovementState) -> u16 {
        if state.command_time == self.last_acknowledged {
            self.seed_ground.unwrap_or(state.ground_entity_number)
        } else {
            state.ground_entity_number
        }
    }
}
