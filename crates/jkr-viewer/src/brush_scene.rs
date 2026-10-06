//! Inline BSP models use the world-material pipeline. Like codemp, which
//! draws an inline model only from `CG_Mover`, only `ET_MOVER`s draw one:
//! snapshot movers come from `movers::collect`, permanent BSP instances from
//! baselines rather than network snapshots.
use super::*;
use crate::{GpuState, first_person_view, menu_backdrop};

pub(crate) fn append_frame(gpu: &mut GpuState, time: i64, now: Instant) {
    let game = gpu
        .live_session
        .as_ref()
        .map(|s| s.game_state())
        .or_else(|| gpu.demo_session.as_ref().map(|s| s.game_state()))
        .or_else(|| gpu.resident.scenery.as_ref().map(|(game, _)| game));
    let snapshot = first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        time as i32,
    )
    .or_else(|| gpu.resident.scenery.as_ref().map(|(_, snapshot)| snapshot));
    append_instances_with_views(
        &gpu.movers,
        &gpu.mover_catalog,
        menu_backdrop::gate_open(gpu, now),
        &mut gpu.mover_groups,
        |number| crate::actor_instance::scene_flags(game, snapshot, number),
    );
    if let (Some(game), Some(snapshot)) = (game, snapshot) {
        for state in game.baselines().filter(|state| {
            jkr_client::legacy_permanent_visible(state, snapshot.player.origin())
                && snapshot
                    .entities
                    .binary_search_by_key(&state.number(), |e| e.number())
                    .is_err()
        }) {
            if let Some(mover) = legacy_present_mover(state, time as i32) {
                // Empty props avoid submitting the menu leaves a second time.
                if mover.visible {
                    if let Some(mesh) = gpu
                        .mover_catalog
                        .mesh_by_model
                        .get(mover.model_index)
                        .copied()
                        .flatten()
                    {
                        let mut instance =
                            ActorInstance::new(mover.origin, mover.rotation, [1.0; 3]);
                        instance.view_flags = ActorInstance::WORLD
                            | crate::actor_instance::legacy_render_flags(state);
                        gpu.mover_groups[mesh].push(instance);
                    }
                }
            }
        }
    }
}
