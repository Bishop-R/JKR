//! cgame's own marks (`codemp/cgame/cg_marks.c`): what `CG_ImpactMark` keeps under
//! `cg_marks 1`, in MP only the footprints. Up to `MAX_MARK_POLYS` clipped polys, each
//! drawn for `MARK_TOTAL_TIME` and faded over its last `MARK_FADE_TIME`, by alpha for an
//! `alphaFade` mark and by colour otherwise (`CG_AddMarks`). A full list makes room by
//! freeing the oldest frame's polys (`CG_AllocMark`).
//!
//! Stock keeps the mark's 0–1 colour and copies it into the polys' byte colours every
//! frame, so a mark draws at a 255th of its colour: black, which the stock footprint
//! textures are anyway. JKR keeps that.

use super::{DecalDraw, DecalPoly, DecalRequest, DecalScratch, DecalSurfaces};
use jkr_bsp::Bsp;
use std::collections::VecDeque;
use std::sync::Arc;

/// `MAX_MARK_POLYS` (`cg_local.h:71`).
const MAX_MARK_POLYS: usize = 256;
/// `MARK_TOTAL_TIME` (`cg_marks.c:238`), in milliseconds.
const MARK_TOTAL_TIME: u32 = 10_000;
/// `MARK_FADE_TIME` (`cg_marks.c:239`), in milliseconds.
const MARK_FADE_TIME: u32 = 1_000;
/// Marks that can wait for one frame's projection pass.
const MAX_PENDING: usize = 64;

struct Mark {
    poly: DecalPoly,
    alpha_fade: bool,
}

/// The list, oldest first, and the marks waiting for this frame's projection.
pub(super) struct CgameMarks {
    pending: Vec<(DecalRequest, bool)>,
    marks: VecDeque<Mark>,
}

impl Default for CgameMarks {
    fn default() -> Self {
        Self {
            pending: Vec::with_capacity(MAX_PENDING),
            marks: VecDeque::with_capacity(MAX_MARK_POLYS),
        }
    }
}

impl CgameMarks {
    /// Queue a mark; `false` when the queue is full and it was dropped.
    pub(super) fn request(&mut self, request: DecalRequest, alpha_fade: bool) -> bool {
        if self.pending.len() == self.pending.capacity() {
            return false;
        }
        self.pending.push((request, alpha_fade));
        true
    }

    pub(super) fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// Clip the queued marks into the world (`R_MarkFragments`) and keep their polys.
    pub(super) fn project_pending(
        &mut self,
        world: &DecalSurfaces,
        bsp: &Bsp,
        scratch: &mut DecalScratch,
        time: u32,
        slot_of: &impl Fn(&str) -> usize,
    ) -> usize {
        let mut stored = 0;
        for (request, alpha_fade) in self.pending.drain(..) {
            let slot = slot_of(&request.shader) as u8;
            let marks = &mut self.marks;
            crate::decal_marks::project(world, bsp, &request, scratch, |vertices| {
                let mut poly = DecalPoly {
                    time,
                    fade_time: 0,
                    fade_duration: MARK_FADE_TIME,
                    shader: Arc::clone(&request.shader),
                    slot,
                    color: request.color,
                    vertices: Default::default(),
                    count: 0,
                };
                poly.fill(&request, slot, vertices);
                make_room(marks);
                marks.push_back(Mark { poly, alpha_fade });
                stored += 1;
            });
        }
        stored
    }

    /// `CG_AddMarks`: forget the marks past their time and draw the rest, unless
    /// `cg_marks` is 0.
    pub(super) fn emit(&mut self, time: u32, shown: bool, draw: &mut impl FnMut(DecalDraw<'_>)) {
        if !shown {
            return;
        }
        while self
            .marks
            .front()
            .is_some_and(|mark| time > mark.poly.time + MARK_TOTAL_TIME)
        {
            self.marks.pop_front();
        }
        for mark in &self.marks {
            draw(DecalDraw {
                shader: &mark.poly.shader,
                slot: mark.poly.slot,
                color: color(mark, time),
                vertices: &mark.poly.vertices[..usize::from(mark.poly.count)],
            });
        }
    }
}

/// `CG_AllocMark`: with every poly in use, the oldest frame's go.
fn make_room(marks: &mut VecDeque<Mark>) {
    if marks.len() < MAX_MARK_POLYS {
        return;
    }
    let oldest = marks.front().map_or(0, |mark| mark.poly.time);
    while marks.front().is_some_and(|mark| mark.poly.time == oldest) {
        marks.pop_front();
    }
}

/// The colour `CG_AddMarks` gives a mark's polys at `time`.
fn color(mark: &Mark, time: u32) -> [f32; 4] {
    // The 0–1 colour lands in a byte as it is.
    let byte = |value: f32| f32::from(value as u8) / 255.0;
    let [red, green, blue, alpha] = mark.poly.color;
    let alpha = f32::from((alpha * 255.0) as u8) / 255.0;
    let left = (mark.poly.time + MARK_TOTAL_TIME).saturating_sub(time);
    if left >= MARK_FADE_TIME {
        return [byte(red), byte(green), byte(blue), alpha];
    }
    if mark.alpha_fade {
        let fade = (255 * left / MARK_FADE_TIME) as f32 / 255.0;
        return [byte(red), byte(green), byte(blue), fade];
    }
    let fade = left as f32 / MARK_FADE_TIME as f32;
    [
        byte(red * fade),
        byte(green * fade),
        byte(blue * fade),
        alpha,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(time: u32, alpha_fade: bool) -> Mark {
        Mark {
            poly: DecalPoly {
                time,
                fade_time: 0,
                fade_duration: MARK_FADE_TIME,
                shader: Arc::from("footstep_r"),
                slot: 0,
                color: [1.0; 4],
                vertices: Default::default(),
                count: 3,
            },
            alpha_fade,
        }
    }

    #[test]
    fn a_mark_lasts_ten_seconds_and_fades_over_the_last() {
        let mut marks = CgameMarks::default();
        marks.marks.push_back(mark(100, true));
        let mut seen = Vec::new();
        marks.emit(5_100, true, &mut |draw| seen.push(draw.color));
        // At a 255th of its colour, whole alpha.
        assert_eq!(seen, [[1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0, 1.0]]);
        seen.clear();
        marks.emit(9_600, true, &mut |draw| seen.push(draw.color));
        assert_eq!(seen[0][3], 127.0 / 255.0);
        seen.clear();
        marks.emit(10_100, true, &mut |draw| seen.push(draw.color));
        assert_eq!(seen.len(), 1);
        seen.clear();
        marks.emit(10_101, true, &mut |draw| seen.push(draw.color));
        assert!(seen.is_empty() && marks.is_empty());
    }

    #[test]
    fn a_colour_fade_keeps_alpha_and_cg_marks_0_hides_the_list() {
        let mut marks = CgameMarks::default();
        marks.marks.push_back(mark(0, false));
        let mut seen = Vec::new();
        marks.emit(9_500, true, &mut |draw| seen.push(draw.color));
        assert_eq!(seen, [[0.0, 0.0, 0.0, 1.0]]);
        seen.clear();
        marks.emit(9_500, false, &mut |draw| seen.push(draw.color));
        assert!(seen.is_empty());
    }

    #[test]
    fn a_full_list_frees_the_oldest_frame() {
        let mut marks = VecDeque::new();
        for index in 0..MAX_MARK_POLYS {
            marks.push_back(mark(1 + (index as u32 / 3), false));
        }
        make_room(&mut marks);
        assert_eq!(marks.len(), MAX_MARK_POLYS - 3);
        assert_eq!(marks.front().map(|mark| mark.poly.time), Some(2));
    }
}
