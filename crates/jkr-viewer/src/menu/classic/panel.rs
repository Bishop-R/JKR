//! The classic Setup and Controls screens. Retail's `setup.menu` and
//! `controls.menu` (and the in-game pop-ups `ingame_setup.menu` and
//! `ingame_controls.menu`) keep the list of option groups on the left and
//! show the chosen group's items in a panel beside it. This module draws
//! everything around those items (backdrop, title, navigation, the group
//! list and the panel box) and gives the item rows their retail geometry.
//! The settings screen and the key-binding editor draw the items, so the
//! options behave the same in both menu styles.

use super::layout::{CANVAS, Entry, HINT_Y, Page, Placement, Size, Slot};
use super::view::{self, Caps, DISABLED, FOCUS, GOLD, HINT};
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu_widgets::MenuCanvas;
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// Pointer tokens of the screen's own buttons (navigation row, group list,
/// Back and Exit): `CHROME_BASE` plus the button's index in its page's
/// [`Page::slots`]. Clear of the items' row tokens (row indices), the
/// settings tabs (500), the key-binding editor's secondary slots (600) and
/// the slider value targets (700).
pub(crate) const CHROME_BASE: u16 = 800;

/// Page slot index of a chrome token.
pub(crate) fn chrome_slot(token: u16) -> Option<usize> {
    (CHROME_BASE..CHROME_BASE + 32)
        .contains(&token)
        .then(|| usize::from(token - CHROME_BASE))
}

/// Retail option item colour (`forecolor 0.65 0.65 1`).
pub(crate) const OPTION: Color = Color::new(0.65, 0.65, 1.0, 1.0);
/// The focused item's colour as retail paints it (`focusColor 1 1 1 1`),
/// pulsing ([`view::focus_pulse`]).
pub(crate) fn focus_text() -> Color {
    view::focus_pulse()
}
/// Retail colour of a key being rebound (`Item_Bind_Paint`'s red pulse).
pub(crate) const BINDING: Color = Color::new(1.0, 0.25, 0.25, 1.0);
/// Retail panel box (`setup_background`: `backcolor 0 0 .6 .5`, border
/// `0 0 .6 1`).
const PANEL_FILL: Color = Color::new(0.0, 0.0, 0.6, 0.5);
const PANEL_BORDER: Color = Color::new(0.0, 0.0, 0.6, 1.0);
/// Retail panel title colour (`forecolor .549 .854 1`).
const PANEL_TITLE: Color = Color::new(0.549, 0.854, 1.0, 1.0);
/// Retail slider art size (`SLIDER_WIDTH`, `SLIDER_HEIGHT`,
/// `SLIDER_THUMB_WIDTH`, `SLIDER_THUMB_HEIGHT` in `ui_shared.h`).
const SLIDER: [f32; 2] = [96.0, 16.0];
const THUMB: [f32; 2] = [12.0, 20.0];
/// Retail gap between an item's label and its value (`textRect.w + 8`).
const VALUE_GAP: f32 = 8.0;

/// Where a panel screen is shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Frame {
    /// The main menu's full page (`setup.menu`, `controls.menu`).
    Main,
    /// The in-game pop-up under the top bar (`ingame_setup.menu`,
    /// `ingame_controls.menu`).
    InGame,
}

/// Canvas geometry of one frame, from the retail item rectangles.
struct Geometry {
    /// The filled panel box.
    panel: [f32; 4],
    /// Left edge and width of an item row.
    row_x: f32,
    row_width: f32,
    /// Top of the first row and the row pitch.
    first_row: f32,
    row_height: f32,
    /// Rows the panel holds.
    rows: usize,
    /// Right edge of item labels (`rect.x + textalignx`).
    label_end: f32,
    /// Item text height.
    text: f32,
    /// The title band and its vertical centre.
    title: [f32; 4],
    /// Centre of the description line.
    hint: [f32; 2],
}

/// `setup.menu`: panel `260 185 340 225`, items `260 188+14n 340 14`
/// labelled up to `textalignx 174`, title band `100 164 440 16`.
const MAIN: Geometry = Geometry {
    panel: [260.0, 185.0, 340.0, 227.0],
    row_x: 260.0,
    row_width: 340.0,
    first_row: 188.0,
    row_height: 14.0,
    rows: 15,
    label_end: 434.0,
    text: 11.0,
    title: [100.0, 164.0, 440.0, 16.0],
    hint: [CANVAS[0] * 0.5, HINT_Y],
};

/// `ingame_setup.menu` (menu rect `45 35 550 335`): box `0 0 570 335`,
/// group list `20 43+30n 170 30`, panel `210 41 350 250`, items
/// `220 41+20n 300 20` labelled up to `textalignx 165`, title band
/// `20 5 510 28`, description at `305 347`.
const IN_GAME: Geometry = Geometry {
    panel: [45.0 + 210.0, 35.0 + 41.0, 350.0, 250.0],
    row_x: 45.0 + 220.0,
    row_width: 300.0,
    first_row: 35.0 + 43.0,
    row_height: 20.0,
    rows: 12,
    label_end: 45.0 + 385.0,
    text: 12.0,
    title: [45.0 + 20.0, 35.0 + 5.0, 510.0, 28.0],
    hint: [45.0 + 305.0, 35.0 + 347.0],
};

/// The in-game pop-up box (`background_pic` `0 0 570 335` of a menu at
/// `45 35`).
const IN_GAME_BOX: [f32; 4] = [45.0, 35.0, 570.0, 335.0];
/// The in-game group list: `20 43+30n 170 30`, labels set against the
/// right edge, glow `185` wide.
const IN_GAME_LIST: [f32; 4] = [45.0 + 20.0, 35.0 + 43.0, 170.0, 30.0];

impl Frame {
    fn geometry(self) -> &'static Geometry {
        match self {
            Self::Main => &MAIN,
            Self::InGame => &IN_GAME,
        }
    }

    /// Item rows the panel holds.
    pub(crate) fn capacity(self) -> usize {
        self.geometry().rows
    }

    /// Where a slider bar sits across its item row: the offset of its left
    /// edge and its width, as fractions of the row width.
    pub(crate) fn slider_span(self) -> (f32, f32) {
        let geometry = self.geometry();
        (
            (geometry.label_end + VALUE_GAP - geometry.row_x) / geometry.row_width,
            SLIDER[0] / geometry.row_width,
        )
    }
}

/// Position along a slider of the pointer at `x` over item row `row`
/// (window coordinates), for a frame with `slider_span` `(offset, width)`.
pub(crate) fn slider_ratio(row: Rect, x: f32, (offset, width): (f32, f32)) -> f32 {
    if row.width <= 0.0 || width <= 0.0 {
        return 0.0;
    }
    (((x - row.x) / row.width - offset) / width).clamp(0.0, 1.0)
}

/// One panel screen: which page's list it shows, which group is open.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PanelFrame {
    pub(crate) frame: Frame,
    pub(crate) page: Page,
    pub(crate) active: Entry,
    pub(crate) art: ArtSet,
}

/// A panel screen being drawn: its placement on the window and its
/// geometry, for the item rows.
pub(crate) struct PanelPlace {
    place: Placement,
    frame: Frame,
    art: ArtSet,
    /// Description of the chrome button under the pointer, if any.
    hovered_hint: Option<(&'static str, bool)>,
}

impl PanelFrame {
    /// Begin the screen in `canvas`: backdrop, title, the page's buttons and
    /// the empty panel box. Item rows go on top through the returned
    /// [`PanelPlace`]; [`PanelPlace::finish`] adds the description line.
    pub(crate) fn begin(
        &self,
        canvas: &mut MenuCanvas,
        viewport: [f32; 2],
        reveal: f32,
    ) -> PanelPlace {
        let place = Placement::new(viewport);
        canvas.begin_transparent(viewport);
        canvas.push_opacity(reveal);
        match self.frame {
            Frame::Main => view::page_backdrop(canvas, viewport, &place, self.page, self.art),
            Frame::InGame => self.in_game_box(canvas, viewport, &place),
        }
        let geometry = self.frame.geometry();
        let title = match (self.frame, self.page) {
            (Frame::Main, page) => page.title().0,
            (Frame::InGame, Page::Controls) => "CONTROLS",
            (Frame::InGame, _) => "SETUP",
        };
        self.title(canvas, &place, geometry, title);
        let mut hovered_hint = None;
        for (index, slot) in self.page.slots().iter().enumerate() {
            let Some(target) = self.slot_target(index, slot) else {
                continue;
            };
            let token = CHROME_BASE + index as u16;
            let target = place.rect(target);
            let hovered = canvas.token_hovered(token);
            if hovered {
                hovered_hint = Some((slot.hint, slot.enabled()));
            }
            if hovered && slot.enabled() {
                self.glow(canvas, &place, slot, target);
            }
            // The open group's entry stays white, as retail recolours it;
            // the hovered one has the focus and pulses.
            let color = match (slot.enabled(), hovered, slot.entry == self.active) {
                (false, _, _) => DISABLED,
                (true, true, _) => view::focus_pulse(),
                (true, false, true) => FOCUS,
                (true, false, false) => GOLD,
            };
            match self.frame {
                Frame::Main => view::entry_label_colored(canvas, &place, slot, color),
                Frame::InGame => self.in_game_label(canvas, &place, index, slot, color),
            }
            canvas.hit_region(token, target);
        }
        panel_box(canvas, place.rect(geometry.panel), place.scale);
        PanelPlace {
            place,
            frame: self.frame,
            art: self.art,
            hovered_hint,
        }
    }

    /// Canvas target of page slot `index`, if this frame shows it: every
    /// button on the main page, only the group list in the pop-up.
    fn slot_target(&self, index: usize, slot: &Slot) -> Option<[f32; 4]> {
        match self.frame {
            Frame::Main => Some(slot.target()),
            Frame::InGame => {
                let row = self.list_row(index)?;
                let [x, y, width, height] = IN_GAME_LIST;
                Some([x, y + row as f32 * height, width, height])
            }
        }
    }

    /// Row of page slot `index` in the pop-up's group list.
    fn list_row(&self, index: usize) -> Option<usize> {
        let slots = self.page.slots();
        (slots.get(index)?.size == Size::List).then(|| {
            slots[..index]
                .iter()
                .filter(|slot| slot.size == Size::List)
                .count()
        })
    }

    /// Focus glow behind a hovered button: retail's `menu_blendbox2` behind
    /// group entries (10 units wider than the entry), `menu_buttonback`
    /// behind the others.
    fn glow(&self, canvas: &mut MenuCanvas, place: &Placement, slot: &Slot, target: Rect) {
        if slot.size != Size::List {
            view::glow(canvas, target, place.scale, self.art);
            return;
        }
        let extra = match self.frame {
            Frame::Main => 10.0,
            Frame::InGame => 15.0,
        };
        let wider = Rect::new(
            target.x,
            target.y,
            target.width + extra * place.scale,
            target.height,
        );
        if self.art.has(ArtPiece::BlendBox2) {
            view::art(canvas, ArtPiece::BlendBox2, wider);
        } else {
            view::glow(canvas, wider, place.scale, self.art);
        }
    }

    /// A pop-up group entry: retail font 3 at 0.9, set against the right
    /// edge of its row.
    fn in_game_label(
        &self,
        canvas: &mut MenuCanvas,
        place: &Placement,
        index: usize,
        slot: &Slot,
        color: Color,
    ) {
        let Some(row) = self.list_row(index) else {
            return;
        };
        let [x, y, width, height] = IN_GAME_LIST;
        let size = Size::List.text();
        let top = y + row as f32 * height + (height - size * 1.2) * 0.5;
        canvas.text_fmt_aligned(
            format_args!("{}", Caps(slot.label)),
            place.rect([x, top, width, size * 1.2]),
            size * place.scale,
            color,
            FontWeight::Semibold,
            1.2 * place.scale,
            TextAlign::End,
        );
    }

    /// The pop-up's dark box over the dimmed match.
    fn in_game_box(&self, canvas: &mut MenuCanvas, viewport: [f32; 2], place: &Placement) {
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: Rect::new(0.0, 0.0, viewport[0], viewport[1]),
            color: view::ink(0.35),
        });
        let rect = place.rect(IN_GAME_BOX);
        if self.art.has(ArtPiece::PopupBox) {
            view::art(canvas, ArtPiece::PopupBox, rect);
        } else {
            let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                rect,
                color: view::ink(0.86),
            });
        }
    }

    /// The panel title over its `menu_blendbox` band.
    fn title(&self, canvas: &mut MenuCanvas, place: &Placement, geometry: &Geometry, text: &str) {
        let band = place.rect(geometry.title);
        if self.art.has(ArtPiece::BlendBox) {
            view::art(canvas, ArtPiece::BlendBox, band);
        } else {
            view::soft_band(canvas, band, 0.16);
        }
        let [x, y, width, height] = geometry.title;
        canvas.text_aligned(
            text,
            place.rect([x, y + (height - 14.0) * 0.5 - 1.0, width, 14.0]),
            11.5 * place.scale,
            PANEL_TITLE,
            FontWeight::Semibold,
            3.0 * place.scale,
            TextAlign::Center,
        );
    }
}

/// The filled panel box with its one-unit border.
fn panel_box(canvas: &mut MenuCanvas, rect: Rect, scale: f32) {
    let draw = canvas.draw_list_mut();
    let _ = draw.push(DrawCommand::SolidRect {
        rect,
        color: PANEL_FILL,
    });
    let line = scale.max(1.0);
    for edge in [
        Rect::new(rect.x, rect.y, rect.width, line),
        Rect::new(rect.x, rect.bottom() - line, rect.width, line),
        Rect::new(rect.x, rect.y, line, rect.height),
        Rect::new(rect.right() - line, rect.y, line, rect.height),
    ] {
        let _ = draw.push(DrawCommand::SolidRect {
            rect: edge,
            color: PANEL_BORDER,
        });
    }
}

impl PanelPlace {
    /// Item rows the panel holds.
    pub(crate) fn capacity(&self) -> usize {
        self.frame.capacity()
    }

    /// Window scale of one canvas unit.
    pub(crate) fn scale(&self) -> f32 {
        self.place.scale
    }

    fn geometry(&self) -> &'static Geometry {
        self.frame.geometry()
    }

    /// Item row `slot` of the panel (window coordinates).
    pub(crate) fn row(&self, slot: usize) -> Rect {
        let geometry = self.geometry();
        self.place.rect([
            geometry.row_x,
            geometry.first_row + slot as f32 * geometry.row_height,
            geometry.row_width,
            geometry.row_height,
        ])
    }

    /// Retail's `menu_blendbox` highlight behind the focused item.
    pub(crate) fn highlight(&self, canvas: &mut MenuCanvas, slot: usize) {
        let row = self.row(slot);
        if self.art.has(ArtPiece::BlendBox) {
            view::art(canvas, ArtPiece::BlendBox, row);
        } else {
            view::soft_band(canvas, row, 0.22);
        }
    }

    /// Text box of row `slot` from canvas x `from` to `to`.
    fn text_rect(&self, slot: usize, from: f32, to: f32) -> Rect {
        let geometry = self.geometry();
        let line = geometry.text * 1.25;
        let top = geometry.first_row
            + slot as f32 * geometry.row_height
            + (geometry.row_height - line) * 0.5;
        self.place.rect([from, top, (to - from).max(0.0), line])
    }

    /// An item's label in capitals, set against the label column's right
    /// edge.
    pub(crate) fn label(&self, canvas: &mut MenuCanvas, slot: usize, text: &str, color: Color) {
        let geometry = self.geometry();
        canvas.text_fmt_aligned(
            format_args!("{}", Caps(text)),
            self.text_rect(slot, geometry.row_x, geometry.label_end),
            geometry.text * self.place.scale,
            color,
            FontWeight::Regular,
            0.4 * self.place.scale,
            TextAlign::End,
        );
    }

    /// Canvas x where an item's value starts.
    fn value_x(&self) -> f32 {
        self.geometry().label_end + VALUE_GAP
    }

    /// Right edge of the panel's text, inside its box.
    fn value_end(&self) -> f32 {
        let [x, _, width, _] = self.geometry().panel;
        x + width - 4.0
    }

    /// An item's value in capitals, from the value column to the panel's
    /// edge.
    pub(crate) fn value(&self, canvas: &mut MenuCanvas, slot: usize, text: &str, color: Color) {
        self.value_fmt(canvas, slot, format_args!("{}", Caps(text)), color);
    }

    /// An item's value as written, such as typed text (an address or a
    /// name), which capitals would misrepresent.
    pub(crate) fn value_plain(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        text: &str,
        color: Color,
    ) {
        self.value_from(canvas, slot, self.value_x(), text, color);
    }

    /// An item's value, formatted without allocating.
    pub(crate) fn value_fmt(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        text: std::fmt::Arguments<'_>,
        color: Color,
    ) {
        let geometry = self.geometry();
        canvas.text_fmt_aligned(
            text,
            self.text_rect(slot, self.value_x(), self.value_end()),
            geometry.text * self.place.scale,
            color,
            FontWeight::Regular,
            0.4 * self.place.scale,
            TextAlign::Start,
        );
    }

    fn value_from(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        from: f32,
        text: &str,
        color: Color,
    ) {
        let geometry = self.geometry();
        canvas.text_aligned(
            text,
            self.text_rect(slot, from, self.value_end()),
            geometry.text * self.place.scale,
            color,
            FontWeight::Regular,
            0.4 * self.place.scale,
            TextAlign::Start,
        );
    }

    /// The slider bar of row `slot` (window coordinates), where retail
    /// draws it: after the label, at the top of the item.
    pub(crate) fn slider_bar(&self, slot: usize) -> Rect {
        let geometry = self.geometry();
        let top = geometry.first_row
            + slot as f32 * geometry.row_height
            + (geometry.row_height - SLIDER[1]) * 0.5;
        self.place.rect([self.value_x(), top, SLIDER[0], SLIDER[1]])
    }

    /// Draw the slider bar of row `slot`.
    pub(crate) fn draw_slider_bar(&self, canvas: &mut MenuCanvas, slot: usize, color: Color) {
        let bar = self.slider_bar(slot);
        if self.art.has(ArtPiece::Slider) {
            view::art(canvas, ArtPiece::Slider, bar);
            return;
        }
        let rail = Rect::new(
            bar.x,
            bar.y + bar.height * 0.45,
            bar.width,
            bar.height * 0.1,
        );
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: rail,
            color: Color::new(color.r, color.g, color.b, 0.6),
        });
    }

    /// Draw the slider thumb of row `slot` at `ratio` along its bar; drawn
    /// after every bar so the art switches texture only once.
    pub(crate) fn draw_slider_thumb(&self, canvas: &mut MenuCanvas, slot: usize, ratio: f32) {
        let bar = self.slider_bar(slot);
        let s = self.place.scale;
        let x = bar.x + bar.width * ratio.clamp(0.0, 1.0);
        let thumb = Rect::new(
            x - THUMB[0] * 0.5 * s,
            bar.y - 2.0 * s,
            THUMB[0] * s,
            THUMB[1] * s,
        );
        if self.art.has(ArtPiece::SliderThumb) {
            view::art(canvas, ArtPiece::SliderThumb, thumb);
        } else {
            let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                rect: Rect::new(
                    thumb.x + thumb.width * 0.3,
                    thumb.y,
                    thumb.width * 0.4,
                    thumb.height,
                ),
                color: FOCUS,
            });
        }
    }

    /// The number shown after a slider bar, and its click target for typed
    /// entry.
    pub(crate) fn slider_value_rect(&self, slot: usize) -> Rect {
        let from = self.value_x() + SLIDER[0] + VALUE_GAP;
        self.text_rect(slot, from, self.value_end())
    }

    /// The number after a slider bar.
    pub(crate) fn slider_value(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        text: &str,
        color: Color,
    ) {
        let from = self.value_x() + SLIDER[0] + VALUE_GAP;
        self.value_from(canvas, slot, from, text, color);
    }

    /// End the screen: the description line shows the hovered button's
    /// description, else `item_hint` (the panel's own line), in retail's
    /// description colour.
    pub(crate) fn finish(&self, canvas: &mut MenuCanvas, item_hint: Option<&str>) {
        let geometry = self.geometry();
        let (text, color) = match (self.hovered_hint, item_hint) {
            (Some((hint, enabled)), _) => (hint, if enabled { HINT } else { DISABLED }),
            (None, Some(hint)) => (hint, HINT),
            (None, None) => ("", HINT),
        };
        if !text.is_empty() {
            let s = self.place.scale;
            canvas.text_aligned(
                text,
                self.place.centered(geometry.hint, 560.0, 18.0),
                13.0 * s,
                color,
                FontWeight::Regular,
                0.3 * s,
                TextAlign::Center,
            );
        }
        canvas.pop_opacity();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fit_their_panel() {
        for frame in [Frame::Main, Frame::InGame] {
            let geometry = frame.geometry();
            let [_, top, _, height] = geometry.panel;
            let last = geometry.first_row + geometry.rows as f32 * geometry.row_height;
            assert!(geometry.first_row >= top, "{frame:?}");
            assert!(last <= top + height, "{frame:?}: {last} > {}", top + height);
            let (offset, width) = frame.slider_span();
            assert!(offset > 0.0 && offset + width < 1.0, "{frame:?}");
        }
    }

    #[test]
    fn slider_ratio_follows_the_bar() {
        let row = Rect::new(100.0, 0.0, 340.0, 14.0);
        let span = Frame::Main.slider_span();
        let left = row.x + row.width * span.0;
        let right = left + row.width * span.1;
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(close(slider_ratio(row, left, span), 0.0));
        assert!(close(slider_ratio(row, right, span), 1.0));
        assert!(close(slider_ratio(row, (left + right) * 0.5, span), 0.5));
        assert_eq!(slider_ratio(row, 0.0, span), 0.0);
        assert_eq!(slider_ratio(row, 1000.0, span), 1.0);
    }

    #[test]
    fn chrome_tokens_round_trip() {
        assert_eq!(chrome_slot(CHROME_BASE), Some(0));
        assert_eq!(chrome_slot(CHROME_BASE + 14), Some(14));
        assert_eq!(chrome_slot(799), None);
        assert_eq!(chrome_slot(900), None);
    }

    #[test]
    fn in_game_list_holds_only_groups() {
        for page in [Page::Setup, Page::Controls] {
            let frame = PanelFrame {
                frame: Frame::InGame,
                page,
                active: page.opening_panel().unwrap(),
                art: ArtSet::default(),
            };
            let rows: Vec<_> = (0..page.slots().len())
                .filter_map(|index| frame.list_row(index))
                .collect();
            let groups = page
                .slots()
                .iter()
                .filter(|slot| slot.size == Size::List)
                .count();
            assert_eq!(rows, (0..groups).collect::<Vec<_>>());
            let [_, y, _, height] = IN_GAME_LIST;
            let [_, box_y, _, box_height] = IN_GAME_BOX;
            assert!(y + groups as f32 * height <= box_y + box_height, "{page:?}");
        }
    }
}
