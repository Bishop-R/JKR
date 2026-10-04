//! Model-authored audio follows evaluated bones, including the first-person actor.
use crate::{GameAudio, GpuState};
use glam::{Quat, Vec3};
use jkr_client::animation_events::{Cue, Cursor, footsteps};

#[derive(Default)]
pub(crate) struct State {
    cursor: Cursor,
    last_time: Option<i64>,
    identity: Option<jkr_runtime::EntityId>,
    motion_revision: u64,
}

pub(crate) fn update(gpu: &mut GpuState, time: i64, audio: &mut Option<GameAudio>) {
    let Some(audio) = audio else {
        return;
    };
    let paused = gpu.resident.map_change_pending;
    let world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, crate::demo_playback::Session::world);
    let snapshot = crate::first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        time as i32,
    );
    let local = snapshot.map(|snapshot| u64::from(snapshot.player.client_num()) + 1);
    let predicted = gpu.local_prediction.predicted_state();
    for mesh in &mut gpu.actor_meshes {
        if let Some(prefetch) = mesh.preview.event_sounds.take() {
            audio.absorb_animation_prefetch(&prefetch);
        }
        let entity = mesh.entity_id.and_then(|id| world.entity(id));
        let Some(entity) =
            entity.filter(|_| !paused && !mesh.corpse_pool && mesh.animator.requested.is_some())
        else {
            mesh.audio_events.cursor.reset();
            mesh.audio_events.last_time = None;
            continue;
        };
        let revision = entity.sampled_motion_revision(time);
        if mesh.audio_events.motion_revision != revision
            || mesh.audio_events.identity != mesh.entity_id
            || mesh.audio_events.last_time.is_some_and(|old| time < old)
        {
            mesh.audio_events.cursor.reset();
        }
        mesh.audio_events.identity = mesh.entity_id;
        mesh.audio_events.motion_revision = revision;
        mesh.audio_events.last_time = Some(time);
        let id = entity.id.get();
        let is_local = local == Some(id);
        let footstep_class = snapshot
            .and_then(|s| {
                s.entities
                    .binary_search_by_key(&((id - 1) as u16), |e| e.number())
                    .ok()
                    .map(|i| s.entities[i].npc_class())
            })
            .unwrap_or(0);
        let transform = entity.sample(time);
        let origin = if is_local {
            predicted.map_or(transform.translation, |p| p.origin)
        } else {
            transform.translation
        };
        let rotation = mesh
            .render_yaw_degrees
            .map_or(Quat::from_array(transform.rotation), |yaw| {
                Quat::from_rotation_z(yaw.to_radians())
            });
        let frames = mesh.animator.event_frames(&mesh.preview.animation, time);
        let matrices = mesh.animator.matrices();
        mesh.audio_events.cursor.advance(
            &mesh.preview.events,
            &mesh.preview.config,
            frames,
            |cue, variant| match cue {
                Cue::Sound { paths, channel } => {
                    let standard = &paths[variant % paths.len()];
                    let path = mesh.saber_names[0]
                        .as_deref()
                        .and_then(|name| {
                            gpu.saber_hilts
                                .as_ref()?
                                .animation_sound(name, standard, variant)
                        })
                        .unwrap_or(standard);
                    audio.play_animation(path, *channel, false, origin, id, is_local);
                }
                Cue::Footstep { right, heavy } => {
                    if !footsteps::allowed_class(footstep_class) {
                        return;
                    }
                    let bolt = if *right { "*r_leg_foot" } else { "*l_leg_foot" };
                    let Ok(Some(bolt)) = mesh.preview.mesh.surface_bolt_matrix(bolt, 0, matrices)
                    else {
                        return;
                    };
                    let point = Vec3::new(bolt[0][3], bolt[1][3], bolt[2][3])
                        * Vec3::from_array(transform.scale);
                    let start = Vec3::from_array(origin) + rotation * point + Vec3::Z * 15.0;
                    let end = start - Vec3::Z * 32.0;
                    let trace = gpu.bsp.trace_box_with(
                        &mut gpu.trace_scratch,
                        start.to_array(),
                        end.to_array(),
                        jkr_bsp::Aabb::new([-7.0, -7.0, 0.0], [7.0, 7.0, 2.0]).unwrap(),
                        0x1111,
                    );
                    if trace.fraction < 1.0 {
                        let path = footsteps::PATHS[footsteps::material(trace.surface_flags)]
                            [usize::from(*heavy)][variant % 4];
                        audio.play_animation(path, 6, true, origin, id, is_local);
                    }
                }
            },
        );
    }
}
