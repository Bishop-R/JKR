//! Actor, held-weapon, saber, and force-overlay frame submission.

use super::*;

#[path = "flag_carrier.rs"]
pub(crate) mod flags;

#[path = "force_power_submission.rs"]
mod force_powers;

#[path = "thrown_saber.rs"]
mod thrown_saber;

#[path = "actor_model_scale.rs"]
pub(crate) mod model_scale;

#[path = "monster_hold.rs"]
pub(crate) mod monster_hold;

#[path = "player_sprites.rs"]
mod player_sprites;

struct Sinks<'a> {
    flag_meshes: [Option<usize>; 2],
    shield_mesh: Option<usize>,
    world: &'a jkr_runtime::World,
    actor_meshes: &'a [ActorMesh],
    object_meshes: &'a [StaticModelMesh],
    actor_groups: &'a mut [Vec<ActorInstance>],
    object_groups: &'a mut [Vec<ActorInstance>],
    overrides: &'a mut Vec<entity_materials::OverrideInstance>,
    entity_instances: &'a mut Vec<EntityInstance>,
    saber_hilts: Option<&'a saber::HiltCatalog>,
    saber_states: &'a mut saber_trail::StateSlab,
    saber_segments: &'a mut saber_trail::SegmentPool,
    /// Trail edges for held and flying blades; `None` with `cg_saberTrail 0`.
    trail_edges: Option<saber_trail::Edges<'a>>,
    saber_instances: &'a mut Vec<saber::Instance>,
    lights: &'a mut dynamic_lights::PointLightList,
    presentation_time: i64,
    muzzle_effects: &'a jkr_client::LegacyMuzzleEffects,
    particles: &'a mut Vec<Particle>,
    effect_aux: &'a mut effect_aux::Runtime,
    effects: &'a mut EffectLibrary,
    vfs: &'a VirtualFileSystem,
    game_audio: &'a mut Option<GameAudio>,
    force_tracker: &'a jkr_client::LegacyForceOverlayTracker,
    material_overrides: model_materials::Overrides,
    camera_position: Vec3,
    camera_yaw: f32,
    view_height: f32,
    predicted_force_powers_active: Option<u32>,
    predicted_local_state: Option<&'a jkr_client::pmove::MovementState>,
    authoritative_local_saber_move: Option<u32>,
    third_person: bool,
    portal_view: bool,
    entity_view_flags: u32,
    detached_camera: bool,
}

/// Submit all presented actor-like entities without allocating frame storage.
pub(crate) fn submit(
    gpu: &mut GpuState,
    local_entity_id: Option<u64>,
    fallback_mesh: Option<usize>,
    presentation_time: i64,
    visual_now: Instant,
    game_audio: &mut Option<GameAudio>,
) -> usize {
    let snapshot = first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        presentation_time as i32,
    );
    let active_world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, demo_playback::Session::world);
    let game_state = gpu
        .live_session
        .as_ref()
        .map(ClientSession::game_state)
        .or_else(|| {
            gpu.demo_session
                .as_ref()
                .map(demo_playback::Session::game_state)
        });
    let aura_shell = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_auraShell"))
        .unwrap_or(1)
        != 0;
    let trails = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_saberTrail"))
        .unwrap_or(1)
        != 0;
    let saber_contact = gpu.effect_aux.saber_contacts.enabled;
    let mut sinks = Sinks {
        flag_meshes: gpu.pickup_catalog.carrier_meshes[flags::model_set(
            game_state
                .and_then(|game| game.config_string(0))
                .and_then(|info| jkr_client::LegacyClientInfo::new(info).integer("g_gametype"))
                .unwrap_or(0),
        )],
        shield_mesh: gpu
            .object_meshes
            .iter()
            .position(|mesh| mesh.appearance.model == "models/weaphits/testboom.md3"),
        world: active_world,
        actor_meshes: &gpu.actor_meshes,
        object_meshes: &gpu.object_meshes,
        actor_groups: &mut gpu.actor_groups,
        object_groups: &mut gpu.object_groups,
        overrides: &mut gpu.pickup_override_instances,
        entity_instances: &mut gpu.entity_instances,
        saber_hilts: gpu.saber_hilts.as_ref(),
        saber_states: &mut gpu.saber_states,
        saber_segments: &mut gpu.saber_trail_segments,
        trail_edges: trails.then(|| {
            saber_trail::Edges::new(saber_contact.then_some((&gpu.bsp, &mut gpu.trace_scratch)))
        }),
        saber_instances: &mut gpu.saber_instances,
        lights: &mut gpu.dynamic_lights,
        presentation_time,
        muzzle_effects: &gpu.muzzle_effects,
        particles: &mut gpu.particles,
        effect_aux: &mut gpu.effect_aux,
        effects: &mut gpu.effects,
        vfs: gpu
            .vfs
            .as_deref()
            .expect("sessions retain their mounted VFS"),
        game_audio,
        force_tracker: &gpu.force_overlays,
        material_overrides: gpu.model_material_overrides,
        camera_position: gpu.camera_position,
        camera_yaw: gpu.camera_yaw,
        view_height: gpu.local_prediction.view_height(),
        predicted_force_powers_active: gpu
            .local_prediction
            .predicted_state()
            .map(|state| state.force_powers_active),
        predicted_local_state: gpu.local_prediction.predicted_state(),
        authoritative_local_saber_move: snapshot.map(|value| value.player.saber_move()),
        third_person: gpu.third_person,
        portal_view: gpu.scene_views.has_portal_view(),
        entity_view_flags: 0,
        detached_camera: gpu.detached_camera,
    };
    let thrown = snapshot
        .zip(game_state)
        .map(|(snapshot, game)| jkr_client::LegacyThrownSabers::new(snapshot, game));
    let mut overlay_count = 0;
    for entity in active_world
        .entities()
        .filter(|entity| {
            !matches!(
                entity.kind,
                EntityKind::Projectile | EntityKind::Mover | EntityKind::Item | EntityKind::Effect
            )
        })
        .take(1_024)
    {
        sinks.entity_view_flags = u16::try_from(entity.id.get().saturating_sub(1))
            .ok()
            .map_or(0, |number| {
                crate::actor_instance::scene_flags(game_state, snapshot, number)
            });
        let mut transform = entity.sample(presentation_time);
        if matches!(entity.kind, EntityKind::Actor | EntityKind::Corpse) {
            overlay_count += submit_actor(
                &mut sinks,
                entity,
                &mut transform,
                snapshot,
                game_state,
                local_entity_id,
                fallback_mesh,
                presentation_time,
                visual_now,
                aura_shell,
            );
        } else if thrown_saber::submit(&mut sinks, thrown.as_ref(), entity, transform) {
            // The flying hilt is owned by this branch, including its blades.
        } else if let Some(mesh) = entity.appearance().and_then(|appearance| {
            sinks
                .object_meshes
                .iter()
                .position(|mesh| &mesh.appearance == appearance)
        }) {
            let mut instance =
                ActorInstance::new(transform.translation, transform.rotation, transform.scale);
            instance.view_flags = sinks.entity_view_flags;
            sinks.object_groups[mesh].push(instance);
        }
    }
    overlay_count
}

#[allow(clippy::too_many_arguments)]
fn submit_actor(
    sinks: &mut Sinks<'_>,
    entity: &jkr_runtime::SceneEntity,
    transform: &mut jkr_runtime::Transform,
    snapshot: Option<&Snapshot>,
    game_state: Option<&GameState>,
    local_entity_id: Option<u64>,
    fallback_mesh: Option<usize>,
    presentation_time: i64,
    visual_now: Instant,
    aura_shell: bool,
) -> usize {
    let draw_actor = sinks.third_person || Some(entity.id.get()) != local_entity_id;
    if Some(entity.id.get()) == local_entity_id && !sinks.detached_camera {
        let (translation, rotation) =
            camera::local_actor_root(sinks.camera_position, sinks.view_height, sinks.camera_yaw);
        transform.translation = translation;
        transform.rotation = rotation;
    }
    let mesh = sinks
        .actor_meshes
        .iter()
        .position(|mesh| mesh.entity_id == Some(entity.id))
        .or_else(|| {
            (entity.kind != EntityKind::Corpse)
                .then_some(fallback_mesh)
                .flatten()
        });
    transform.rotation = actor_pose::world_rotation(
        mesh.and_then(|index| sinks.actor_meshes.get(index)),
        transform.rotation,
    );
    let state = snapshot.and_then(|snapshot| {
        snapshot
            .entities
            .iter()
            .find(|state| u64::from(state.number()) + 1 == entity.id.get())
    });
    crate::vehicle_pose::place(
        sinks.world,
        sinks.actor_meshes,
        mesh,
        entity,
        transform,
        snapshot,
        state,
        Some(entity.id.get()) == local_entity_id,
        presentation_time,
    );
    if let Some(state) = state {
        model_scale::apply(transform, state.model_scale_percent(), state.npc_class());
    }
    // A monster's victim is drawn in its hand or jaw (`cg_players.c:9220-9244`).
    let local = Some(entity.id.get()) == local_entity_id;
    if let Some(snapshot) = snapshot.filter(|_| local || state.is_some()) {
        monster_hold::place(
            sinks.world,
            sinks.actor_meshes,
            snapshot,
            state.filter(|_| !local),
            transform,
            presentation_time,
        );
    }
    if let Some(snapshot) = snapshot {
        player_sprites::submit(
            sinks,
            entity.kind,
            transform.translation,
            snapshot,
            state,
            local,
            draw_actor,
            visual_now,
        );
    }
    if let (Some(mesh), Some(snapshot)) = (mesh, snapshot) {
        flags::submit(
            sinks,
            mesh,
            entity,
            *transform,
            snapshot,
            presentation_time,
            draw_actor,
        );
        force_powers::submit(
            sinks,
            mesh,
            entity,
            *transform,
            snapshot,
            presentation_time as i32,
            visual_now,
        );
    }
    let mut equipment = if Some(entity.id.get()) == local_entity_id {
        local_actor_state::equipment(
            entity.equipment(),
            sinks.predicted_local_state,
            sinks.authoritative_local_saber_move,
        )
    } else {
        entity.equipment()
    };
    if let Some(body) = mesh.and_then(|index| sinks.actor_meshes[index].body_identity.as_ref()) {
        // CG_BodyQueueCopy removes dropped weapons above WP_BRYAR_PISTOL.
        equipment = equipment
            .filter(|_| (1..=4).contains(&body.weapon))
            .map(|mut held| {
                held.weapon = body.weapon as u8;
                held.kind = if body.weapon == 3 {
                    jkr_runtime::HeldItemKind::EnergyBlade
                } else {
                    jkr_runtime::HeldItemKind::Ranged
                };
                held.active = false;
                held.secondary_active = false;
                held.primary_in_flight = false;
                held
            });
    }
    if let Some((equipment, held_mesh)) = equipment.zip(mesh) {
        submit_equipment(
            sinks,
            entity,
            equipment,
            held_mesh,
            *transform,
            draw_actor,
            presentation_time,
            visual_now,
        );
    }
    if (draw_actor || sinks.portal_view)
        && let Some(mesh) = mesh
    {
        let mut instance = ActorInstance::new(
            transform.translation,
            weapon_view::actor_world_rotation(transform.rotation).to_array(),
            transform.scale,
        )
        .with_entity_color(entity.color());
        instance.view_flags = sinks.entity_view_flags | u32::from(!draw_actor);
        sinks.actor_groups[mesh].push(instance);
        sinks.actor_meshes[mesh]
            .retained_pose
            .mark_drawn(entity.id, presentation_time);
        if let (Some(snapshot), Some(game_state)) = (snapshot, game_state) {
            return force_overlay_submission::submit(
                sinks.overrides,
                mesh,
                instance,
                entity.id,
                snapshot,
                game_state,
                sinks.force_tracker,
                sinks.material_overrides,
                presentation_time as i32,
                sinks.third_person,
                aura_shell,
                sinks.predicted_force_powers_active,
                sinks.shield_mesh,
            );
        }
    } else if draw_actor {
        sinks.entity_instances.push(EntityInstance {
            position: transform.translation,
            kind: 0,
            size: 1.0,
            alpha: 1.0,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            direction: [0.0; 3],
            rotation: 0.0,
            uv_transform: [1.0, 1.0, 0.0, 0.0],
        });
    }
    0
}

#[allow(clippy::too_many_arguments)]
fn submit_equipment(
    sinks: &mut Sinks<'_>,
    entity: &jkr_runtime::SceneEntity,
    equipment: jkr_runtime::HeldEquipment,
    actor_mesh: usize,
    transform: jkr_runtime::Transform,
    draw_actor: bool,
    presentation_time: i64,
    visual_now: Instant,
) {
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let origin = Vec3::from_array(transform.translation);
    if saber_submission::submit(
        entity.id.get(),
        std::array::from_fn(|hand| {
            let mesh = &sinks.actor_meshes[actor_mesh];
            mesh.saber_names[hand]
                .as_deref()
                .zip(mesh.weapon_attachments[hand])
        }),
        equipment,
        origin,
        rotation,
        sinks.saber_hilts,
        sinks.saber_states,
        sinks.saber_segments,
        sinks.object_groups,
        sinks.saber_instances,
        presentation_time,
        sinks.trail_edges.as_mut(),
        sinks.lights,
    ) {
        return;
    }
    // The local first-person player's model carries `RF_THIRD_PERSON`
    // (`cg_players.c:8901-8911`, sabers excepted), so its bolted world gun is
    // not drawn and the world flash is skipped (`cg_weapons.c:701-703`); the
    // view model and its `tag_flash` stand in (`first_person_weapon.rs`).
    if !draw_actor && !sinks.portal_view {
        return;
    }
    let Some(attachment) = sinks.actor_meshes[actor_mesh].weapon_attachments[0] else {
        return;
    };
    let Some(model) = weapon_view::held_model(equipment.weapon) else {
        return;
    };
    let (grip, weapon_rotation) = saber::world_attachment(origin, rotation, attachment);
    let Some(weapon_mesh) = sinks
        .object_meshes
        .iter()
        .position(|mesh| mesh.appearance.variant.is_empty() && mesh.appearance.model == model)
    else {
        return;
    };
    let mut instance = ActorInstance::new(grip.to_array(), weapon_rotation.to_array(), [1.0; 3]);
    instance.view_flags = sinks.entity_view_flags | u32::from(!draw_actor);
    sinks.object_groups[weapon_mesh].push(instance);
    // The main first-person flash already owns event/audio spawning. A
    // second view must never advance effects or play a second sound.
    if !draw_actor {
        return;
    }
    let client = u16::try_from(entity.id.get().saturating_sub(1)).ok();
    let request = client.and_then(|client| sinks.muzzle_effects.request(client));
    let socket = sinks.object_meshes[weapon_mesh]
        .flash_bolt
        .and_then(|flash| muzzle_flash::world_socket(flash, grip, weapon_rotation));
    if let (Some(request), Some(socket)) = (request, socket) {
        muzzle_effects::spawn(
            sinks.particles,
            sinks.effect_aux,
            sinks.effects,
            sinks.vfs,
            sinks.game_audio,
            request,
            socket,
            visual_now,
            presentation_time as i32,
        );
    }
}
