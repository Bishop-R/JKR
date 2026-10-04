//! Map list presentation: the hero form's header, a live filter line, a
//! box-free list of long names with their `mp/…` names, and the highlighted
//! map's levelshot large on the right; plus the preview frame the Create
//! game form shows beside its Map row. Nothing sits behind the text.

use super::create_game::CreateGameMenu;
use super::create_game_catalog::MODES;
use super::levelshot::Preview;
use crate::menu_widgets::{FormLayout, MenuCanvas, Scrim};
use crate::ui_renderer::LEVELSHOT_TEXTURE;
use crate::{TextVertex, UiFont};
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// Footer caps; only the back cap, which doubles as the pointer's way out.
const KEY_HINTS: [(&str, &str); 1] = [("ESC", "Back")];
/// Height of one list row at scale 1.
const ROW_HEIGHT: f32 = 44.0;
/// Width of the large preview at scale 1 (4:3).
const PREVIEW_WIDTH: f32 = 640.0;

impl CreateGameMenu {
    /// Build the open map list at `reveal` opacity and append its text.
    pub(super) fn append_picker(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        let layout = FormLayout::new(viewport);
        let s = layout.scale;
        self.ui.begin_hero(viewport, reveal, Scrim::Full);
        self.ui.form_header(
            &layout,
            "JKR   /   HOST",
            "Choose a map",
            MODES[self.draft.mode].label,
        );
        let row_height = ROW_HEIGHT * s;
        let list_bottom = viewport[1] - 110.0 * s;
        self.picker.set_page(
            ((list_bottom - layout.rows_y) / row_height)
                .floor()
                .max(1.0) as usize,
        );
        self.filter_line(&layout);
        self.list_rows(&layout, row_height);
        // The highlighted map, large, right of the list.
        let x = layout.margin + layout.column_width + 56.0 * s;
        let width = (PREVIEW_WIDTH * s).min(viewport[0] - x - layout.margin);
        if width > 80.0 * s {
            let rect = Rect::new(x, layout.rows_y, width, width * 0.75);
            let preview = self.levelshots.preview(self.preview_map());
            draw_preview(&mut self.ui, rect, preview, s);
            if let Some(entry) = self.picker.highlighted(&self.catalogue) {
                let theme = self.ui.theme();
                let title_y = rect.bottom() + 18.0 * s;
                self.ui.text(
                    &entry.title,
                    Rect::new(x, title_y, width, 30.0 * s),
                    26.0 * s,
                    theme.foreground,
                    FontWeight::Semibold,
                    0.0,
                );
                self.ui.text(
                    &entry.name,
                    Rect::new(x, title_y + 36.0 * s, width, 20.0 * s),
                    15.0 * s,
                    theme.muted,
                    FontWeight::Regular,
                    0.6 * s,
                );
            }
        }
        self.ui.form_footer(&layout, &KEY_HINTS);
        self.ui.end_hero();
        let focus = self.picker.selected().saturating_sub(self.picker.first());
        self.ui.finish(focus as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// What has been typed (or how to filter), and how many maps match.
    fn filter_line(&mut self, layout: &FormLayout) {
        let s = layout.scale;
        let theme = self.ui.theme();
        let field = Rect::new(
            layout.margin,
            layout.rows_y - 52.0 * s,
            layout.column_width,
            36.0 * s,
        );
        let text = Rect::new(field.x, field.y + 8.0 * s, field.width * 0.7, 22.0 * s);
        if self.picker.filter().is_empty() {
            self.ui.text(
                "Type to filter",
                text,
                17.0 * s,
                theme.muted,
                FontWeight::Regular,
                0.2 * s,
            );
            self.ui
                .separator_line(Rect::new(field.x, field.bottom() - 1.0, field.width, 1.0));
        } else {
            self.ui.text(
                self.picker.filter(),
                text,
                17.0 * s,
                theme.foreground,
                FontWeight::Semibold,
                0.2 * s,
            );
            self.ui.edit_underline(field, theme.accent, s);
        }
        let (shown, total) = (
            self.picker.match_count(),
            self.picker.total(&self.catalogue),
        );
        let count = Rect::new(field.x, field.y + 10.0 * s, field.width, 20.0 * s);
        if shown == total {
            self.ui.text_fmt_aligned(
                format_args!("{total} maps"),
                count,
                14.0 * s,
                theme.muted,
                FontWeight::Regular,
                0.4 * s,
                TextAlign::End,
            );
        } else {
            self.ui.text_fmt_aligned(
                format_args!("{shown} of {total}"),
                count,
                14.0 * s,
                theme.muted,
                FontWeight::Regular,
                0.4 * s,
                TextAlign::End,
            );
        }
    }

    /// The visible rows; row token `i` is list position `first + i`.
    fn list_rows(&mut self, layout: &FormLayout, row_height: f32) {
        let s = layout.scale;
        let theme = self.ui.theme();
        let (first, page, count) = (
            self.picker.first(),
            self.picker.page(),
            self.picker.match_count(),
        );
        let width = layout.column_width - 18.0 * s;
        for slot in 0..page.min(count.saturating_sub(first)) {
            let position = first + slot;
            let Some(entry) = self.picker.entry(&self.catalogue, position) else {
                break;
            };
            let selected = position == self.picker.selected();
            let rect = Rect::new(
                layout.margin,
                layout.rows_y + slot as f32 * row_height,
                width,
                row_height,
            );
            self.ui.form_row_frame(rect, slot as u16, selected, s);
            let text_y = rect.y + (row_height - 22.0 * s) * 0.5;
            let name_shown = entry.title != entry.name;
            let title_width = if name_shown {
                rect.width * 0.64
            } else {
                rect.width
            };
            self.ui.text(
                &entry.title,
                Rect::new(rect.x, text_y, title_width, 22.0 * s),
                17.0 * s,
                if selected {
                    theme.foreground
                } else {
                    Color::new(0.82, 0.88, 0.94, 0.78)
                },
                if selected {
                    FontWeight::Semibold
                } else {
                    FontWeight::Regular
                },
                0.2 * s,
            );
            if name_shown {
                let x = rect.x + title_width + 12.0 * s;
                self.ui.text_aligned(
                    &entry.name,
                    Rect::new(x, text_y + 2.0 * s, rect.right() - x, 20.0 * s),
                    14.0 * s,
                    if selected { theme.accent } else { theme.muted },
                    FontWeight::Regular,
                    0.4 * s,
                    TextAlign::End,
                );
            }
        }
        if count == 0 {
            self.ui.text(
                "No map matches.",
                Rect::new(layout.margin, layout.rows_y + 12.0 * s, width, 22.0 * s),
                17.0 * s,
                theme.muted,
                FontWeight::Regular,
                0.2 * s,
            );
        }
        if count > page {
            scroll_indicator(
                &mut self.ui,
                layout,
                row_height * page as f32,
                first,
                page,
                count,
            );
        }
    }
}

/// Thin, display-only position mark right of the list.
fn scroll_indicator(
    ui: &mut MenuCanvas,
    layout: &FormLayout,
    height: f32,
    first: usize,
    page: usize,
    count: usize,
) {
    let s = layout.scale;
    let track = Rect::new(
        layout.margin + layout.column_width - 4.0 * s,
        layout.rows_y,
        3.0 * s,
        height,
    );
    ui.list_scroll_mark(track, first, page, count, s);
}

/// A map preview in `rect`: the levelshot with a hairline edge, or — while
/// it decodes or when the map ships none — the bare edge (with "No preview"
/// for a map that has none).
pub(super) fn draw_preview(ui: &mut MenuCanvas, rect: Rect, preview: Preview, s: f32) {
    if preview == Preview::Image {
        let _ = ui.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect,
            texture: LEVELSHOT_TEXTURE,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        });
    }
    let _ = ui.draw_list_mut().push(DrawCommand::Border {
        rect,
        radius: 0.0,
        width: 1.0,
        color: Color::new(
            1.0,
            1.0,
            1.0,
            if preview == Preview::Image {
                0.16
            } else {
                0.12
            },
        ),
    });
    if preview == Preview::Missing {
        let muted = ui.theme().muted;
        let size = (rect.height * 0.09).clamp(12.0 * s, 17.0 * s);
        ui.text_aligned(
            "No preview",
            Rect::new(
                rect.x,
                rect.y + (rect.height - size * 1.3) * 0.5,
                rect.width,
                size * 1.3,
            ),
            size,
            muted,
            FontWeight::Regular,
            0.4 * s,
            TextAlign::Center,
        );
    }
}
