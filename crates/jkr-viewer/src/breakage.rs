//! Breakable presentation: glass shards, debris chunks and model explosions.
//!
//! The stock rules live in [`jkr_client::breakage`], which reproduces OpenJK's
//! `CG_GlassShatter`, `CG_Chunks`, `CG_MiscModelExplosion` and `LE_FRAGMENT`
//! update bit for bit. This runtime adds what only the viewer has: the broken
//! window's BSP faces, shard particles, fragment models, world traces and sounds.
//!
//! Known differences from stock: fragments trace the world but not solid entities
//! (`CG_Trace` also clips movers); resting chunks vanish at the end of their life
//! instead of fading over their last two seconds, because the renderer has no
//! `RF_FORCE_ENT_ALPHA` path yet; and the break sound plays at the break instead of
//! on the freed breakable's entity channel.

use crate::effect_library::EffectLibrary;
use crate::particle_types::{Particle, PrimitiveShape, ShardShape};
use glam::{Quat, Vec3};
use jkr_bsp::{Aabb, Bsp, TraceScratch};
use jkr_client::breakage as rules;
use jkr_protocol::{EntityState, GameState};
use jkr_vfs::VirtualFileSystem;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_GLASS_REQUESTS: usize = 16;
/// `MAX_LOCAL_ENTITIES` is 2048 in stock, shared by every local entity kind.
const MAX_FRAGMENTS: usize = 512;
const MAX_ENTITIES: usize = 1024;
/// Stock `CG_MiscModelExplosion` loops `13 + 7 * size` times; sizes are 0..=2.
const MAX_BURSTS: usize = 48;
/// `SOLID_BMODEL`: the entity's model index is an inline brush model.
const SOLID_BMODEL: u32 = 0x00ff_ffff;
const CONTENTS_SOLID: u32 = 1;
const CONTENTS_NODROP: u32 = 0x8000_0000;

/// Every sound a breakable can start, registered with the map's audio.
pub(crate) const SOUNDS: [&str; 18] = [
    rules::GLASS_BREAK_SOUND,
    "sound/weapons/explosions/glassbreak1",
    "sound/effects/grate_destroy",
    "sound/effects/wall_smash",
    "sound/weapons/explosions/cratebust1",
    "sound/weapons/explosions/cratebust2",
    "sound/weapons/explosions/glasslcar",
    "sound/effects/stone_bounce",
    "sound/effects/stone_bounce2",
    "sound/effects/metal_bounce",
    "sound/effects/metal_bounce2",
    "sound/ambience/spark1.wav",
    "sound/ambience/spark2.wav",
    "sound/ambience/spark3.wav",
    "sound/ambience/spark4.wav",
    "sound/ambience/spark5.wav",
    "sound/ambience/spark6.wav",
    "sound/weapons/explosions/glassbreak1.wav",
];

/// Whether the map places anything that can shatter or break into chunks.
pub(crate) fn map_has_breakables(bsp: &Bsp) -> bool {
    let entities = bsp.entities();
    ["func_glass", "func_breakable", "misc_model_breakable"]
        .iter()
        .any(|name| {
            entities
                .windows(name.len())
                .any(|w| w.eq_ignore_ascii_case(name.as_bytes()))
        })
}

/// Chunk models `CG_RegisterGraphics` loads for every map; here only for maps with
/// breakables, to keep loading lean.
pub(crate) fn extend_appearances(
    bsp: &Bsp,
    appearances: &mut std::collections::BTreeSet<jkr_runtime::Appearance>,
) {
    if !map_has_breakables(bsp) {
        return;
    }
    for kind in rules::ChunkKind::ALL {
        for variant in 0..4 {
            appearances.insert(jkr_runtime::Appearance {
                model: kind.model(variant),
                variant: String::new(),
            });
        }
    }
}

struct GlassRequest {
    event: rules::GlassShatter,
    model: u16,
}

struct Fragment {
    body: rules::FragmentBody,
    model: Arc<str>,
    scale: f32,
    drawn: Option<([f32; 3], [f32; 3])>,
}

/// Map-lifetime breakable state inside [`crate::effect_aux::Runtime`].
pub(crate) struct Runtime {
    random: rules::Random,
    table: rules::GlassTable,
    glass: Vec<GlassRequest>,
    fragments: Vec<Fragment>,
    /// Last inline model drawn by each brush entity number.
    inline_models: Box<[u16]>,
    stock_models: Vec<Arc<str>>,
    clock: Instant,
    last_time: i32,
    sound_serial: u32,
    dropped: u64,
}

impl Default for Runtime {
    fn default() -> Self {
        // Stock seeds `holdrand` with 0x89abcdef and draws the crack table first.
        let mut random = rules::Random::new(0x89ab_cdef, 0x0123_4567);
        let table = rules::GlassTable::new(&mut random);
        Self {
            random,
            table,
            glass: Vec::with_capacity(MAX_GLASS_REQUESTS),
            fragments: Vec::with_capacity(MAX_FRAGMENTS),
            inline_models: vec![0; MAX_ENTITIES].into_boxed_slice(),
            stock_models: rules::ChunkKind::ALL
                .iter()
                .flat_map(|kind| (0..4).map(|variant| Arc::from(kind.model(variant))))
                .collect(),
            clock: Instant::now(),
            last_time: 0,
            sound_serial: 0,
            dropped: 0,
        }
    }
}

/// The stock events this module presents.
pub(crate) fn handles(event: u16) -> bool {
    matches!(
        event,
        rules::EV_GLASS_SHATTER | rules::EV_DEBRIS | rules::EV_MISC_MODEL_EXP
    )
}

impl Runtime {
    /// Remember which inline model each brush entity draws: the window a shatter
    /// event names is freed in the snapshot that carries the event.
    pub(crate) fn observe(&mut self, entity: &EntityState) {
        if entity.solid() == SOLID_BMODEL {
            if let Some(slot) = self.inline_models.get_mut(usize::from(entity.number())) {
                *slot = entity.model_index().max(0) as u16;
            }
        }
    }

    fn time(&self, now: Instant) -> i32 {
        now.saturating_duration_since(self.clock).as_millis() as i32
    }

    fn next_source(&mut self) -> jkr_audio::SourceId {
        self.sound_serial = self.sound_serial.wrapping_add(1);
        jkr_audio::SourceId(0x4000_0000 | (self.sound_serial & 0x3fff_ffff))
    }

    /// `EV_GLASS_SHATTER` waits for the frame update, which has the BSP and camera.
    fn queue_glass(&mut self, entity: &EntityState) {
        let event = rules::GlassShatter::decode(entity);
        let model = self
            .inline_models
            .get(usize::from(event.entity))
            .copied()
            .unwrap_or(0);
        if model == 0 || self.glass.len() == self.glass.capacity() {
            self.dropped += 1;
            return;
        }
        self.glass.push(GlassRequest { event, model });
    }

    /// `EV_DEBRIS`: the break sound, then every chunk as a live fragment.
    fn spawn_debris(
        &mut self,
        entity: &EntityState,
        game_state: &GameState,
        vfs: &VirtualFileSystem,
        audio: &mut Option<crate::GameAudio>,
        now: Instant,
    ) {
        let debris = rules::Debris::decode(entity);
        if let Some(material) = debris.material.filter(|m| *m != rules::Material::None) {
            if let Some(sound) = rules::chunk_break_sound(material, &mut self.random) {
                let source = self.next_source();
                if let Some(audio) = audio.as_mut() {
                    audio.play(vfs, &sound, 1.0, debris.origin, source);
                }
            }
        }
        let custom = (debris.custom_model > 0)
            .then(|| jkr_client::legacy_model_appearance(game_state, debris.custom_model as i16))
            .flatten()
            .map(|appearance| Arc::<str>::from(appearance.model));
        let time = self.time(now);
        let Self {
            random,
            fragments,
            stock_models,
            dropped,
            ..
        } = self;
        rules::chunks(&debris, time, custom.is_some(), random, |fragment| {
            let model = match fragment.model {
                rules::ChunkModel::Custom(_) => custom.clone(),
                rules::ChunkModel::Stock(kind, variant) => {
                    let family = rules::ChunkKind::ALL.iter().position(|k| *k == kind);
                    family.and_then(|f| stock_models.get(f * 4 + usize::from(variant)).cloned())
                }
            };
            let Some(model) = model else { return };
            if fragments.len() == fragments.capacity() {
                *dropped += 1;
                return;
            }
            fragments.push(Fragment {
                body: rules::FragmentBody::new(&fragment, time),
                model,
                scale: fragment.scale,
                drawn: None,
            });
        });
    }

    /// `EV_MISC_MODEL_EXP`: where each burst plays; the caller spawns the effects.
    fn model_explosion(
        &mut self,
        entity: &EntityState,
        bursts: &mut [Option<(&'static str, Vec3, Vec3)>; MAX_BURSTS],
    ) {
        let explosion = rules::ModelExplosion::decode(entity);
        let mut count = 0;
        rules::model_explosion(&explosion, &mut self.random, |name, origin, direction| {
            if let Some(slot) = bursts.get_mut(count) {
                *slot = Some((name, Vec3::from_array(origin), Vec3::from_array(direction)));
                count += 1;
            }
        });
    }

    /// Advance glass requests and fragments; called once per frame with the BSP.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        now: Instant,
        view_forward: Vec3,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
        particles: &mut Vec<Particle>,
        effects: &mut EffectLibrary,
        vfs: &VirtualFileSystem,
        audio: &mut Option<crate::GameAudio>,
    ) {
        let mut glass = std::mem::take(&mut self.glass);
        for request in glass.drain(..) {
            self.shatter(
                &request,
                now,
                view_forward,
                bsp,
                particles,
                effects,
                vfs,
                audio,
            );
        }
        self.glass = glass;
        let time = self.time(now);
        let frame_time = time - self.last_time;
        self.last_time = time;
        if self.fragments.is_empty() {
            return;
        }
        let Self {
            random,
            fragments,
            sound_serial,
            ..
        } = self;
        fragments.retain_mut(|fragment| {
            let frame = fragment
                .body
                .advance(time, frame_time, random, |start, end| {
                    let hit = bsp.trace_box_with(scratch, start, end, Aabb::POINT, CONTENTS_SOLID);
                    rules::FragmentTrace {
                        fraction: hit.fraction,
                        end: hit.end_position,
                        normal: hit.plane.map_or([0.0, 0.0, 1.0], |plane| plane.normal),
                        start_solid: hit.start_solid,
                        all_solid: hit.all_solid,
                        no_drop: hit.fraction < 1.0
                            && bsp.point_contents(hit.end_position, CONTENTS_NODROP)
                                & CONTENTS_NODROP
                                != 0,
                    }
                });
            match frame {
                rules::FragmentFrame::Gone => false,
                rules::FragmentFrame::Hidden => {
                    fragment.drawn = None;
                    true
                }
                rules::FragmentFrame::Draw {
                    origin,
                    angles,
                    sound,
                    ..
                } => {
                    fragment.drawn = Some((origin, angles));
                    if let (Some((path, at)), Some(audio)) = (sound, audio.as_mut()) {
                        *sound_serial = sound_serial.wrapping_add(1);
                        let source =
                            jkr_audio::SourceId(0x4000_0000 | (*sound_serial & 0x3fff_ffff));
                        audio.play(vfs, path, 1.0, at, source);
                    }
                    true
                }
            }
        });
    }

    /// `CG_GlassShatter`: pick the face of the broken window and spawn its shards.
    #[allow(clippy::too_many_arguments)]
    fn shatter(
        &mut self,
        request: &GlassRequest,
        now: Instant,
        view_forward: Vec3,
        bsp: &Bsp,
        particles: &mut Vec<Particle>,
        effects: &mut EffectLibrary,
        vfs: &VirtualFileSystem,
        audio: &mut Option<crate::GameAudio>,
    ) {
        let render = bsp.render();
        let Some(model) = render.inline_model(usize::from(request.model)) else {
            return;
        };
        let vertices = render.vertices();
        let faces = render.surfaces()[model.surfaces.clone()]
            .iter()
            .filter(|surface| matches!(surface.kind, jkr_bsp::SurfaceKind::Planar))
            .filter_map(|surface| {
                let points = vertices.get(surface.vertices.clone())?;
                let last = points.last()?;
                let corner = |i: usize| points.get(i).unwrap_or(last).position;
                Some((
                    [corner(0), corner(1), corner(2), corner(3)],
                    surface.lightmap_vectors[2],
                ))
            });
        let Some(verts) = rules::choose_glass_face(faces, view_forward.to_array()) else {
            return;
        };
        let source = self.next_source();
        if let Some(audio) = audio.as_mut() {
            audio.play(
                vfs,
                rules::GLASS_BREAK_SOUND,
                1.0,
                request.event.damage_point,
                source,
            );
        }
        let shader = effects.shader(rules::SHARD_SHADER);
        let definition = effects.code_primitive_definition();
        let constant = |value| {
            crate::effect_envelope::Envelope::from_values(
                value,
                value,
                0.0,
                jkr_effect::CurveFlags::default(),
            )
        };
        let (alpha_start, alpha_end, alpha_parameter) = rules::SHARD_ALPHA;
        let alpha = crate::effect_envelope::Envelope::from_values(
            alpha_start,
            alpha_end,
            alpha_parameter,
            jkr_effect::CurveFlags {
                modifier: jkr_effect::CurveModifier::NonLinear,
                ..Default::default()
            },
        );
        let dropped = &mut self.dropped;
        rules::shatter(
            &verts,
            &request.event,
            &self.table,
            &mut self.random,
            |shard| {
                if particles.len() == particles.capacity() {
                    *dropped += 1;
                    return;
                }
                let points = shard.points.map(Vec3::from_array);
                let middle = (points[0] + points[1] + points[2] + points[3]) * 0.25;
                let moves_at = now + Duration::from_millis(shard.delay_ms.max(0) as u64);
                particles.push(Particle {
                    motion: crate::particle_motion::Motion::new(
                        middle,
                        Vec3::from_array(shard.velocity),
                        Vec3::from_array(shard.acceleration),
                        0.0,
                        0.0,
                        moves_at,
                    ),
                    spawned_at: now,
                    delay: Duration::ZERO,
                    lifetime: Duration::from_millis(u64::from(rules::SHARD_LIFETIME_MS)),
                    size: constant(1.0),
                    start_length: 1.0,
                    end_length: 1.0,
                    streak: None,
                    normal: None,
                    alpha,
                    use_alpha: true,
                    set_shader_time: false,
                    rgb: [constant(1.0); 3],
                    seed: 0,
                    shader: Arc::clone(&shader),
                    physics: crate::particle_physics::State::new(
                        Arc::clone(&definition),
                        0,
                        0,
                        0.0,
                        jkr_effect::PrimitiveFlags::default(),
                    ),
                    shape: PrimitiveShape::Shard(ShardShape::new(
                        points.map(|p| p - middle),
                        shard.st,
                        [shard.rotation_delta[0], shard.rotation_delta[1]],
                        shard.delay_ms,
                    )),
                });
            },
        );
    }

    /// Draw the live fragments with the shared rigid-model instances.
    pub(crate) fn append_models(
        &self,
        catalog: &crate::effect_emitter::ModelCatalog,
        groups: &mut [Vec<crate::ActorInstance>],
    ) {
        for fragment in &self.fragments {
            let Some((origin, angles)) = fragment.drawn else {
                continue;
            };
            let Some(group) = catalog
                .mesh(&fragment.model)
                .and_then(|index| groups.get_mut(index))
            else {
                continue;
            };
            if group.len() < group.capacity() {
                group.push(crate::ActorInstance::new(
                    origin,
                    jkr_client::legacy_angles_to_quaternion(angles),
                    [fragment.scale; 3],
                ));
            }
        }
    }
}

/// Present one breakable event; `spawn` plays a named effect at an origin and
/// direction (the caller owns the effect runtime this module lives in).
#[allow(clippy::too_many_arguments)]
pub(crate) fn present(
    auxiliary: &mut crate::effect_aux::Runtime,
    event: u16,
    entity: &EntityState,
    game_state: &GameState,
    vfs: &VirtualFileSystem,
    audio: &mut Option<crate::GameAudio>,
    now: Instant,
    mut spawn: impl FnMut(
        &mut crate::effect_aux::Runtime,
        &str,
        Vec3,
        Quat,
        &mut Option<crate::GameAudio>,
    ),
) {
    match event {
        rules::EV_GLASS_SHATTER => auxiliary.breakage.queue_glass(entity),
        rules::EV_DEBRIS => auxiliary
            .breakage
            .spawn_debris(entity, game_state, vfs, audio, now),
        rules::EV_MISC_MODEL_EXP => {
            let mut bursts = [None; MAX_BURSTS];
            auxiliary.breakage.model_explosion(entity, &mut bursts);
            for (name, origin, direction) in bursts.into_iter().flatten() {
                let rotation = crate::combat_effects::rotation_from_direction(direction.to_array());
                spawn(auxiliary, name, origin, rotation, audio);
            }
        }
        _ => {}
    }
}
