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

#[path = "seeker_drone.rs"]
pub(crate) mod seeker_drone;

#[path = "jetpack_carrier.rs"]
pub(crate) mod jetpack;

#[path = "special_entities.rs"]
mod special_entities;

/// `bg_itemlist`'s `item_seeker`.
const ITEM_SEEKER: usize = 4;

struct Sinks<'a> {
    flag_meshes: [Option<usize>; 2],
    shield_mesh: Option<usize>,
    /// `models/items/remote.md3`, the seeker drone's (`item_seeker`'s pickup model).
    seeker_mesh: Option<usize>,
    /// The jetpack a carrier wears.
    jetpack: Option<jetpack::JetpackModel>,
    world: &'a jkr_runtime::World,
    actor_meshes: &'a [ActorMesh],
    object_meshes: &'a [StaticModelMesh],
    actor_groups: &'a mut [Vec<ActorInstance>],
    object_groups: &'a mut [Vec<ActorInstance>],
    /// The skeletons of general Ghoul2 entities drawn posed.
    poses: &'a mut object_meshes::posed::Poses,
    overrides: &'a mut Vec<entity_materials::OverrideInstance>,
    entity_instances: &'a mut Vec<EntityInstance>,
    saber_hilts: Option<&'a saber::HiltCatalog>,
    saber_states: &'a mut saber_trail::StateSlab,
    saber_segments: &'a mut saber_trail::SegmentPool,
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
    /// Radians up (the opposite of a Quake pitch).
    camera_pitch: f32,
    view_height: f32,
    predicted_force_powers_active: Option<u32>,
    predicted_local_state: Option<&'a jkr_client::pmove::MovementState>,
    predicted_vehicle: Option<crate::vehicle_pose::Predicted>,
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
    gpu.posed_objects.begin_frame();
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
        seeker_mesh: gpu.pickup_catalog.mesh(ITEM_SEEKER),
        jetpack: gpu.pickup_catalog.jetpack,
        world: active_world,
        actor_meshes: &gpu.actor_meshes,
        object_meshes: &gpu.object_meshes,
        actor_groups: &mut gpu.actor_groups,
        object_groups: &mut gpu.object_groups,
        poses: &mut gpu.posed_objects,
        overrides: &mut gpu.pickup_override_instances,
        entity_instances: &mut gpu.entity_instances,
        saber_hilts: gpu.saber_hilts.as_ref(),
        saber_states: &mut gpu.saber_states,
        saber_segments: &mut gpu.saber_trail_segments,
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
        camera_pitch: gpu.camera_pitch,
        view_height: gpu.local_prediction.view_height(),
        predicted_force_powers_active: gpu
            .local_prediction
            .predicted_state()
            .map(|state| state.force_powers_active),
        predicted_local_state: gpu.local_prediction.predicted_state(),
        predicted_vehicle: gpu
            .live_session
            .as_ref()
            .and_then(|_| gpu.local_prediction.vehicle_pose()),
        authoritative_local_saber_move: snapshot.map(|value| value.player.saber_move()),
        third_person: gpu.third_person,
        portal_view: gpu.scene_views.has_portal_view(),
        entity_view_flags: 0,
        detached_camera: gpu.detached_camera,
    };
    if let Some(snapshot) = snapshot {
        special_entities::submit(&mut sinks, snapshot, visual_now);
    }
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
                trails,
            );
        } else if thrown_saber::submit(&mut sinks, thrown.as_ref(), entity, transform) {
            // The flying hilt is owned by this branch, including its blades.
        } else if let Some(mesh) = entity.appearance().and_then(|appearance| {
            sinks
                .object_meshes
                .iter()
                .position(|mesh| &mesh.appearance == appearance)
        }) {
            let posed = submit_posed(
                &mut sinks,
                mesh,
                entity.id.get(),
                transform,
                (snapshot, game_state),
                visual_now,
            );
            if !posed {
                let rotation = if sinks.object_meshes[mesh].ghoul2 {
                    weapon_view::actor_world_rotation(transform.rotation).to_array()
                } else {
                    transform.rotation
                };
                let mut instance =
                    ActorInstance::new(transform.translation, rotation, transform.scale);
                instance.view_flags = sinks.entity_view_flags;
                sinks.object_groups[mesh].push(instance);
            }
        }
    }
    overlay_count
}

/// The Ghoul2 model of `entity` (object mesh `mesh`) drawn posed, part by part
/// (`CG_General`'s skeleton), and a sentry's muzzle flash played; `false` when the model is
/// not posed or no skeleton is free for it.
fn submit_posed(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    entity: u64,
    transform: jkr_runtime::Transform,
    (snapshot, game_state): (Option<&Snapshot>, Option<&GameState>),
    now: Instant,
) -> bool {
    let Some(model) = sinks.object_meshes[mesh].posed.as_deref() else {
        return false;
    };
    let time = sinks.presentation_time;
    let state = object_meshes::posed::entity_state(snapshot, entity);
    let gunner_view = state
        .and_then(object_meshes::posed::gunner)
        .and_then(|gunner| gunner_view(sinks, snapshot, gunner, time));
    let sentry_enemy = state
        .filter(|state| state.bolt1())
        .and_then(|state| enemy_origin(sinks, snapshot, state.bolt2(), time));
    let draw = object_meshes::posed::Draw {
        mesh,
        model,
        entity,
        state,
        game_state,
        transform,
        view_flags: sinks.entity_view_flags,
        time,
        gunner_view,
        sentry_enemy,
    };
    let Some(drawn) = sinks.poses.submit(draw, sinks.object_groups) else {
        return false;
    };
    if let Some(flash) = drawn.flash {
        // `cgs.effects.mTurretMuzzleFlash`.
        effect_runtime::spawn_effect(
            sinks.particles,
            sinks.effect_aux,
            sinks.effects,
            sinks.vfs,
            "turret/muzzle_flash",
            Vec3::from_array(flash.origin),
            now,
            (entity as u32) | (time as u32).rotate_left(11),
            0,
            sinks.game_audio,
            combat_effects::rotation_from_direction(flash.direction),
        );
    }
    true
}

/// Where an emplaced gun's `gunner` looks (`cg_ents.c:1323-1334`): this client's own view
/// when it is the gunner in first person, otherwise the gunner's presented angles.
fn gunner_view(
    sinks: &Sinks<'_>,
    snapshot: Option<&Snapshot>,
    gunner: u16,
    time: i64,
) -> Option<[f32; 3]> {
    let local = snapshot.is_some_and(|snapshot| snapshot.player.client_num() == gunner);
    if local && !sinks.third_person {
        let pitch = -sinks.camera_pitch.to_degrees();
        return Some([pitch, sinks.camera_yaw.to_degrees(), 0.0]);
    }
    let entity = sinks.world.entity(EntityId::new(u64::from(gunner) + 1))?;
    Some(entity.sample_pose(time)?.view_angles_degrees)
}

/// Where a sentry's enemy `number` stands (`cg_turret.c:213-215` reads its `pos.trBase`):
/// its state in this snapshot, else where this client last placed it; `None` for no enemy
/// (`ENTITYNUM_NONE`) or one it never saw.
fn enemy_origin(
    sinks: &Sinks<'_>,
    snapshot: Option<&Snapshot>,
    number: u16,
    time: i64,
) -> Option<[f32; 3]> {
    let entity = u64::from(number) + 1;
    if number >= 1_023 {
        return None;
    }
    object_meshes::posed::entity_state(snapshot, entity)
        .map(jkr_protocol::EntityState::trajectory_base)
        .or_else(|| {
            Some(
                sinks
                    .world
                    .entity(EntityId::new(entity))?
                    .sample(time)
                    .translation,
            )
        })
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
    trails: bool,
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
        sinks.predicted_vehicle,
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
        seeker_drone::submit(
            sinks,
            entity.kind,
            transform.translation,
            snapshot,
            state,
            local,
            presentation_time,
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
        jetpack::submit(
            sinks, mesh, *transform, snapshot, state, local, draw_actor, visual_now,
        );
    }
    // `EF_DISINTEGRATION`: smoke at the waist for the first second, no weapon or Force
    // shells while it burns, and nothing at all once a player is gone.
    let dissolving = mesh.and_then(|index| {
        sinks.actor_meshes[index]
            .dissolve
            .phase(presentation_time, entity.kind == EntityKind::Corpse)
    });
    if let (Some(index), Some(_)) = (mesh, dissolving) {
        dissolve_smoke(sinks, index, *transform, entity.id.get(), visual_now);
    }
    if dissolving == Some(actor_dissolve::Phase::Gone) {
        return 0;
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
    if let Some((equipment, held_mesh)) = equipment.zip(mesh).filter(|_| dissolving.is_none()) {
        submit_equipment(
            sinks,
            entity,
            equipment,
            held_mesh,
            *transform,
            draw_actor,
            presentation_time,
            visual_now,
            trails,
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
        if let (Some(snapshot), Some(game_state), None) = (snapshot, game_state, dissolving) {
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

/// The disruptor's death smoke this frame for the actor drawn with `mesh`
/// (`cgs.effects.mDisruptorDeathSmoke`, [`actor_dissolve`]).
fn dissolve_smoke(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    transform: jkr_runtime::Transform,
    entity: u64,
    now: Instant,
) {
    let meshes = sinks.actor_meshes;
    let puffs = meshes[mesh].dissolve.puffs();
    let Some(waist) = actor_dissolve::waist(&meshes[mesh], transform).filter(|_| puffs > 0) else {
        return;
    };
    let forward = actor_dissolve::camera_forward(sinks.camera_yaw, sinks.camera_pitch);
    for puff in 0..puffs {
        let seed = (entity as u32) ^ (sinks.presentation_time as u32).rotate_left(7) ^ puff;
        // `Q_flrand(-1, 1)` for the height, from the puff's own seed.
        let height = ((seed.wrapping_mul(2_654_435_761) >> 8) as f32 / (1 << 23) as f32) - 1.0;
        effect_runtime::spawn_effect(
            sinks.particles,
            sinks.effect_aux,
            sinks.effects,
            sinks.vfs,
            "disruptor/death_smoke",
            actor_dissolve::smoke_origin(waist, forward, height),
            now,
            seed,
            0,
            sinks.game_audio,
            combat_effects::rotation_from_direction([0.0, 1.0, 0.0]),
        );
    }
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
    trails: bool,
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
        trails,
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
