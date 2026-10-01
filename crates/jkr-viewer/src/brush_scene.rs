//! Inline scenery uses the world-material pipeline regardless of entity type.
//! Permanent BSP instances come from baselines rather than network snapshots.
use super::*;
use crate::{GpuState, demo_playback, first_person_view, menu_backdrop};
use jkr_runtime::EntityKind;

pub(crate) fn append_frame(gpu: &mut GpuState, time: i64, now: Instant) {
    let game = gpu
        .live_session
        .as_ref()
        .map(|s| s.game_state())
        .or_else(|| gpu.demo_session.as_ref().map(|s| s.game_state()));
    let snapshot = first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        time as i32,
    );
    append_instances_with_views(
        &gpu.movers,
        &gpu.mover_catalog,
        menu_backdrop::gate_open(gpu, now),
        &mut gpu.mover_groups,
        |number| crate::actor_instance::scene_flags(game, snapshot, number),
    );
    let world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, demo_playback::Session::world);
    for entity in world
        .entities()
        .filter(|entity| entity.kind != EntityKind::Mover)
    {
        let Some(model) = entity.appearance().and_then(|a| inline_number(&a.model)) else {
            continue;
        };
        let Some(mesh) = gpu
            .mover_catalog
            .mesh_by_model
            .get(model)
            .copied()
            .flatten()
        else {
            continue;
        };
        let pose = entity.sample(time);
        let mut instance = ActorInstance::new(pose.translation, pose.rotation, pose.scale);
        instance.view_flags = ActorInstance::WORLD
            | u16::try_from(entity.id.get().saturating_sub(1))
                .ok()
                .map_or(0, |number| {
                    crate::actor_instance::scene_flags(game, snapshot, number)
                });
        gpu.mover_groups[mesh].push(instance);
    }
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

fn inline_number(path: &str) -> Option<usize> {
    let value = path.strip_prefix('*')?.parse::<usize>().ok()?;
    (value != 0).then_some(value)
}
