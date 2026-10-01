//! Conservative CPU rejection of static draw bounds outside the current camera.
//! Scoped cameras keep mirrors independent of the main view; shadow casters never use it.
use glam::{Mat4, Vec3, Vec4};
use std::cell::Cell;

#[derive(Clone, Copy)]
struct Frustum([Vec4; 6]);

impl Frustum {
    fn new(clip: Mat4) -> Self {
        if !clip.is_finite() {
            return Self([Vec4::W; 6]);
        }
        let rows = clip.transpose();
        Self(
            [
                rows.w_axis + rows.x_axis,
                rows.w_axis - rows.x_axis,
                rows.w_axis + rows.y_axis,
                rows.w_axis - rows.y_axis,
                rows.z_axis,
                rows.w_axis - rows.z_axis,
            ]
            .map(|plane| {
                let length = plane.truncate().length();
                if length > 1e-8 && plane.is_finite() {
                    plane / length
                } else {
                    Vec4::W
                }
            }),
        )
    }

    fn intersects(self, bounds: [Vec3; 2]) -> bool {
        let [low, high] = bounds;
        if !low.is_finite() || !high.is_finite() {
            return true;
        }
        self.0.iter().all(|plane| {
            let normal = plane.truncate();
            let positive = Vec3::select(normal.cmpge(Vec3::ZERO), high, low);
            // Two world units of guard cover floating-point edge reconstruction.
            normal.dot(positive) + plane.w >= -2.0
        })
    }
}

/// Camera scopes and generations owned by one immutable world runtime.
#[derive(Default)]
pub(crate) struct State {
    main: Cell<Option<Frustum>>,
    clip: Cell<Option<Mat4>>,
    active: Cell<Option<(Frustum, u64)>>,
    generation: Cell<u64>,
}

impl State {
    /// Record the exact main-camera matrix uploaded for the next frame.
    pub(crate) fn prepare_main(&self, clip: Mat4) {
        self.main.set(Some(Frustum::new(clip)));
        self.clip.set(Some(clip));
    }

    /// Activate the main camera until the returned guard is dropped.
    pub(crate) fn main(&self) -> Guard<'_> {
        self.enter(self.main.get())
    }

    /// Temporarily activate a reflected camera, restoring its parent on drop.
    pub(crate) fn camera(&self, clip: Mat4) -> Guard<'_> {
        self.enter(Some(Frustum::new(clip)))
    }

    fn enter(&self, next: Option<Frustum>) -> Guard<'_> {
        let generation = self.generation.get().wrapping_add(2).max(2);
        self.generation.set(generation);
        Guard {
            state: self,
            previous: self.active.replace(next.map(|f| (f, generation))),
        }
    }

    /// Identity of the active scoped camera; zero means conservative unculled drawing.
    pub(super) fn active_generation(&self) -> u64 {
        self.active.get().map_or(0, |(_, n)| n)
    }

    /// Reuse a draw's bounds result across depth, lighting, material and AO passes.
    pub(super) fn cached(&self, bounds: [Vec3; 2], cache: &Cache) -> bool {
        let Some((frustum, generation)) = self.active.get() else {
            return true;
        };
        let stored = cache.0.get();
        if stored & !1 == generation {
            return stored & 1 != 0;
        }
        let visible = frustum.intersects(bounds);
        cache.0.set(generation | u64::from(visible));
        visible
    }
}

/// Eight bytes per map-owned draw; never allocates or synchronizes per frame.
#[derive(Clone, Default)]
pub(super) struct Cache(Cell<u64>);

/// Restores the enclosing view even when a render path returns early.
pub(crate) struct Guard<'a> {
    state: &'a State,
    previous: Option<(Frustum, u64)>,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.state.active.set(self.previous);
    }
}
