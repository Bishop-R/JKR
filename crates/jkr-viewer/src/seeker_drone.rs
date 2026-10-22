//! The seeker drone over a player (`CG_Player`, `codemp/cgame/cg_players.c:9124-9188`): the
//! remote's model circling 40 units above them, 20 out and bobbing 5, turned toward its
//! target (`genericenemyindex`) or swaying where it has none. In its last five seconds the
//! index holds its end time past `MAX_GENTITIES`, and it sinks toward its owner as that
//! time nears. NPCs carry no drone.
use super::*;

/// `ET_NPC`, `EF_SEEKERDRONE`, `MAX_GENTITIES`, `ENTITYNUM_NONE`.
const ET_NPC: u8 = 13;
const EF_SEEKERDRONE: u32 = 1 << 21;
const MAX_GENTITIES: i32 = 1_024;
const ENTITYNUM_NONE: i32 = 1_023;
/// `playerState_t::genericEnemyIndex`, `entityState_t::genericenemyindex`.
const PS_GENERIC_ENEMY: usize = 26;
const ES_GENERIC_ENEMY: usize = 18;

/// Where the drone of a player at `origin` is at client time `time`, and how it is turned;
/// `None` for a player without one. `enemy_origin` answers where an entity is drawn.
pub(crate) fn placement(
    origin: [f32; 3],
    flags: u32,
    generic_enemy: i32,
    time: i32,
    enemy_origin: impl Fn(u16) -> Option<[f32; 3]>,
) -> Option<([f32; 3], [f32; 3])> {
    if flags & EF_SEEKERDRONE == 0 && generic_enemy == -1 {
        return None;
    }
    let mut elevated = origin;
    elevated[2] += 40.0;
    // `((cg.time / 12) & 255) * (M_PI * 2) / 255`, a double kept as a float.
    let angle = (f64::from((time / 12) & 255) * (std::f64::consts::PI * 2.0) / 255.0) as f32;
    let (sine, cosine) = (f64::from(angle).sin(), f64::from(angle).cos());
    let dir = [
        (cosine * 20.0) as f32,
        (sine * 20.0) as f32,
        (cosine * 5.0) as f32,
    ];
    let circling: [f32; 3] = std::array::from_fn(|axis| elevated[axis] + dir[axis]);
    let mut position = circling;
    let mut angles = None;
    if generic_enemy > MAX_GENTITIES {
        // An integer quotient, clamped as a float.
        let sink = ((generic_enemy - time) / 80) as f32;
        elevated[2] -= 55.0 - sink.clamp(1.0, 55.0);
        position = std::array::from_fn(|axis| elevated[axis] + dir[axis]);
    } else if generic_enemy != ENTITYNUM_NONE && generic_enemy != -1 {
        if let Some(enemy) = u16::try_from(generic_enemy).ok().and_then(&enemy_origin) {
            let mut toward: [f32; 3] = std::array::from_fn(|axis| enemy[axis] - circling[axis]);
            jkr_game_jka::player_angle_math::normalize(&mut toward);
            angles = Some(jkr_game_jka::player_angle_math::vector_angles(toward));
        }
    }
    let angles = angles.unwrap_or_else(|| {
        let mut yaw = (f64::from(angle) * 180.0 / std::f64::consts::PI + 90.0) as f32;
        if yaw > 360.0 {
            yaw -= 360.0;
        }
        [(sine * 30.0) as f32, yaw, 0.0]
    });
    Some((position, angles))
}

/// The drone of the actor `entity` at `origin`, if it has one, into the rigid-model groups.
#[allow(clippy::too_many_arguments)]
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    kind: EntityKind,
    origin: [f32; 3],
    snapshot: &Snapshot,
    state: Option<&jkr_protocol::EntityState>,
    local: bool,
    time: i64,
) {
    let Some(mesh) = sinks.seeker_mesh else {
        return;
    };
    if kind != EntityKind::Actor {
        return;
    }
    let (flags, generic_enemy) = if local {
        let flags = sinks
            .predicted_local_state
            .map_or(snapshot.player.entity_flags(), |state| state.entity_flags);
        let enemy = snapshot.player.raw_field(PS_GENERIC_ENEMY).unwrap_or(0);
        (flags, enemy as i32)
    } else if let Some(state) = state.filter(|state| state.entity_type() != ET_NPC) {
        let enemy = state.raw_field(ES_GENERIC_ENEMY).unwrap_or(0);
        (state.e_flags(), enemy as i32)
    } else {
        return;
    };
    let world = sinks.world;
    let enemy_origin = |number: u16| {
        world
            .entity(EntityId::new(u64::from(number) + 1))
            .map(|enemy| enemy.sample(time).translation)
    };
    let Some((position, angles)) =
        placement(origin, flags, generic_enemy, time as i32, enemy_origin)
    else {
        return;
    };
    let instance = ActorInstance::new(
        position,
        jkr_client::legacy_angles_to_quaternion(angles),
        [1.0; 3],
    );
    // Map-load reservation covers every pickup model, the remote's among them.
    sinks.object_groups[mesh].push(instance);
}
