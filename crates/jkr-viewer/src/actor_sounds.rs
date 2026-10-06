//! Model-authored sound and effect cues follow evaluated bones, including first person.
use crate::{GameAudio, GpuState};
use glam::{Quat, Vec3};
use jkr_client::animation_events::{Cue, Cursor, footsteps};

#[path = "actor_event_effects.rs"]
mod effects;

#[derive(Default)]
pub(crate) struct State {
    cursor: Cursor,
    last_time: Option<i64>,
    identity: Option<jkr_runtime::EntityId>,
    motion_revision: u64,
    effects_loaded: bool,
    effect_weapon: Option<u8>,
    effect_flash: Option<crate::bolt::BoltMatrix>,
}

/// Load the effects `mesh`'s animation events name, once per mesh. Returns whether the
/// library gained any, when the assets built from it must be built again; that is a
/// rebuild of the whole effect atlas, so it is done as the model loads, not as it is
/// first animated (every player of a Siege round spawns in the same frame).
pub(crate) fn preload_effects(
    mesh: &mut crate::actor_mesh::ActorMesh,
    library: &mut crate::effect_library::EffectLibrary,
    vfs: &jkr_vfs::VirtualFileSystem,
) -> bool {
    if mesh.audio_events.effects_loaded {
        return false;
    }
    mesh.audio_events.effects_loaded = true;
    library.preload(vfs, mesh.preview.events.effect_paths())
}

pub(crate) fn update(gpu: &mut GpuState, time: i64, audio: &mut Option<GameAudio>) -> bool {
    let mut effects_changed = false;
    let now = std::time::Instant::now();
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
        if let Some(audio) = audio.as_mut()
            && let Some(prefetch) = mesh.preview.event_sounds.take()
        {
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
        if let Some(vfs) = gpu.vfs.as_deref() {
            effects_changed |= preload_effects(mesh, &mut gpu.effects, vfs);
        }
        let frames = mesh.animator.event_frames(&mesh.preview.animation, time);
        let matrices = mesh.animator.matrices();
        let weapon = entity.equipment().map_or(0, |equipment| equipment.weapon);
        let hilt = (weapon == 3)
            .then(|| {
                Some(
                    gpu.saber_hilts
                        .as_ref()?
                        .get(mesh.saber_names[0].as_deref()?),
                )
            })
            .flatten();
        if mesh.audio_events.effect_weapon != Some(weapon) {
            mesh.audio_events.effect_weapon = Some(weapon);
            mesh.audio_events.effect_flash =
                crate::weapon_view::held_model(weapon).and_then(|model| {
                    gpu.object_meshes
                        .iter()
                        .find(|m| m.appearance.model == model)?
                        .flash_bolt
                });
        }
        let gun_flash = mesh.audio_events.effect_flash;
        let placement = effects::Placement {
            preview: &mesh.preview,
            matrices,
            attachment: mesh.weapon_attachments[0],
            hilt,
            gun_flash,
            origin: Vec3::from_array(origin),
            rotation,
            scale: Vec3::from_array(transform.scale),
        };
        mesh.audio_events.cursor.advance(
            &mesh.preview.events,
            &mesh.preview.config,
            frames,
            |cue, variant| match cue {
                Cue::Sound { paths, channel } => {
                    let Some(audio) = audio.as_mut() else {
                        return;
                    };
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
                    let Some(audio) = audio.as_mut() else {
                        return;
                    };
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
                Cue::Effect { path, bolt } => {
                    let Some(vfs) = gpu.vfs.as_deref() else {
                        return;
                    };
                    if !gpu.effects.contains_definition(path) {
                        return;
                    }
                    crate::effect_runtime::spawn_effect(
                        &mut gpu.particles,
                        &mut gpu.effect_aux,
                        &mut gpu.effects,
                        vfs,
                        path,
                        placement.origin(bolt.as_deref()),
                        now,
                        id as u32 ^ time as u32,
                        0,
                        audio,
                        crate::combat_effects::rotation_from_direction([0.0, 1.0, 0.0]),
                    );
                }
            },
        );
    }
    effects_changed
}
