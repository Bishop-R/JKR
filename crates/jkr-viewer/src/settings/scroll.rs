//! Scrolling of the settings row column. A tab with more rows than fit
//! between the tab strip and the footer shows a window of whole rows: the
//! wheel moves the window a row per notch, the selection (moved by keys or
//! the wheel) is kept inside it, and a thin mark right of the column shows
//! where the window is. Only rows in the window are drawn and registered as
//! pointer targets, so hit-testing follows the scroll and no row reaches the
//! footer. Tabs that fit keep the old wheel behaviour of moving the
//! selection.

use crate::menu_widgets::{FormLayout, MenuCanvas};
use jkr_ui::Rect;
use std::ops::Range;

/// Gap between the row column and the scroll mark, at scale 1.
const MARK_GAP: f32 = 12.0;
/// Width of the scroll mark, at scale 1.
const MARK_WIDTH: f32 = 3.0;

/// Which rows of the open tab are on screen.
pub(super) struct RowScroll {
    /// First row shown.
    first: usize,
    /// Whole rows the column fits; set each frame by [`Self::fit`].
    page: usize,
    /// Rows of the tab as of the last [`Self::fit`].
    count: usize,
}

impl RowScroll {
    pub(super) const fn new() -> Self {
        Self {
            first: 0,
            page: usize::MAX,
            count: 0,
        }
    }

    /// Size the window for `layout` and a tab of `count` rows, scroll
    /// `selected` into it and return the rows to draw.
    pub(super) fn fit(
        &mut self,
        layout: &FormLayout,
        count: usize,
        selected: usize,
    ) -> Range<usize> {
        self.page = page_rows(layout.rows_y, layout.rows_bottom(), layout.row_height);
        self.count = count;
        self.first = reveal(self.first, selected, self.page, count);
        self.first..self.first.saturating_add(self.page).min(count)
    }

    /// Apply wheel `notches` (positive = down) to a tab of `count` rows and
    /// return the new selection: an overflowing tab scrolls a row per notch
    /// and pulls the selection along only when it would leave the window;
    /// a tab that fits moves the selection instead.
    pub(super) fn wheel(&mut self, notches: i32, count: usize, selected: usize) -> usize {
        let last = count.saturating_sub(1);
        if count <= self.page {
            return step(selected, notches).min(last);
        }
        self.first = clamp_first(step(self.first, notches), self.page, count);
        selected.clamp(self.first, (self.first + self.page - 1).min(last))
    }

    /// `layout` with its rows moved up past the hidden ones, so
    /// [`FormLayout::row_rect`] of a shown row is where it draws.
    pub(super) fn shifted(&self, layout: FormLayout) -> FormLayout {
        FormLayout {
            rows_y: layout.rows_y - self.first as f32 * layout.row_height,
            ..layout
        }
    }

    /// The display-only position mark right of the column, when the tab
    /// overflows; `layout` is the unshifted one.
    pub(super) fn mark(&self, ui: &mut MenuCanvas, layout: &FormLayout) {
        if self.count <= self.page {
            return;
        }
        let s = layout.scale;
        let track = Rect::new(
            layout.margin + layout.column_width + MARK_GAP * s,
            layout.rows_y,
            MARK_WIDTH * s,
            self.page as f32 * layout.row_height,
        );
        ui.list_scroll_mark(track, self.first, self.page, self.count, s);
    }
}

/// Whole rows of `row_height` that fit between `top` and `bottom`, at least
/// one; the small tolerance keeps an exact fit from losing a row to rounding.
fn page_rows(top: f32, bottom: f32, row_height: f32) -> usize {
    ((bottom - top) / row_height.max(1.0) + 1e-3)
        .floor()
        .max(1.0) as usize
}

/// The first row that shows `selected` with as little movement from `first`
/// as possible, clamped to the tab.
fn reveal(first: usize, selected: usize, page: usize, count: usize) -> usize {
    let first = if selected < first {
        selected
    } else if selected >= first.saturating_add(page) {
        selected + 1 - page
    } else {
        first
    };
    clamp_first(first, page, count)
}

/// `first` limited so the window never runs past the last row.
fn clamp_first(first: usize, page: usize, count: usize) -> usize {
    first.min(count.saturating_sub(page))
}

/// `index` moved by `delta`, stopping at zero.
fn step(index: usize, delta: i32) -> usize {
    index.saturating_add_signed(delta as isize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scroll(first: usize, page: usize, count: usize) -> RowScroll {
        RowScroll { first, page, count }
    }

    #[test]
    fn a_form_fits_eleven_rows_at_every_common_window_height() {
        for viewport in [
            [1920.0, 1080.0],
            [1280.0, 720.0],
            [2560.0, 1440.0],
            [1024.0, 768.0],
            [2560.0, 1080.0],
            [3840.0, 2160.0],
        ] {
            let layout = FormLayout::new(viewport);
            let mut rows = RowScroll::new();
            assert_eq!(rows.fit(&layout, 12, 0), 0..11, "{viewport:?}");
        }
    }

    #[test]
    fn shown_rows_end_above_the_footer() {
        for viewport in [[800.0, 600.0], [1920.0, 1080.0], [1920.0, 1200.0]] {
            let layout = FormLayout::new(viewport);
            let mut rows = RowScroll::new();
            let shown = rows.fit(&layout, 20, 19);
            let layout = rows.shifted(layout);
            assert!(layout.row_rect(shown.start).y >= layout.viewport[1] * 0.40 - 0.01);
            assert!(layout.row_rect(shown.end - 1).bottom() <= layout.rows_bottom() + 0.01);
        }
    }

    #[test]
    fn page_rows_counts_whole_rows_and_never_none() {
        assert_eq!(page_rows(0.0, 100.0, 50.0), 2);
        assert_eq!(page_rows(0.0, 99.0, 50.0), 1);
        assert_eq!(page_rows(0.0, 10.0, 50.0), 1);
        assert_eq!(page_rows(0.0, 150.0 - 1e-5, 50.0), 3);
    }

    #[test]
    fn reveal_moves_the_window_only_as_far_as_needed() {
        assert_eq!(reveal(0, 5, 11, 12), 0);
        assert_eq!(reveal(0, 11, 11, 12), 1);
        assert_eq!(reveal(1, 0, 11, 12), 0);
        assert_eq!(reveal(1, 6, 11, 12), 1);
        // A stale window past the end of a shorter tab snaps back.
        assert_eq!(reveal(5, 2, 11, 6), 0);
        assert_eq!(reveal(9, 9, 11, 30), 9);
    }

    #[test]
    fn fit_returns_the_rows_to_draw() {
        let layout = FormLayout::new([1920.0, 1080.0]);
        let mut rows = RowScroll::new();
        assert_eq!(rows.fit(&layout, 4, 3), 0..4);
        assert_eq!(rows.fit(&layout, 13, 12), 2..13);
        assert_eq!(rows.fit(&layout, 13, 5), 2..13);
        assert_eq!(rows.fit(&layout, 13, 0), 0..11);
        assert_eq!(rows.fit(&layout, 0, 0), 0..0);
    }

    #[test]
    fn shifted_rows_draw_from_the_top_of_the_column() {
        let layout = FormLayout::new([1920.0, 1080.0]);
        let top = layout.row_rect(0).y;
        let rows = scroll(3, 11, 14);
        assert!((rows.shifted(layout).row_rect(3).y - top).abs() < 1e-3);
    }

    #[test]
    fn wheel_scrolls_an_overflowing_tab_and_keeps_the_selection_shown() {
        let mut rows = scroll(0, 11, 12);
        // Down a notch: the window moves, the selection is pulled along.
        assert_eq!(rows.wheel(1, 12, 0), 1);
        assert_eq!(rows.first, 1);
        // At the end the window stops.
        assert_eq!(rows.wheel(1, 12, 5), 5);
        assert_eq!(rows.first, 1);
        // Back up: a selection on the last row is pulled into view.
        assert_eq!(rows.wheel(-1, 12, 11), 10);
        assert_eq!(rows.first, 0);
        assert_eq!(rows.wheel(-3, 12, 4), 4);
        assert_eq!(rows.first, 0);
    }

    #[test]
    fn wheel_moves_the_selection_when_the_tab_fits() {
        let mut rows = scroll(0, 11, 8);
        assert_eq!(rows.wheel(1, 8, 3), 4);
        assert_eq!(rows.wheel(-1, 8, 0), 0);
        assert_eq!(rows.wheel(1, 8, 7), 7);
        assert_eq!(rows.wheel(1, 0, 0), 0);
        assert_eq!(rows.first, 0);
    }
}
