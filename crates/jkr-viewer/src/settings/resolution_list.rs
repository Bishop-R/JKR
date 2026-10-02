//! The resolution list of the settings screen: Enter or a click on the
//! Resolution row opens it over the form, in the hero-form style of the map
//! list. Rows show each size with its aspect-ratio group, the size in use
//! highlighted and tagged; arrows, pages, Home/End or the wheel move, Enter
//! or a click picks and Esc (or the footer cap) goes back to the form.

use super::resolution::{self, parse_size};
use super::*;
use crate::menu_widgets::{BACK_TOKEN, FormLayout, Scrim};
use jkr_ui::{Color, FontWeight, InputEvent, TextAlign, UiEventKind};

/// Footer caps; only the back cap, which doubles as the pointer's way out.
const KEY_HINTS: [(&str, &str); 1] = [("ESC", "Back")];
/// Height of one list row at scale 1.
const ROW_HEIGHT: f32 = 44.0;

impl SettingsMenu {
    /// The `r_resolution` size in use.
    fn current_resolution(console: &ViewerConsole) -> Option<[u32; 2]> {
        console.text_value("r_resolution").and_then(parse_size)
    }

    /// The display mode that is applied now.
    fn applied_display(&self, console: &ViewerConsole) -> DisplayMode {
        DisplayMode::requested(console).effective(self.exclusive_available())
    }

    /// Rebuild the sizes on offer for the current display mode.
    pub(super) fn build_choices(&mut self, console: &ViewerConsole) {
        let exclusive = self.applied_display(console) == DisplayMode::Exclusive;
        resolution::build_choices(
            self.monitor.as_ref(),
            Self::current_resolution(console),
            exclusive,
            &mut self.choices,
        );
    }

    /// Open the list on the size in use.
    pub(super) fn open_resolutions(&mut self, console: &ViewerConsole) {
        self.build_choices(console);
        let note = match self.applied_display(console) {
            DisplayMode::Windowed => "The window's size, grouped by aspect ratio.",
            DisplayMode::Borderless => "Used when windowed; borderless fills the desktop.",
            DisplayMode::Exclusive => "The monitor's video modes, by aspect ratio.",
        };
        self.picker
            .open(&self.choices, Self::current_resolution(console), note);
    }

    /// Step the Resolution row within its aspect-ratio group.
    pub(super) fn step_resolution(&mut self, console: &mut ViewerConsole, direction: i32) {
        self.build_choices(console);
        let current = Self::current_resolution(console);
        if let Some(size) = resolution::step(&self.choices, current, direction)
            && Some(size) != current
        {
            set_resolution(console, size);
        }
        self.refresh(console);
    }

    /// A key while the list is open.
    pub(super) fn resolution_key(
        &mut self,
        key: KeyCode,
        repeat: bool,
        console: &mut ViewerConsole,
    ) {
        let confirms = matches!(
            key,
            KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::Space
                | KeyCode::Escape
                | KeyCode::Backspace
        );
        // A held Enter that opened the list must not pick from it too.
        if repeat && confirms {
            return;
        }
        if let PickResult::Close(Some(size)) = self.picker.key(key) {
            self.pick_resolution(console, size);
        }
    }

    /// Pointer while the list is open: hover highlights, a click picks, the
    /// wheel scrolls and the footer cap closes the list.
    pub(super) fn resolution_pointer(&mut self, event: InputEvent, console: &mut ViewerConsole) {
        let Some(event) = self.ui.pointer(event) else {
            return;
        };
        let Some(token) = event.token else {
            return;
        };
        let slot = usize::from(token);
        let on_row = slot < self.picker.page();
        match event.kind {
            UiEventKind::Wheel => {
                let notches = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
                self.picker.scroll(notches);
            }
            UiEventKind::HoverEnter | UiEventKind::Hover if on_row => {
                self.picker.hover(self.picker.first() + slot);
            }
            UiEventKind::Activate if token == BACK_TOKEN => self.picker.close(),
            UiEventKind::Activate if on_row => {
                self.picker.hover(self.picker.first() + slot);
                if let PickResult::Close(Some(size)) = self.picker.pick() {
                    self.pick_resolution(console, size);
                }
            }
            _ => {}
        }
    }

    fn pick_resolution(&mut self, console: &mut ViewerConsole, size: [u32; 2]) {
        if Some(size) != Self::current_resolution(console) {
            set_resolution(console, size);
        }
        self.refresh(console);
    }

    /// Build the open list at `reveal` opacity and append its text.
    pub(super) fn append_resolutions(
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
            "JKR   /   SETTINGS",
            "Resolution",
            self.picker.note(),
        );
        let row_height = ROW_HEIGHT * s;
        let list_bottom = viewport[1] - 110.0 * s;
        self.picker.set_page(
            ((list_bottom - layout.rows_y) / row_height)
                .floor()
                .max(1.0) as usize,
        );
        self.resolution_rows(&layout, row_height);
        self.ui.form_footer(&layout, &KEY_HINTS);
        self.ui.end_hero();
        let focus = self.picker.selected().saturating_sub(self.picker.first());
        self.ui.finish(focus as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// The visible rows; row token `i` is list position `first + i`.
    fn resolution_rows(&mut self, layout: &FormLayout, row_height: f32) {
        let s = layout.scale;
        let theme = self.ui.theme();
        let (first, page) = (self.picker.first(), self.picker.page());
        let count = self.picker.choices().len();
        let current = self.picker.current();
        let width = layout.column_width - 18.0 * s;
        for slot in 0..page.min(count.saturating_sub(first)) {
            let position = first + slot;
            let Some(&choice) = self.picker.choices().get(position) else {
                break;
            };
            let selected = position == self.picker.selected();
            let in_use = Some(choice.size) == current;
            let rect = Rect::new(
                layout.margin,
                layout.rows_y + slot as f32 * row_height,
                width,
                row_height,
            );
            self.ui.form_row_frame(rect, slot as u16, selected, s);
            let text_y = rect.y + (row_height - 22.0 * s) * 0.5;
            let [w, h] = choice.size;
            self.ui.text_fmt_aligned(
                format_args!("{w} x {h}"),
                Rect::new(rect.x, text_y, rect.width * 0.5, 22.0 * s),
                17.0 * s,
                if selected || in_use {
                    theme.foreground
                } else {
                    Color::new(0.82, 0.88, 0.94, 0.78)
                },
                if selected || in_use {
                    FontWeight::Semibold
                } else {
                    FontWeight::Regular
                },
                0.2 * s,
                TextAlign::Start,
            );
            let tag = match (in_use, choice.desktop) {
                (true, true) => "   ·   desktop   ·   in use",
                (true, false) => "   ·   in use",
                (false, true) => "   ·   desktop",
                (false, false) => "",
            };
            let x = rect.x + rect.width * 0.5;
            self.ui.text_fmt_aligned(
                format_args!("{}{tag}", choice.aspect),
                Rect::new(x, text_y + 2.0 * s, rect.right() - x, 20.0 * s),
                14.0 * s,
                if selected || in_use {
                    theme.accent
                } else {
                    theme.muted
                },
                FontWeight::Regular,
                0.4 * s,
                TextAlign::End,
            );
        }
        if count > page {
            let track = Rect::new(
                layout.margin + layout.column_width - 4.0 * s,
                layout.rows_y,
                3.0 * s,
                row_height * page as f32,
            );
            self.ui.list_scroll_mark(track, first, page, count, s);
        }
    }
}

/// Write `size` to `r_resolution`.
fn set_resolution(console: &mut ViewerConsole, [width, height]: [u32; 2]) {
    console.set_cvar("r_resolution", &format!("{width}x{height}"));
}
