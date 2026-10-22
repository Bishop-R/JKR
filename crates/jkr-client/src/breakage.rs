//! Breakable presentation from `codemp/cgame/cg_effects.c`: `CG_GlassShatter`,
//! `CG_Chunks` and `CG_MiscModelExplosion`, with `RE_GetBModelVerts`' face choice.
//!
//! Everything here is pure: the viewer supplies geometry and a [`Random`], receives
//! shard, fragment and effect descriptions, and owns drawing, sound and collision.
//! Random draws keep stock's order, so a shared seed reproduces the reference.

use jkr_game_jka::trajectory;
use jkr_protocol::EntityState;

/// `EV_GLASS_SHATTER` (`bg_public.h`).
pub const EV_GLASS_SHATTER: u16 = 81;
/// `EV_DEBRIS`.
pub const EV_DEBRIS: u16 = 82;
/// `EV_MISC_MODEL_EXP`.
pub const EV_MISC_MODEL_EXP: u16 = 83;
/// Protocol-26 field numbers of `genericenemyindex` and `trickedentindex`.
const GENERIC_ENEMY_INDEX: usize = 18;
const TRICKED_ENT_INDEX: usize = 58;
/// `angles2` in protocol-26 field order.
const ANGLES2: [usize; 3] = [82, 51, 84];

/// Stock's shared `flrand`/`irand` generator ([`crate::LegacyRandom`]) plus a second
/// stream for the few places stock calls the C library's `rand()` directly.
#[derive(Clone, Debug)]
pub struct Random {
    hold: crate::LegacyRandom,
    libc: u32,
}

impl Random {
    /// Stock seeds `holdrand` with `0x89abcdef`; the viewer seeds per session.
    /// `libc` must be non-zero.
    pub fn new(hold: u32, libc: u32) -> Self {
        Self {
            hold: crate::LegacyRandom::seeded(hold),
            libc: libc.max(1),
        }
    }

    /// `flrand(min, max)`.
    pub fn float(&mut self, min: f32, max: f32) -> f32 {
        self.hold.flrand(min, max)
    }

    /// `irand(min, max)`, inclusive.
    pub fn integer(&mut self, min: i32, max: i32) -> i32 {
        self.hold.irand(min, max)
    }

    /// `rand() & 1`. Any uniform stream preserves stock behavior; this one is xorshift.
    pub fn coin(&mut self) -> bool {
        self.libc ^= self.libc << 13;
        self.libc ^= self.libc >> 17;
        self.libc ^= self.libc << 5;
        self.libc & 1 != 0
    }
}

/// `material_t` (`q_shared.h:503-524`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Material {
    Metal,
    Glass,
    Electrical,
    ElectricalMetal,
    DarkStone,
    LightStone,
    GlassMetal,
    Metal2,
    None,
    GreyStone,
    Metal3,
    Crate1,
    Grate1,
    Rope,
    Crate2,
    WhiteMetal,
    SnowyRock,
}

impl Material {
    /// The stock enumeration value; out-of-range values have no material.
    pub fn from_index(index: i32) -> Option<Self> {
        use Material::*;
        const ALL: [Material; 17] = [
            Metal,
            Glass,
            Electrical,
            ElectricalMetal,
            DarkStone,
            LightStone,
            GlassMetal,
            Metal2,
            None,
            GreyStone,
            Metal3,
            Crate1,
            Grate1,
            Rope,
            Crate2,
            WhiteMetal,
            SnowyRock,
        ];
        usize::try_from(index)
            .ok()
            .and_then(|i| ALL.get(i).copied())
    }
}

/// `CG_GlassShatter`'s event fields (`cg_event.c:3295-3298`, `g_mover.c:GlassDie`).
#[derive(Clone, Copy, Debug)]
pub struct GlassShatter {
    /// The broken `func_glass`, already freed on the server.
    pub entity: u16,
    /// Where the damage landed (`origin`).
    pub damage_point: [f32; 3],
    /// Direction of the blow (`angles`), stock's `pos2`.
    pub damage_direction: [f32; 3],
    /// `trickedentindex`: hole radius around the damage point.
    pub damage_radius: f32,
    /// `pos.trTime`: zero for no shard limit.
    pub max_shards: i32,
}

impl GlassShatter {
    /// Decode the temporary entity's fields.
    pub fn decode(entity: &EntityState) -> Self {
        Self {
            entity: entity.integer_field(GENERIC_ENEMY_INDEX).unwrap_or(0) as u16,
            damage_point: entity.event_origin(),
            damage_direction: entity.angles(),
            damage_radius: entity.integer_field(TRICKED_ENT_INDEX).unwrap_or(0) as f32,
            max_shards: entity.trajectory_time(),
        }
    }
}

/// `GetQuadArea` (`tr_world.cpp:571-590`): summed squared quarter-cross lengths.
fn quad_area(v: &[[f32; 3]; 4]) -> f32 {
    let half = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| -> f32 {
        let e1 = sub(a, b);
        let e2 = sub(a, c);
        let cross = scale(cross(e1, e2), 0.25);
        dot(cross, cross)
    };
    half(v[0], v[1], v[3]) + half(v[2], v[1], v[3])
}

/// `RE_GetBModelVerts` (`tr_world.cpp:592-666`): the larger of the two biggest faces
/// that faces the viewer, or the biggest. Each face is its first four points and
/// plane normal; areas compare as truncated integers like stock's `int dist`.
pub fn choose_glass_face<I>(faces: I, view_forward: [f32; 3]) -> Option<[[f32; 3]; 4]>
where
    I: IntoIterator<Item = ([[f32; 3]; 4], [f32; 3])>,
{
    let mut best: [Option<([[f32; 3]; 4], [f32; 3])>; 2] = [None, None];
    let mut first = None;
    let mut max = [0_i32; 2];
    for face in faces {
        first.get_or_insert(face);
        let area = x86_int(quad_area(&face.0));
        if area > max[0] {
            max[1] = max[0];
            best[1] = best[0];
            max[0] = area;
            best[0] = Some(face);
        } else if area >= max[1] {
            max[1] = area;
            best[1] = Some(face);
        }
    }
    // Stock's indices default to surface zero when nothing beats an area of zero.
    let one = best[0].or(first)?;
    let two = best[1].or(first)?;
    let dot1 = dot(one.1, view_forward);
    let dot2 = dot(two.1, view_forward);
    Some(if dot2 < dot1 && dot2 < 0.0 {
        two.0
    } else {
        one.0
    })
}

/// `offX`/`offZ` (`cg_effects.c:236-237`), drawn once by `CG_InitGlass`.
///
/// Wide or tall panes index past `offX[20][20]` in stock (undefined behavior; in
/// practice the next statics are read). The two tables are kept as one flat
/// `offX | offZ` block and out-of-range reads wrap around it, so large panes keep
/// irregular cracks instead of straight grid lines.
#[derive(Clone, Debug)]
pub struct GlassTable {
    flat: [f32; 800],
}

impl GlassTable {
    /// `CG_InitGlass` (`:412-425`): note the transposed `offZ` store.
    pub fn new(random: &mut Random) -> Self {
        let mut flat = [0.0; 800];
        for i in 0..20 {
            for t in 0..20 {
                flat[t * 20 + i] = random.float(-1.0, 1.0) * 0.03;
                flat[400 + i * 20 + t] = random.float(-1.0, 1.0) * 0.03;
            }
        }
        Self { flat }
    }

    /// `offX[row][column]`.
    fn x(&self, row: usize, column: usize) -> f32 {
        self.flat[(row * 20 + column) % 800]
    }

    /// `offZ[row][column]`.
    fn z(&self, row: usize, column: usize) -> f32 {
        self.flat[(400 + row * 20 + column) % 800]
    }
}

/// One `CG_DoGlassQuad` poly (`cg_effects.c:244-347`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shard {
    /// World-space corners from the bilinear crack grid.
    pub points: [[f32; 3]; 4],
    /// Texture coordinates: the shard's place on the pane.
    pub st: [[f32; 2]; 4],
    /// Milliseconds the shard stays stuck before it falls (`motionDelay`).
    pub delay_ms: i32,
    /// Initial velocity.
    pub velocity: [f32; 3],
    /// Gravity-like acceleration.
    pub acceleration: [f32; 3],
    /// Elasticity; unused by stock, whose glass polys never trace (`FX_APPLY_PHYSICS`
    /// without `FX_EXPENSIVE_PHYSICS`).
    pub bounce: f32,
    /// Pitch and yaw rotation, degrees per 100 ms of frame time (`CPoly::CalcRotateMatrix`).
    pub rotation_delta: [f32; 3],
}

/// Shard opacity at spawn and end; the fade starts at 85% of life (`FX_ALPHA_NONLINEAR`).
pub const SHARD_ALPHA: (f32, f32, f32) = (0.15, 0.0, 85.0);
/// `killTime`.
pub const SHARD_LIFETIME_MS: u32 = 6_000;
/// `cgs.media.glassShardShader`.
pub const SHARD_SHADER: &str = "gfx/misc/test_crackle";
/// Played at the damage point when glass breaks (`CG_DoGlass`).
pub const GLASS_BREAK_SOUND: &str = "sound/effects/glassbreak1.wav";

/// `CG_DoGlass` (`cg_effects.c:431-617`): tessellate `verts` into falling shards.
/// Shards near the damage point fall at once; the rest wait longer the farther away.
pub fn shatter(
    verts: &[[f32; 3]; 4],
    event: &GlassShatter,
    table: &GlassTable,
    random: &mut Random,
    mut shard: impl FnMut(Shard),
) {
    const DECAY_SLOW: f32 = 0.1;
    const DECAY_MEDIUM: f32 = 0.04;
    const DECAY_FAST: f32 = 0.009;
    let (height, width) = height_width(verts);
    let (step_height, max_height, decay) = if height < 100.0 {
        (0.2_f32, 5_usize, DECAY_SLOW)
    } else if height > 220.0 {
        (0.05, 20, DECAY_FAST)
    } else {
        (0.1, 10, DECAY_MEDIUM)
    };
    // Stock's double arithmetic, narrowed into its float and int locals.
    let step_width = ((0.25 - f64::from(width) * 0.0002) as f32).max(0.01);
    let max_width = ((f64::from(width) * 0.2) as i32).max(5) as usize;
    let decay = (decay + DECAY_FAST) * 0.5;
    let mut count = 0;
    let mut z = 0.0_f32;
    let mut i = 0_usize;
    while z < 1.0 {
        let mut x = 0.0_f32;
        let mut t = 0_usize;
        while x < 1.0 {
            let inner = |t: usize| t > 0 && t < max_width;
            let inner_row = |i: usize| i > 0 && i < max_height;
            let pick =
                |inside: bool, base: f32, offset: f32| if inside { base - offset } else { base };
            let corners = [
                [
                    pick(inner(t), x, table.x(i, t)),
                    pick(inner_row(i), z, table.z(t, i)),
                ],
                [
                    pick(inner(t + 1), x, table.x(i, t + 1)) + step_width,
                    pick(inner_row(i), z, table.z(t + 1, i)),
                ],
                [
                    pick(inner(t + 1), x, table.x(i + 1, t + 1)) + step_width,
                    pick(inner_row(i + 1), z, table.z(t + 1, i + 1)) + step_height,
                ],
                [
                    pick(inner(t), x, table.x(i + 1, t)),
                    pick(inner_row(i + 1), z, table.z(t, i + 1)) + step_height,
                ],
            ];
            let points = bilerp(verts, &corners);
            let mut difference = distance_squared(points[0], event.damage_point) * decay
                - random.float(0.0, 1.0) * 32.0;
            difference -= event.damage_radius * event.damage_radius;
            let (stick, delay_ms) = if difference > 1.0 {
                (true, (difference + random.float(0.0, 1.0) * 200.0) as i32)
            } else {
                (false, 0)
            };
            shard(glass_quad(points, corners, stick, delay_ms, event, random));
            count += 1;
            if event.max_shards != 0 && count >= event.max_shards {
                return;
            }
            x += step_width;
            t += 1;
        }
        z += step_height;
        i += 1;
    }
}

/// `CG_DoGlassQuad`'s random motion (`:244-271`).
fn glass_quad(
    points: [[f32; 3]; 4],
    st: [[f32; 2]; 4],
    stick: bool,
    delay_ms: i32,
    event: &GlassShatter,
    random: &mut Random,
) -> Shard {
    let mut velocity = [random.float(-12.0, 12.0), random.float(-12.0, 12.0), -1.0];
    if !stick {
        for (v, d) in velocity.iter_mut().zip(event.damage_direction) {
            *v += 0.3 * d;
        }
    }
    let acceleration = [0.0, 0.0, -(600.0 + random.float(0.0, 1.0) * 100.0)];
    let bounce = random.float(0.0, 1.0) * 0.2 + 0.15;
    let rotation_delta = [random.float(-40.0, 40.0), random.float(-40.0, 40.0), 0.0];
    Shard {
        points,
        st,
        delay_ms,
        velocity,
        acceleration,
        bounce,
        rotation_delta,
    }
}

/// `CG_CalcHeightWidth` (`:385-406`): mean point-to-edge distances. Stock normalizes
/// the edge in place, so the second distance of each pair uses the unit edge.
fn height_width(v: &[[f32; 3]; 4]) -> (f32, f32) {
    let pair = |edge: [f32; 3], first: [f32; 3], second: [f32; 3]| {
        let mut edge = edge;
        let mut c = cross(edge, first);
        let mut total = normalize(&mut c) / normalize(&mut edge);
        let mut c = cross(edge, second);
        total += normalize(&mut c) / normalize(&mut edge);
        total * 0.5
    };
    let width = pair(sub(v[3], v[0]), sub(v[1], v[0]), sub(v[2], v[0]));
    let height = pair(sub(v[1], v[0]), sub(v[2], v[0]), sub(v[3], v[0]));
    (height, width)
}

/// `CG_CalcBiLerp` (`:349-379`), kept in stock's operation order.
fn bilerp(v: &[[f32; 3]; 4], uv: &[[f32; 2]; 4]) -> [[f32; 3]; 4] {
    uv.map(|[u, w]| {
        let top = madd(scale(v[0], 1.0 - u), u, v[1]);
        let low = scale(top, 1.0 - w);
        let bottom = madd(scale(v[3], 1.0 - u), u, v[2]);
        madd(low, w, bottom)
    })
}

/// `EV_DEBRIS`'s `CG_Chunks` arguments (`cg_event.c:3300-3304`).
#[derive(Clone, Copy, Debug)]
pub struct Debris {
    /// Entity whose sound channel carries the break sound.
    pub owner: u16,
    /// Center the chunks fly away from.
    pub origin: [f32; 3],
    /// Bounding box the chunks start in (`origin2`, `angles2`).
    pub minimums: [f32; 3],
    pub maximums: [f32; 3],
    /// Base launch speed.
    pub speed: f32,
    /// Chunk count (`eventParm`).
    pub count: i32,
    /// Chunk material (`trickedentindex`).
    pub material: Option<Material>,
    /// Map-supplied chunk model index into `CS_MODELS` (`modelindex`), when positive.
    pub custom_model: i32,
    /// Model scale (`apos.trBase[0]`); zero or less means one.
    pub scale: f32,
}

impl Debris {
    /// Decode the temporary entity's fields.
    pub fn decode(entity: &EntityState) -> Self {
        Self {
            owner: entity.owner(),
            origin: entity.event_origin(),
            minimums: entity.origin2(),
            maximums: ANGLES2.map(|f| f32::from_bits(entity.raw_field(f).unwrap_or(0))),
            speed: entity.speed(),
            count: i32::from(entity.event_parameter()),
            material: Material::from_index(entity.integer_field(TRICKED_ENT_INDEX).unwrap_or(-1)),
            custom_model: i32::from(entity.model_index()),
            scale: entity.angular_trajectory_base()[0],
        }
    }
}

/// `CG_MiscModelExplosion`'s fields (`cg_event.c:3306-3309`).
#[derive(Clone, Copy, Debug)]
pub struct ModelExplosion {
    pub minimums: [f32; 3],
    pub maximums: [f32; 3],
    /// Size class (`time`): zero, one or two.
    pub size: i32,
    pub material: Option<Material>,
}

impl ModelExplosion {
    pub fn decode(entity: &EntityState) -> Self {
        Self {
            minimums: entity.origin2(),
            maximums: ANGLES2.map(|f| f32::from_bits(entity.raw_field(f).unwrap_or(0))),
            size: entity.time(),
            material: Material::from_index(i32::from(entity.event_parameter())),
        }
    }
}

/// `CHUNK_*` model families in `cgs.media.chunkModels` (`cg_main.c:1310-1320`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkKind {
    Metal1,
    Metal2,
    Rock1,
    Rock2,
    Rock3,
    Crate1,
    Crate2,
    WhiteMetal,
}

impl ChunkKind {
    /// Every stock family, for preloading.
    pub const ALL: [ChunkKind; 8] = [
        ChunkKind::Metal1,
        ChunkKind::Metal2,
        ChunkKind::Rock1,
        ChunkKind::Rock2,
        ChunkKind::Rock3,
        ChunkKind::Crate1,
        ChunkKind::Crate2,
        ChunkKind::WhiteMetal,
    ];

    /// Model path of variant `0..=3`. Multiplayer swaps the two metal families.
    pub fn model(self, variant: u8) -> String {
        let stem = match self {
            ChunkKind::Metal1 => "metal/metal2",
            ChunkKind::Metal2 => "metal/metal1",
            ChunkKind::Rock1 => "rock/rock1",
            ChunkKind::Rock2 => "rock/rock2",
            ChunkKind::Rock3 => "rock/rock3",
            ChunkKind::Crate1 => "crate/crate1",
            ChunkKind::Crate2 => "crate/crate2",
            ChunkKind::WhiteMetal => "metal/wmetal1",
        };
        format!("models/chunks/{stem}_{}.md3", variant + 1)
    }
}

/// Which model a fragment draws.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkModel {
    /// A `CS_MODELS` entry the map supplied.
    Custom(i32),
    /// A stock chunk family and variant.
    Stock(ChunkKind, u8),
}

/// `leBounceSoundType_t`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BounceSound {
    None,
    Rock,
    Metal,
}

impl BounceSound {
    /// The two variants `CG_FragmentBounceSound` picks between.
    pub fn paths(self) -> Option<[&'static str; 2]> {
        match self {
            BounceSound::None => None,
            BounceSound::Rock => {
                Some(["sound/effects/stone_bounce", "sound/effects/stone_bounce2"])
            }
            BounceSound::Metal => {
                Some(["sound/effects/metal_bounce", "sound/effects/metal_bounce2"])
            }
        }
    }
}

/// One `LE_FRAGMENT` local entity from `CG_Chunks` (`cg_effects.c:922-982`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fragment {
    pub model: ChunkModel,
    /// `TR_GRAVITY` start.
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    /// `TR_LINEAR` tumble in degrees and degrees per second.
    pub angles: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub bounce_factor: f32,
    /// Uniform model scale (`le->radius`).
    pub scale: f32,
    /// `endTime` minus the spawn time.
    pub lifetime_ms: i32,
    pub bounce_sound: BounceSound,
}

/// `SINK_TIME` (`cg_local.h:62`): a resting fragment fades over its last two.
const SINK_TIME_MS: i32 = 1_000;
/// `TR_LINEAR`, `TR_GRAVITY`.
const TR_LINEAR: u8 = 2;
const TR_GRAVITY: u8 = 6;

/// What a fragment's world trace reports (`CG_Trace` with `CONTENTS_SOLID`).
#[derive(Clone, Copy, Debug)]
pub struct FragmentTrace {
    pub fraction: f32,
    pub end: [f32; 3],
    pub normal: [f32; 3],
    pub start_solid: bool,
    pub all_solid: bool,
    /// `CG_PointContents(endpos) & CONTENTS_NODROP`.
    pub no_drop: bool,
}

/// One frame of a fragment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FragmentFrame {
    /// Draw at `origin` with `angles`; `alpha` is 255 until the resting fade.
    Draw {
        origin: [f32; 3],
        angles: [f32; 3],
        alpha: u8,
        /// A bounce sound to start at the impact point.
        sound: Option<(&'static str, [f32; 3])>,
    },
    /// Stuck in solid this frame: nothing to draw, still alive.
    Hidden,
    /// Expired or fell into a no-drop zone.
    Gone,
}

/// A live `LE_FRAGMENT` (`cg_localents.c:157-332`), advanced by `CG_AddLocalEntities`.
#[derive(Clone, Debug)]
pub struct FragmentBody {
    base: [f32; 3],
    delta: [f32; 3],
    start: i32,
    resting: bool,
    angle_base: [f32; 3],
    angle_delta: [f32; 3],
    angle_start: i32,
    origin: [f32; 3],
    angles: [f32; 3],
    end_time: i32,
    bounce_factor: f32,
    bounce_sound: BounceSound,
    alpha: u8,
    forced_alpha: bool,
}

impl FragmentBody {
    /// The local entity `CG_Chunks` allocates at client time `time`. Until its first
    /// tumble it is drawn with the default axis.
    pub fn new(fragment: &Fragment, time: i32) -> Self {
        Self {
            base: fragment.origin,
            delta: fragment.velocity,
            start: time,
            resting: false,
            angle_base: fragment.angles,
            angle_delta: fragment.angular_velocity,
            angle_start: time,
            origin: fragment.origin,
            angles: [0.0; 3],
            end_time: time.wrapping_add(fragment.lifetime_ms),
            bounce_factor: fragment.bounce_factor,
            bounce_sound: fragment.bounce_sound,
            alpha: 0,
            forced_alpha: false,
        }
    }

    /// `CG_AddLocalEntities`' expiry check, then `CG_AddFragment` at `time` after a
    /// frame of `frame_time` milliseconds. `trace` sweeps a point through the world.
    pub fn advance(
        &mut self,
        time: i32,
        frame_time: i32,
        random: &mut Random,
        mut trace: impl FnMut([f32; 3], [f32; 3]) -> FragmentTrace,
    ) -> FragmentFrame {
        if time >= self.end_time {
            return FragmentFrame::Gone;
        }
        if self.resting {
            let left = self.end_time - time;
            if left < SINK_TIME_MS * 2 {
                self.forced_alpha = true;
                let fraction = left as f32 / (SINK_TIME_MS * 2) as f32;
                let mut alpha = ((fraction * 255.0) as i32).clamp(1, 255);
                if self.alpha != 0 && alpha > i32::from(self.alpha) {
                    alpha = i32::from(self.alpha);
                }
                self.alpha = alpha as u8;
            }
            return self.draw(None);
        }
        let target = trajectory::legacy_evaluate_trajectory(
            self.base, self.delta, TR_GRAVITY, self.start, 0, time,
        );
        let hit = trace(self.origin, target);
        if hit.fraction == 1.0 {
            self.origin = target;
            self.angles = trajectory::legacy_evaluate_trajectory(
                self.angle_base,
                self.angle_delta,
                TR_LINEAR,
                self.angle_start,
                0,
                time,
            );
            return self.draw(None);
        }
        if hit.no_drop {
            return FragmentFrame::Gone;
        }
        if hit.start_solid {
            return FragmentFrame::Hidden;
        }
        let sound = self.bounce_sound(random).map(|path| (path, hit.end));
        self.reflect(&hit, time, frame_time);
        self.draw(sound)
    }

    fn draw(&self, sound: Option<(&'static str, [f32; 3])>) -> FragmentFrame {
        FragmentFrame::Draw {
            origin: self.origin,
            angles: self.angles,
            alpha: if self.forced_alpha { self.alpha } else { 255 },
            sound,
        }
    }

    /// `CG_FragmentBounceSound` (`:177-213`): half the bounces sound once; a quiet
    /// bounce silences the fragment half of the time.
    fn bounce_sound(&mut self, random: &mut Random) -> Option<&'static str> {
        if random.coin() {
            let paths = self.bounce_sound.paths()?;
            let sound = paths[random.integer(0, 1) as usize];
            self.bounce_sound = BounceSound::None;
            Some(sound)
        } else {
            if random.coin() {
                self.bounce_sound = BounceSound::None;
            }
            None
        }
    }

    /// `CG_ReflectVelocity` (`:218-244`).
    fn reflect(&mut self, hit: &FragmentTrace, time: i32, frame_time: i32) {
        let hit_time = ((time - frame_time) as f32 + frame_time as f32 * hit.fraction) as i32;
        let velocity = trajectory::legacy_evaluate_trajectory_delta(
            self.delta, TR_GRAVITY, self.start, 0, hit_time,
        );
        let along = dot(velocity, hit.normal);
        self.delta = scale(madd(velocity, -2.0 * along, hit.normal), self.bounce_factor);
        self.base = hit.end;
        self.start = time;
        if hit.all_solid
            || (hit.normal[2] > 0.0
                && (self.delta[2] < 40.0 || self.delta[2] < -(frame_time as f32) * self.delta[2]))
        {
            self.resting = true;
        }
    }
}

/// The break sound `CG_Chunks` starts on the owner's body channel.
pub fn chunk_break_sound(material: Material, random: &mut Random) -> Option<String> {
    use Material::*;
    Some(match material {
        Glass | GlassMetal => "sound/weapons/explosions/glassbreak1".into(),
        Grate1 => "sound/effects/grate_destroy".into(),
        Electrical => format!("sound/ambience/spark{}.wav", random.integer(1, 6)),
        DarkStone | LightStone | GreyStone | WhiteMetal | SnowyRock => {
            "sound/effects/wall_smash".into()
        }
        Crate1 | Crate2 => format!(
            "sound/weapons/explosions/crateBust{}",
            random.integer(0, 1) + 1
        ),
        Metal | Metal2 | Metal3 | ElectricalMetal => "sound/weapons/explosions/glasslcar".into(),
        None | Rope => return Option::None,
    })
}

/// `CG_Chunks` (`cg_effects.c:801-984`) after its break sound: the fragments to spawn.
/// `time` is the client time in milliseconds; `custom_loaded` says whether the
/// map's custom model is available (`cgs.gameModels[customChunk]`).
pub fn chunks(
    debris: &Debris,
    time: i32,
    custom_loaded: bool,
    random: &mut Random,
    mut fragment: impl FnMut(Fragment),
) {
    use Material::*;
    // An out-of-range material takes every switch's `default` branch: no sound,
    // full speed, and only a custom model can be thrown.
    let (bounce_sound, speed_scale) = match debris.material {
        Some(None | Glass | Grate1 | Electrical | Rope) => return,
        Some(DarkStone | LightStone | GreyStone | WhiteMetal | SnowyRock) => {
            (BounceSound::Rock, 0.5)
        }
        Some(GlassMetal) => (BounceSound::Metal, 1.0),
        Some(Metal | Metal2 | Metal3 | ElectricalMetal) => (BounceSound::Metal, 0.8),
        Some(Crate1 | Crate2) | Option::None => (BounceSound::None, 1.0),
    };
    let base_scale = if debris.scale <= 0.0 {
        1.0
    } else {
        debris.scale
    };
    let custom = (debris.custom_model > 0 && custom_loaded)
        .then_some(ChunkModel::Custom(debris.custom_model));
    // Stock's `chunkModel` survives iterations; families draw a fresh variant.
    let mut previous = Option::None;
    for _ in 0..debris.count {
        let model = custom.or_else(|| {
            let kind = match debris.material {
                Some(Metal2) => ChunkKind::Metal2,
                Some(GreyStone) => ChunkKind::Rock1,
                Some(LightStone) => ChunkKind::Rock2,
                Some(DarkStone) => ChunkKind::Rock3,
                Some(SnowyRock) => {
                    if random.integer(0, 1) != 0 {
                        ChunkKind::Rock1
                    } else {
                        ChunkKind::Rock3
                    }
                }
                Some(WhiteMetal) => ChunkKind::WhiteMetal,
                Some(Crate1) => ChunkKind::Crate1,
                Some(Crate2) => ChunkKind::Crate2,
                Some(ElectricalMetal | GlassMetal | Metal) => ChunkKind::Metal1,
                Some(Metal3) => {
                    if random.coin() {
                        ChunkKind::Metal1
                    } else {
                        ChunkKind::Metal2
                    }
                }
                _ => return previous,
            };
            Some(ChunkModel::Stock(kind, random.integer(0, 3) as u8))
        });
        previous = model;
        let Some(model) = model else {
            continue;
        };
        let end_time = ((time + 1300) as f32 + random.float(0.0, 1.0) * 900.0) as i32;
        let origin: [f32; 3] = std::array::from_fn(|j| {
            let r = random.float(0.0, 1.0) * 0.8 + 0.1;
            r * debris.minimums[j] + (1.0 - r) * debris.maximums[j]
        });
        let mut direction = sub(origin, debris.origin);
        normalize(&mut direction);
        let speed = random.float(debris.speed * 0.5, debris.speed * 1.25) * speed_scale;
        let velocity = scale(direction, speed);
        let angles = [
            random.float(0.0, 1.0) * 360.0,
            random.float(0.0, 1.0) * 360.0,
            random.float(0.0, 1.0) * 360.0,
        ];
        let spin = [random.float(-1.0, 1.0), random.float(-1.0, 1.0), 0.0];
        let angular_velocity = scale(spin, random.float(0.0, 1.0) * 600.0 + 200.0);
        let bounce_factor = 0.2 + random.float(0.0, 1.0) * 0.2;
        let scale = random.float(base_scale * 0.75, base_scale * 1.25);
        fragment(Fragment {
            model,
            origin,
            velocity,
            angles,
            angular_velocity,
            bounce_factor,
            scale,
            lifetime_ms: end_time.wrapping_sub(time),
            bounce_sound,
        });
    }
}

/// `CG_MiscModelExplosion` (`cg_effects.c:687-787`): effect name and origin/direction
/// of every burst. The secondary effect is chosen by a coin per burst.
pub fn model_explosion(
    explosion: &ModelExplosion,
    random: &mut Random,
    mut burst: impl FnMut(&'static str, [f32; 3], [f32; 3]),
) {
    use Material::*;
    let Some(material) = explosion.material else {
        return;
    };
    let (effect, secondary, base): (&str, Option<&str>, i32) = match material {
        Glass => ("chunks/glassbreak", Option::None, 5),
        GlassMetal => ("chunks/glassbreak", Some("chunks/metalexplode"), 5),
        Electrical | ElectricalMetal => ("chunks/sparkexplode", Option::None, 5),
        Metal | Metal2 | Metal3 | Crate1 | Crate2 => ("chunks/metalexplode", Option::None, 2),
        Grate1 => ("chunks/grateexplode", Option::None, 8),
        Rope => ("chunks/ropebreak", Option::None, 20),
        WhiteMetal | DarkStone | LightStone | GreyStone | SnowyRock => {
            let effect = if explosion.size == 2 {
                "chunks/rockbreaklg"
            } else {
                "chunks/rockbreakmed"
            };
            (effect, Option::None, 13)
        }
        None => return,
    };
    let count = base + 7 * explosion.size;
    let middle = scale(add(explosion.minimums, explosion.maximums), 0.5);
    for _ in 0..count {
        let origin: [f32; 3] = std::array::from_fn(|j| {
            let r = random.float(0.0, 1.0) * 0.8 + 0.1;
            r * explosion.minimums[j] + (1.0 - r) * explosion.maximums[j]
        });
        let mut direction = sub(origin, middle);
        normalize(&mut direction);
        let name = match secondary {
            Some(second) if random.coin() => second,
            _ => effect,
        };
        burst(name, origin, direction);
    }
}

/// Every effect [`model_explosion`] can play, for preloading.
pub const EXPLOSION_EFFECTS: [&str; 7] = [
    "chunks/glassbreak",
    "chunks/metalexplode",
    "chunks/sparkexplode",
    "chunks/grateexplode",
    "chunks/ropebreak",
    "chunks/rockbreaklg",
    "chunks/rockbreakmed",
];

/// C's float-to-int conversion on x86 (`cvttss2si`): out of range is `INT_MIN`.
fn x86_int(value: f32) -> i32 {
    if value.is_finite() && (-2_147_483_648.0..2_147_483_648.0).contains(&value) {
        value as i32
    } else {
        i32::MIN
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// `VectorMA(a, s, b)`.
fn madd(a: [f32; 3], s: f32, b: [f32; 3]) -> [f32; 3] {
    [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    dot(d, d)
}

/// `VectorNormalize`: returns the length; a zero vector stays zero.
fn normalize(v: &mut [f32; 3]) -> f32 {
    let length = dot(*v, *v).sqrt();
    if length != 0.0 {
        let inverse = 1.0 / length;
        *v = scale(*v, inverse);
    }
    length
}
