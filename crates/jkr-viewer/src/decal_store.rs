//! Persistent world decals with the reference's ring-buffer lifetime.
//!
//! Mirrors rd-vanilla `tr_decals.cpp`: two rings (`NORMAL`, `FADE`) of
//! `MAX_DECAL_POLYS` slots, only the first `r_markcount` of which are used
//! (`tr_init.cpp:1617`, default 100). `RE_AllocDecal` (`tr_decals.cpp:87-125`)
//! recycles the head slot and, when that slot's group is older than the
//! current frame, every neighbouring slot from the same frame with it;
//! `RE_FreeDecal` (`tr_decals.cpp:67-81`) moves a normal decal to the fade
//! ring for `DECAL_FADE_TIME` milliseconds. `R_AddDecals`
//! (`tr_decals.cpp:258-323`) emits each live poly per frame, with fading
//! polys' vertex alpha at `255 * (1 - t / DECAL_FADE_TIME)`.
//!
//! Requests arrive from effect spawning without world access and are
//! projected when the frame is prepared, so the queue is a fixed-capacity
//! buffer that drops (and counts) overflow instead of growing.
//!
//! Temporary marks (`CG_ImpactMark(..., temporary = qtrue)`, used by
//! `CG_PlayerShadow`) bypass the rings: cgame clips them with the same
//! `R_MarkFragments` and hands the polys to `R_AddPolysToScene` for the
//! current frame only (`cg_marks.c:131-230`).
//!
//! Marks on models (`actor_marks`) are this frame's polys too, laid by the
//! renderer's gore pass rather than cgame; each draws every stage of its shader.
//!
//! Marks cgame keeps itself (`CG_ImpactMark(..., temporary = qfalse)`, the
//! footprints) live in `cgame_marks` under `cg_marks 1` and in the rings under 2.

use super::*;
use crate::decal_marks::{DecalRequest, DecalScratch, DecalSurfaces, DecalVertex};

/// `MAX_DECAL_POLYS` (`tr_decals.cpp:27`).
const MAX_DECAL_POLYS: usize = 500;
/// `r_markcount` default (`tr_init.cpp:1617`).
const MARK_COUNT: usize = 100;
/// `DECAL_FADE_TIME` (`tr_decals.cpp:47`).
const FADE_TIME_MILLIS: u32 = 1_000;
/// Marks that can wait for one frame's projection pass.
const MAX_PENDING_REQUESTS: usize = 64;
/// Per-frame polys for temporary marks; `MAX_MARK_FRAGMENTS` twice over.
const MAX_TEMPORARY_POLYS: usize = 256;
/// Per-frame polys of marks on models: sixteen large marks on a dozen models.
const MAX_MODEL_MARK_POLYS: usize = 8_192;
/// [`DecalDraw::slot`] of a poly that draws each of its shader's stages in that stage's
/// own blend pass.
pub(crate) const ALL_SLOTS: u8 = u8::MAX;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ring {
    Normal = 0,
    Fade = 1,
}

/// One stored fragment (`decalPoly_t`, `tr_decals.cpp:29-37`).
#[derive(Clone)]
struct DecalPoly {
    /// Frame time the poly was allocated on; zero marks a free slot.
    time: u32,
    /// Zero for normal decals; the end of the fade otherwise.
    fade_time: u32,
    fade_duration: u32,
    shader: Arc<str>,
    /// Blend pipeline slot of the shader, resolved once at projection time.
    slot: u8,
    color: [f32; 4],
    vertices: [DecalVertex; crate::decal_marks::MAX_VERTICES_ON_DECAL],
    count: u8,
}

impl DecalPoly {
    fn fill(&mut self, request: &DecalRequest, slot: u8, vertices: &[DecalVertex]) {
        self.shader = Arc::clone(&request.shader);
        self.slot = slot;
        self.color = request.color;
        self.count = vertices.len() as u8;
        self.vertices[..vertices.len()].copy_from_slice(vertices);
    }

    fn free() -> Self {
        Self {
            time: 0,
            fade_time: 0,
            fade_duration: FADE_TIME_MILLIS,
            shader: Arc::from(""),
            slot: 0,
            color: [0.0; 4],
            vertices: [DecalVertex::default(); crate::decal_marks::MAX_VERTICES_ON_DECAL],
            count: 0,
        }
    }
}

/// Ring buffer state for one decal type (`re_decalPolyHead`/`Total`).
struct DecalRing {
    polys: Vec<DecalPoly>,
    head: usize,
    total: usize,
}

impl DecalRing {
    fn new() -> Self {
        Self {
            polys: vec![DecalPoly::free(); MAX_DECAL_POLYS],
            head: 0,
            total: 0,
        }
    }
}

/// A poly emitted this frame by [`DecalStore::emit`].
pub(crate) struct DecalDraw<'a> {
    pub(crate) shader: &'a Arc<str>,
    pub(crate) slot: u8,
    pub(crate) color: [f32; 4],
    pub(crate) vertices: &'a [DecalVertex],
}

pub(crate) struct DecalStore {
    rings: [DecalRing; 2],
    pending: Vec<DecalRequest>,
    pending_temporary: Vec<DecalRequest>,
    /// Polys of this frame's temporary marks; replaced by every projection.
    temporary: Vec<DecalPoly>,
    /// Polys of this frame's marks on models ([`Self::add_model_mark`]).
    model_marks: Vec<DecalPoly>,
    dropped_requests: usize,
    dropped_model_marks: usize,
    scratch: DecalScratch,
    epoch: Instant,
    /// `cg_marks`; temporary marks bypass it.
    marks: i64,
    cgame: cgame_marks::CgameMarks,
}

impl Default for DecalStore {
    fn default() -> Self {
        Self {
            rings: [DecalRing::new(), DecalRing::new()],
            pending: Vec::with_capacity(MAX_PENDING_REQUESTS),
            pending_temporary: Vec::with_capacity(MAX_PENDING_REQUESTS),
            temporary: Vec::with_capacity(MAX_TEMPORARY_POLYS),
            model_marks: Vec::with_capacity(MAX_MODEL_MARK_POLYS),
            dropped_requests: 0,
            dropped_model_marks: 0,
            scratch: DecalScratch::default(),
            epoch: Instant::now(),
            marks: 1,
            cgame: cgame_marks::CgameMarks::default(),
        }
    }
}

impl DecalStore {
    /// `cg_marks`: at 0 `CG_ImpactMark` returns before queueing and cgame draws none of
    /// its marks, at 2 its marks go to the renderer's decals, otherwise to its own list
    /// (`codemp/cgame/cg_marks.c:146-155,247`). Temporary marks (shadows) are
    /// unaffected. Stock's 0 leaves the effects' decals to `r_markcount`; JKR's 0
    /// removes those as well, which is what the menu's "Wall marks: Off" promises.
    pub(crate) fn set_marks(&mut self, marks: i64) {
        self.marks = marks;
    }

    /// Queue a mark for the next projection pass (`RE_AddDecalToScene`).
    pub(crate) fn request(&mut self, request: DecalRequest) {
        if self.marks == 0 {
            return;
        }
        if self.pending.len() < self.pending.capacity() {
            self.pending.push(request);
        } else {
            self.dropped_requests += 1;
        }
    }

    /// Queue a mark cgame keeps (`CG_ImpactMark` with `temporary` false); `alpha_fade`
    /// fades it by alpha rather than by colour in cgame's own list.
    pub(crate) fn request_cgame(&mut self, request: DecalRequest, alpha_fade: bool) {
        match self.marks {
            0 => {}
            2 => self.request(request),
            _ => {
                if !self.cgame.request(request, alpha_fade) {
                    self.dropped_requests += 1;
                }
            }
        }
    }

    /// Queue a mark that lives for the next frame only.
    pub(crate) fn request_temporary(&mut self, request: DecalRequest) {
        if self.pending_temporary.len() < self.pending_temporary.capacity() {
            self.pending_temporary.push(request);
        } else {
            self.dropped_requests += 1;
        }
    }

    /// Whether any poly would be emitted this frame.
    pub(crate) fn has_polys(&self) -> bool {
        self.live_count() != (0, 0)
            || !self.temporary.is_empty()
            || !self.model_marks.is_empty()
            || !self.cgame.is_empty()
    }

    /// Forget last frame's marks on models, before this frame's are added.
    pub(crate) fn begin_model_marks(&mut self) {
        self.model_marks.clear();
    }

    /// One polygon of a mark on a model, for this frame only, drawn with every stage of
    /// `shader`; past [`MAX_MODEL_MARK_POLYS`] a frame's further polys are dropped.
    pub(crate) fn add_model_mark(&mut self, shader: &Arc<str>, vertices: &[DecalVertex]) {
        let count = vertices
            .len()
            .min(crate::decal_marks::MAX_VERTICES_ON_DECAL);
        if count < 3 || self.model_marks.len() == MAX_MODEL_MARK_POLYS {
            self.dropped_model_marks += 1;
            return;
        }
        let mut poly = DecalPoly {
            time: 0,
            fade_time: 0,
            fade_duration: FADE_TIME_MILLIS,
            shader: Arc::clone(shader),
            slot: ALL_SLOTS,
            color: [1.0; 4],
            vertices: [DecalVertex::default(); crate::decal_marks::MAX_VERTICES_ON_DECAL],
            count: count as u8,
        };
        poly.vertices[..count].copy_from_slice(&vertices[..count]);
        self.model_marks.push(poly);
    }

    /// Live polys in the normal and fade rings.
    pub(crate) fn live_count(&self) -> (usize, usize) {
        (self.rings[0].total, self.rings[1].total)
    }

    /// Project every queued request into the world (`RE_AddDecalToScene`).
    /// `slot_of` resolves a shader's blend pipeline slot. Returns the number
    /// of fragments stored.
    pub(crate) fn project_pending(
        &mut self,
        world: &DecalSurfaces,
        bsp: &Bsp,
        now: Instant,
        slot_of: impl Fn(&str) -> usize,
    ) -> usize {
        let time = self.frame_time(now);
        let mut stored = 0;
        for index in 0..self.pending.len() {
            let request = self.pending[index].clone();
            let slot = slot_of(&request.shader) as u8;
            let Self { rings, scratch, .. } = self;
            crate::decal_marks::project(world, bsp, &request, scratch, |vertices| {
                alloc(rings, Ring::Normal, time).fill(&request, slot, vertices);
                stored += 1;
            });
        }
        self.pending.clear();
        stored += self
            .cgame
            .project_pending(world, bsp, &mut self.scratch, time, &slot_of);
        self.temporary.clear();
        for index in 0..self.pending_temporary.len() {
            let request = self.pending_temporary[index].clone();
            let slot = slot_of(&request.shader) as u8;
            let Self {
                temporary,
                scratch,
                dropped_requests,
                ..
            } = self;
            crate::decal_marks::project(world, bsp, &request, scratch, |vertices| {
                if temporary.len() == temporary.capacity() {
                    *dropped_requests += 1;
                    return;
                }
                let mut poly = DecalPoly::free();
                poly.time = time;
                poly.fill(&request, slot, vertices);
                temporary.push(poly);
                stored += 1;
            });
        }
        self.pending_temporary.clear();
        stored
    }

    /// `R_AddDecals`: hand every live poly to `draw`, retiring finished fades.
    pub(crate) fn emit(&mut self, now: Instant, mut draw: impl FnMut(DecalDraw<'_>)) {
        let time = self.frame_time(now);
        for ring in [Ring::Normal, Ring::Fade] {
            let head = self.rings[ring as usize].head;
            let mut index = head;
            loop {
                self.emit_slot(ring, index, time, &mut draw);
                index = (index + 1) % MARK_COUNT;
                if index == head {
                    break;
                }
            }
        }
        self.cgame.emit(time, self.marks != 0, &mut draw);
        for poly in self.temporary.iter().chain(&self.model_marks) {
            draw(DecalDraw {
                shader: &poly.shader,
                slot: poly.slot,
                color: poly.color,
                vertices: &poly.vertices[..usize::from(poly.count)],
            });
        }
    }

    fn emit_slot(
        &mut self,
        ring: Ring,
        index: usize,
        time: u32,
        draw: &mut impl FnMut(DecalDraw<'_>),
    ) {
        let poly = &self.rings[ring as usize].polys[index];
        if poly.time == 0 {
            return;
        }
        let alpha = if poly.fade_time == 0 {
            poly.color[3]
        } else {
            let elapsed = time.wrapping_sub(poly.time);
            if elapsed >= poly.fade_duration {
                free(&mut self.rings, ring, index, time);
                return;
            }
            1.0 - elapsed as f32 / poly.fade_duration as f32
        };
        draw(DecalDraw {
            shader: &poly.shader,
            slot: poly.slot,
            color: [poly.color[0], poly.color[1], poly.color[2], alpha],
            vertices: &poly.vertices[..usize::from(poly.count)],
        });
    }

    /// Milliseconds since the store was created, never zero (`tr.refdef.time`).
    fn frame_time(&self, now: Instant) -> u32 {
        (now.saturating_duration_since(self.epoch).as_millis() as u32).max(1)
    }
}

/// `RE_AllocDecal`: always succeeds, evicting the head slot's whole
/// same-frame group when it is older than this frame.
fn alloc(rings: &mut [DecalRing; 2], ring: Ring, time: u32) -> &mut DecalPoly {
    let head = rings[ring as usize].head;
    let existing = rings[ring as usize].polys[head].time;
    if existing != 0 {
        if existing != time {
            let mut index = head;
            loop {
                index = (index + 1) % MARK_COUNT;
                if index == head || rings[ring as usize].polys[index].time != existing {
                    break;
                }
                free(rings, ring, index, time);
            }
        }
        free(rings, ring, head, time);
    }
    let state = &mut rings[ring as usize];
    // Retain the slot's shared shader allocation until fill replaces it.
    state.polys[head].fade_time = 0;
    state.polys[head].fade_duration = FADE_TIME_MILLIS;
    state.polys[head].count = 0;
    state.polys[head].time = time;
    state.total += 1;
    state.head = (head + 1) % MARK_COUNT;
    &mut state.polys[head]
}

/// `RE_FreeDecal`: a normal decal moves to the fade ring before it goes.
fn free(rings: &mut [DecalRing; 2], ring: Ring, index: usize, time: u32) {
    if rings[ring as usize].polys[index].time == 0 {
        return;
    }
    if ring == Ring::Normal {
        let mut fading = rings[0].polys[index].clone();
        fading.time = time;
        fading.fade_time = time + FADE_TIME_MILLIS;
        fading.fade_duration = FADE_TIME_MILLIS;
        *alloc(rings, Ring::Fade, time) = fading;
    }
    let state = &mut rings[ring as usize];
    state.polys[index].time = 0;
    state.total -= 1;
}

#[path = "decal_cgame_marks.rs"]
mod cgame_marks;
#[path = "saber_cut_marks.rs"]
pub(crate) mod saber_cuts;
