//! Allocation-free Raven particle collision and nested-effect scheduling.
//!
//! The expensive branch mirrors `CParticle::UpdateOrigin` in
//! `codemp/client/FxPrimitives.cpp:226-314`. Authored `usePhysics` without
//! `expensivePhysics` deliberately remains kinematic: the reference only
//! enters its trace branch when `FX_EXPENSIVE_PHYSICS` is also set (:230).

use super::*;
use jkr_effect::EffectDefinition;

const CONTENTS_SOLID: u32 = 0x0000_0001;
const CONTENTS_TERRAIN: u32 = 0x0000_1000;
// `codemp/game/bg_public.h:1225` is the mask passed by
// `FxPrimitives.cpp:239-254`.
const MASK_SOLID: u32 = CONTENTS_SOLID | CONTENTS_TERRAIN;
/// `codemp/game/surfaceflags.h`: contents and surface flags shared with
/// decal placement (`R_BoxSurfaces_r`, `tr_marks.cpp:167-171`).
pub(crate) const CONTENTS_FOG: u32 = 0x0000_0008;
pub(crate) const SURF_NOMARKS: u32 = 0x0010_0000;
pub(crate) const SURF_NOIMPACT: u32 = 0x0008_0000;

const MAX_NESTED_REQUESTS: usize = 512;

#[derive(Clone)]
pub(crate) struct State {
    definition: Arc<EffectDefinition>,
    component_index: usize,
    depth: u8,
    elasticity: f32,
    apply: bool,
    expensive: bool,
    kill_on_impact: bool,
    impact_effect: bool,
    death_effect: bool,
    /// A Line's `org2fromTrace` end, not traced yet; `true` also plays its impact effect
    /// where the trace ends (`traceImpactFx`).
    line_trace: Option<bool>,
}

impl State {
    pub(crate) fn new(
        definition: Arc<EffectDefinition>,
        component_index: usize,
        depth: u8,
        elasticity: f32,
        flags: jkr_effect::PrimitiveFlags,
    ) -> Self {
        Self {
            definition,
            component_index,
            depth,
            elasticity,
            apply: flags.apply_physics,
            expensive: flags.expensive_physics,
            kill_on_impact: flags.kill_on_impact,
            impact_effect: flags.impact_runs_effect,
            death_effect: flags.death_runs_effect,
            line_trace: None,
        }
    }

    /// The state of a Line whose end a trace finds (`Some`), playing its impact effect
    /// there when `Some(true)`.
    pub(crate) fn with_line_trace(mut self, line_trace: Option<bool>) -> Self {
        self.line_trace = line_trace;
        self
    }

    /// Whether this is a Line whose end is not traced yet, and so not drawn.
    pub(crate) fn line_trace_pending(&self) -> bool {
        self.line_trace.is_some()
    }
}

#[derive(Clone, Copy)]
enum NestedKind {
    Impact,
    Death,
}

struct NestedRequest {
    definition: Arc<EffectDefinition>,
    component_index: usize,
    kind: NestedKind,
    origin: Vec3,
    normal: Vec3,
    seed: u32,
    depth: u8,
}

pub(crate) struct PendingEffects {
    requests: Vec<NestedRequest>,
    dropped: u64,
    scheduled: u64,
    traces: u64,
}

impl PendingEffects {
    pub(crate) fn new() -> Self {
        Self {
            requests: Vec::with_capacity(MAX_NESTED_REQUESTS),
            dropped: 0,
            scheduled: 0,
            traces: 0,
        }
    }

    fn push(&mut self, particle: &Particle, kind: NestedKind, origin: Vec3, normal: Vec3) {
        self.push_state(&particle.physics, particle.seed, kind, origin, normal);
    }

    fn push_state(
        &mut self,
        state: &State,
        seed: u32,
        kind: NestedKind,
        origin: Vec3,
        normal: Vec3,
    ) {
        if self.requests.len() == self.requests.capacity() {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.requests.push(NestedRequest {
            definition: Arc::clone(&state.definition),
            component_index: state.component_index,
            kind,
            origin,
            normal,
            seed,
            depth: state.depth.saturating_add(1),
        });
        self.scheduled = self.scheduled.saturating_add(1);
    }
}

pub(crate) fn update(
    particles: &mut Vec<Particle>,
    now: Instant,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
    pending: &mut PendingEffects,
) {
    particles.retain_mut(|particle| {
        let age = now.saturating_duration_since(particle.spawned_at);
        if age >= particle.delay + particle.lifetime {
            if particle.physics.death_effect && !particle.physics.kill_on_impact {
                let origin = particle.motion.sample().origin;
                pending.push(
                    particle,
                    NestedKind::Death,
                    origin,
                    deterministic_normal(particle),
                );
            }
            return false;
        }
        if let Some(impact) = particle.physics.line_trace.take() {
            trace_line_end(particle, impact, bsp, scratch, pending);
        }
        if age < particle.delay {
            return true;
        }
        advance_particle(particle, now, bsp, scratch, pending)
    });
}

/// `CFxScheduler::CreateEffect`'s `FX_ORG2_FROM_TRACE` (`FxScheduler.cpp:1408-1436`): a
/// Line ends where a `MASK_SOLID` trace from its origin stops, and with
/// `FX_TRACE_IMPACT_FX` its impact effect plays there along the surface's normal.
fn trace_line_end(
    particle: &mut Particle,
    impact: bool,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
    pending: &mut PendingEffects,
) {
    let Some(reach) = particle.streak else { return };
    let origin = particle.motion.sample().origin;
    pending.traces = pending.traces.wrapping_add(1);
    let trace = bsp.trace_box_with(
        scratch,
        origin.to_array(),
        (origin + reach).to_array(),
        Aabb::POINT,
        MASK_SOLID,
    );
    let end = Vec3::from_array(trace.end_position);
    particle.streak = Some(end - origin);
    if impact {
        let normal = trace.plane.map_or(Vec3::ZERO, |plane| Vec3::from_array(plane.normal));
        pending.push(particle, NestedKind::Impact, end, normal);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_and_spawn(
    particles: &mut Vec<Particle>,
    pending: &mut PendingEffects,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    audio: &mut Option<GameAudio>,
    now: Instant,
    view_forward: Vec3,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
) {
    auxiliary.breakage.update(
        now,
        view_forward,
        bsp,
        scratch,
        particles,
        effects,
        vfs,
        audio,
    );
    auxiliary.emitters.update(now, bsp, scratch, pending);
    while let Some((effect, origin, rotation, seed, depth)) = auxiliary.emitters.pop_emission() {
        crate::effect_runtime::spawn_effect(
            particles, auxiliary, effects, vfs, &effect, origin, now, seed, depth, audio, rotation,
        );
    }
    update(particles, now, bsp, scratch, pending);
    spawn_pending(pending, particles, auxiliary, effects, vfs, audio, now);
}

fn advance_particle(
    particle: &mut Particle,
    now: Instant,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
    pending: &mut PendingEffects,
) -> bool {
    advance_body(
        &mut particle.motion,
        &mut particle.physics,
        particle.seed,
        now,
        bsp,
        scratch,
        pending,
    )
}

pub(crate) fn advance_emitter(
    motion: &mut crate::particle_motion::Motion,
    state: &mut State,
    seed: u32,
    now: Instant,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
    pending: &mut PendingEffects,
) -> bool {
    advance_body(motion, state, seed, now, bsp, scratch, pending)
}

pub(crate) fn schedule_emitter_death(
    state: &State,
    seed: u32,
    origin: Vec3,
    pending: &mut PendingEffects,
) {
    if state.death_effect && !state.kill_on_impact {
        pending.push_state(state, seed, NestedKind::Death, origin, Vec3::Z);
    }
}

fn advance_body(
    motion: &mut crate::particle_motion::Motion,
    state: &mut State,
    seed: u32,
    now: Instant,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
    pending: &mut PendingEffects,
) -> bool {
    if !state.apply || !state.expensive {
        motion.advance(now);
        return true;
    }
    let Some(step) = motion.begin_step(now) else {
        return true;
    };
    pending.traces = pending.traces.wrapping_add(1);
    let trace = bsp.trace_box_with(
        scratch,
        step.previous_origin.to_array(),
        step.predicted_origin.to_array(),
        Aabb::POINT,
        MASK_SOLID,
    );
    apply_trace_body(motion, state, seed, step, trace, pending)
}

fn apply_trace_body(
    motion: &mut crate::particle_motion::Motion,
    state: &mut State,
    seed: u32,
    step: crate::particle_motion::Step,
    trace: jkr_bsp::CollisionTrace,
    pending: &mut PendingEffects,
) -> bool {
    if trace.start_solid || trace.all_solid {
        motion.stop_linear_motion();
        state.apply = false;
        state.impact_effect = false;
        return true;
    }
    if trace.fraction >= 1.0 {
        motion.commit_origin(step.predicted_origin);
        return true;
    }
    let normal = Vec3::from_array(trace.plane.map_or([0.0, 0.0, 1.0], |plane| plane.normal));
    if state.impact_effect && trace.surface_flags & SURF_NOIMPACT == 0 {
        pending.push_state(
            state,
            seed,
            NestedKind::Impact,
            Vec3::from_array(trace.end_position),
            normal,
        );
    }
    if state.kill_on_impact {
        return false;
    }
    // `FxPrimitives.cpp:290-310`: apply the contact-fraction acceleration a
    // second time, reflect `v - 2 dot(v,n)n`, scale by elasticity, then halve
    // elasticity for the next contact. Raven parks particles below speed 10.
    motion.add_acceleration_step(step.seconds * trace.fraction);
    motion.reflect(normal, state.elasticity);
    state.elasticity *= 0.5;
    if motion.sample().velocity.length_squared() < 100.0 {
        motion.stop_linear_motion();
        state.apply = false;
        state.impact_effect = false;
    }
    motion.commit_origin(Vec3::from_array(trace.end_position) + normal);
    true
}

pub(crate) fn spawn_pending(
    pending: &mut PendingEffects,
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    audio: &mut Option<GameAudio>,
    now: Instant,
) {
    for request in pending.requests.drain(..) {
        let component = &request.definition.components[request.component_index];
        let names = match request.kind {
            NestedKind::Impact => &component.impact_effects,
            NestedKind::Death => &component.death_effects,
        };
        for (index, name) in names.iter().enumerate() {
            effect_runtime::spawn_effect(
                particles,
                auxiliary,
                effects,
                vfs,
                name,
                request.origin,
                now,
                request.seed ^ index as u32,
                request.depth,
                audio,
                combat_effects::rotation_from_direction(request.normal.to_array()),
            );
        }
    }
}

fn deterministic_normal(particle: &Particle) -> Vec3 {
    let values = [
        signed_unit(particle.seed),
        signed_unit(particle.seed.wrapping_add(1)),
        signed_unit(particle.seed.wrapping_add(2)),
    ];
    Vec3::from_array(values).normalize_or(Vec3::Z)
}

fn signed_unit(mut seed: u32) -> f32 {
    seed ^= seed >> 16;
    seed = seed.wrapping_mul(0x7feb_352d);
    seed ^= seed >> 15;
    seed = seed.wrapping_mul(0x846c_a68b);
    seed ^= seed >> 16;
    seed as f32 / u32::MAX as f32 * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    const BEAM: &str = "Line\n{\n\tspawnflags\torg2fromTrace traceImpactFx\n\tshaders\n\t[\n\t\t\
        gfx/misc/whiteLine2\n\t]\n\timpactFx\n\t[\n\t\ttest/glow\n\t]\n}\n";
    const GLOW: &str = "Particle\n{\n\tshaders\n\t[\n\t\tgfx/effects/whiteFlare\n\t]\n}\n";

    /// A Line with `org2fromTrace` is not drawn until its first update has traced its end,
    /// and then plays its `impactFx` where the trace stopped (`FxScheduler.cpp:1408-1436`).
    #[test]
    fn traced_lines_find_their_end_and_play_their_impact() {
        let mut vfs = VirtualFileSystem::new();
        let files = [("effects/test/beam.efx", BEAM), ("effects/test/glow.efx", GLOW)];
        vfs.mount_memory("test", files).unwrap();
        let mut particles = Vec::new();
        let mut auxiliary = crate::effect_aux::Runtime::default();
        let mut effects = EffectLibrary::default();
        let now = Instant::now();
        let origin = Vec3::new(10.0, 20.0, 30.0);
        crate::effect_runtime::spawn_effect(
            &mut particles,
            &mut auxiliary,
            &mut effects,
            &vfs,
            "test/beam",
            origin,
            now,
            7,
            0,
            &mut None,
            Quat::IDENTITY,
        );
        assert_eq!(particles.len(), 1);
        assert!(particles[0].physics.line_trace_pending());
        // Nothing to hit in an empty map: the line reaches `FX_MAX_TRACE_DIST`.
        let bsp = Bsp::empty([-20_000.0; 3], [20_000.0; 3]);
        let mut pending = PendingEffects::new();
        update(&mut particles, now, &bsp, &mut TraceScratch::default(), &mut pending);
        assert!(!particles[0].physics.line_trace_pending());
        assert_eq!(particles[0].streak, Some(Vec3::new(16_384.0, 0.0, 0.0)));
        spawn_pending(
            &mut pending,
            &mut particles,
            &mut auxiliary,
            &mut effects,
            &vfs,
            &mut None,
            now,
        );
        assert_eq!(particles.len(), 2);
        let end = origin + Vec3::new(16_384.0, 0.0, 0.0);
        assert_eq!(particles[1].motion.sample().origin, end);
    }
}
