//! The settings rows as a classic option panel (`setup.menu` items): the
//! label set against the label column, the value after it; toggles read
//! Yes or No, numbers are retail sliders with their value beside them, and
//! the focused item sits on the `menu_blendbox` highlight.

use super::*;
use crate::menu::classic::layout::Span;
use crate::menu::classic::panel::{OPTION, PanelFrame, focus_text};

/// The rows' wheel target over the panel and the scrollbar beside it, clear
/// of the row (0-499), tab (500), slider value (700), chrome (800) and back
/// (900) tokens.
pub(super) const CLASSIC_SCROLL_TOKEN: u16 = 911;
pub(super) const CLASSIC_SCROLLBAR_TOKEN: u16 = 912;
/// Rows one wheel notch scrolls, as the settings form steps.
pub(super) const CLASSIC_WHEEL_ROWS: i32 = 1;

impl SettingsMenu {
    /// Show rows `span` of tab `tab` as a classic option panel drawn in
    /// `frame`'s geometry.
    pub(crate) fn open_classic(
        &mut self,
        console: &ViewerConsole,
        tab: usize,
        span: Span,
        frame: crate::menu::classic::panel::Frame,
    ) {
        self.open_tab(console, tab);
        let rows = span.within(0, settings(self.tab).len());
        self.selected = rows.start;
        self.classic = Some(ClassicRows {
            rows,
            slider_span: frame.slider_span(),
            first: 0,
            visible: frame.capacity(),
        });
    }

    /// Back to the full tabbed screen, keeping the tab.
    pub(crate) fn leave_classic(&mut self) {
        if self.classic.take().is_some() {
            self.editing = None;
            self.numeric = None;
        }
    }

    /// Draw the classic panel screen around the rows of the open span.
    pub(crate) fn append_classic(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
        frame: &PanelFrame,
    ) {
        let rows = self.row_span();
        let place = frame.begin(&mut self.ui, viewport, reveal);
        let visible = place.capacity();
        let first = match &mut self.classic {
            Some(classic) => {
                classic.visible = visible;
                classic.first = classic.first.min(classic.max_first());
                classic.first
            }
            None => 0,
        };
        let shown = rows.start + first..rows.end.min(rows.start + first + visible);
        // A list longer than the panel scrolls with the wheel anywhere over
        // its rows and shows a scrollbar, as the classic key bindings do.
        if rows.len() > visible {
            let top = place.row(0);
            self.ui.scroll_region(
                CLASSIC_SCROLL_TOKEN,
                Rect::new(top.x, top.y, top.width, visible as f32 * top.height),
            );
        }
        if shown.contains(&self.selected) {
            place.highlight(&mut self.ui, self.selected - shown.start);
        }
        for row in shown.clone() {
            let slot = row - shown.start;
            let Some(setting) = settings(self.tab).get(row) else {
                continue;
            };
            let focused = row == self.selected;
            let color = if focused { focus_text() } else { OPTION };
            place.label(&mut self.ui, slot, setting.label, color);
            self.ui.hit_region(row as u16, place.row(slot));
            let value = self.values.get(row).map_or("?", String::as_str);
            match setting.kind {
                ValueKind::Bool => {
                    let yes = value.eq_ignore_ascii_case("on");
                    place.value(&mut self.ui, slot, if yes { "Yes" } else { "No" }, color);
                }
                ValueKind::Integer { .. } | ValueKind::Float { .. } => {
                    place.draw_slider_bar(&mut self.ui, slot, color);
                    let editing = self.numeric.as_ref().filter(|edit| edit.row == row);
                    let target = place.slider_value_rect(slot);
                    self.ui.hit_region(
                        crate::menu_widgets::numeric::VALUE_BASE + row as u16,
                        target,
                    );
                    match editing {
                        Some(edit) => edit.draw(&mut self.ui, target, place.scale()),
                        None => place.slider_value(&mut self.ui, slot, value, color),
                    }
                }
                ValueKind::Choice(_) => place.value(&mut self.ui, slot, value, color),
                ValueKind::Text => match self.editing.as_deref().filter(|_| focused) {
                    Some(buffer) => {
                        place.value_fmt(&mut self.ui, slot, format_args!("{buffer}_"), color)
                    }
                    None => place.value_plain(&mut self.ui, slot, value, color),
                },
            }
        }
        // Thumbs after every bar, so the slider art binds once each.
        for row in shown.clone() {
            let slot = row - shown.start;
            let Some(setting) = settings(self.tab).get(row) else {
                continue;
            };
            let value = self
                .values
                .get(row)
                .and_then(|value| value.parse::<f64>().ok());
            let ratio = match (setting.kind, value) {
                (ValueKind::Integer { min, max, .. }, Some(value)) if max > min => {
                    ((value - min as f64) / (max - min) as f64) as f32
                }
                (ValueKind::Float { min, max, .. }, Some(value)) if max > min => {
                    ((value - min) / (max - min)) as f32
                }
                (ValueKind::Integer { .. } | ValueKind::Float { .. }, _) => 0.0,
                _ => continue,
            };
            place.draw_slider_thumb(&mut self.ui, slot, ratio);
        }
        if rows.len() > visible {
            let top = place.row(0);
            let s = place.scale();
            self.ui.scrollbar(
                CLASSIC_SCROLLBAR_TOKEN,
                Rect::new(
                    top.right() - 6.0 * s,
                    top.y,
                    3.0 * s,
                    visible as f32 * top.height,
                ),
                first,
                visible,
                rows.len(),
            );
        }
        let hint = if self.numeric.is_some() {
            Some("Type a value, then ENTER to set it or ESC to cancel.")
        } else if self.editing.is_some() {
            Some("Type the new text, then ENTER to set it or ESC to cancel.")
        } else {
            None
        };
        place.finish(&mut self.ui, hint);
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::super::ClassicRows;

    fn classic(rows: std::ops::Range<usize>, visible: usize) -> ClassicRows {
        ClassicRows {
            rows,
            slider_span: (0.5, 0.25),
            first: 0,
            visible,
        }
    }

    #[test]
    fn keyboard_selection_stays_in_view() {
        let mut panel = classic(10..40, 15);
        panel.reveal(30);
        assert_eq!(panel.first, 6, "row 30 is the last of 16..31");
        panel.reveal(12);
        assert_eq!(panel.first, 2);
        panel.reveal(14);
        assert_eq!(panel.first, 2, "already shown");
        // A row before the span (the key-bindings row) leaves it alone.
        panel.reveal(3);
        assert_eq!(panel.first, 2);
    }

    #[test]
    fn wheel_and_bar_stop_at_the_ends() {
        let mut panel = classic(0..20, 15);
        assert_eq!(panel.max_first(), 5);
        panel.scroll_by(-3);
        assert_eq!(panel.first, 0);
        panel.scroll_by(9);
        assert_eq!(panel.first, 5);
        panel.scroll_to_ratio(0.4);
        assert_eq!(panel.first, 2);
        panel.scroll_to_ratio(7.0);
        assert_eq!(panel.first, 5);
        // A span that fits never scrolls.
        let mut short = classic(0..9, 15);
        short.scroll_by(4);
        assert_eq!(short.first, 0);
    }
}
