//! Retained main-menu presentation: a full-bleed hero over the live map with
//! a left column of box-free entries. Everything scales with viewport height
//! so 1080p, ultrawide and 4K keep the same proportions.

use super::{ClientMenu, MAIN_ITEMS};
use crate::menu_widgets::{HeroColumn, MenuCanvas, Scrim};
use crate::ui_renderer::{BANNER_SIZE, BANNER_TEXTURE};
use crate::{TextVertex, UiFont};
use jkr_ui::{DrawCommand, FontWeight, Rect, TextAlign};

/// Build version, bottom right; the only footer text the main menu carries.
pub(super) const VERSION_LINE: &str = concat!("JKR ", env!("CARGO_PKG_VERSION"));

/// Top of the entry list and the height of one entry.
fn list_metrics(viewport: [f32; 2], scale: f32) -> (f32, f32) {
    (viewport[1] * 0.47, 72.0 * scale)
}

/// Build the main menu into `canvas` at `reveal` opacity; shared by the live
/// client and evidence.
pub(crate) fn build(canvas: &mut MenuCanvas, viewport: [f32; 2], selection: usize, reveal: f32) {
    let column = HeroColumn::new(viewport);
    let (s, x, width) = (column.scale, column.margin, column.column_width);
    let height = viewport[1];
    let (list_top, row_height) = list_metrics(viewport, s);
    canvas.begin_hero(viewport, reveal, Scrim::Full);
    let theme = canvas.theme();
    let wordmark_y = height * 0.19;
    canvas.text(
        "JEDI ACADEMY   /   MULTIPLAYER",
        Rect::new(x, wordmark_y - 30.0 * s, width, 18.0 * s),
        14.0 * s,
        theme.accent,
        FontWeight::Semibold,
        3.2 * s,
    );
    canvas.text(
        "JK::R",
        Rect::new(x - 6.0 * s, wordmark_y, width, 140.0 * s),
        132.0 * s,
        theme.foreground,
        FontWeight::Semibold,
        -2.0 * s,
    );
    // The saber emblem, in the player's accent, fills the gap to the items.
    let emblem_height = 96.0 * s;
    let emblem_width = emblem_height * BANNER_SIZE[0] as f32 / BANNER_SIZE[1] as f32;
    let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
        rect: Rect::new(x, wordmark_y + 150.0 * s, emblem_width, emblem_height),
        texture: BANNER_TEXTURE,
        color: theme.accent,
    });
    for (row, item) in MAIN_ITEMS.iter().enumerate() {
        canvas.hero_item(
            row as u16,
            item.label,
            item.hint,
            column.row_rect(list_top, row, row_height),
            row == selection,
            s,
        );
    }
    let footer_y = height - 64.0 * s;
    canvas.text_aligned(
        VERSION_LINE,
        Rect::new(
            viewport[0] - x - 320.0 * s,
            footer_y + 3.0 * s,
            320.0 * s,
            16.0 * s,
        ),
        12.0 * s,
        theme.muted,
        FontWeight::Regular,
        1.2 * s,
        TextAlign::End,
    );
    canvas.end_hero();
    canvas.finish(selection as u16);
}

impl ClientMenu {
    pub(super) fn append_main(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        let reveal = self.screen_reveal();
        if self.menu_style == super::MenuStyle::Classic {
            super::classic::view::build(&mut self.ui, viewport, &self.classic, reveal, self.art);
        } else {
            build(&mut self.ui, viewport, self.main_selection, reveal);
        }
        self.ui.append_text(vertices, font, viewport);
    }
}
