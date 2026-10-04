//! Pointer routing for the player form: hover selects, the wheel moves the
//! selection, tabs switch pages, a click on a cycler steps it toward the
//! half of the value zone that was hit, a click on the Force side picker
//! picks the card under it, a click or drag on a slider sets it from the
//! pointer, a click on a palette picks the chip under it, and the footer's
//! ESC cap goes back.

use super::force_view::side_direction;
use super::grid::{GRID_SCROLL_TOKEN, MODEL_ROW, TILE_BASE};
use super::rows::FORCE_SIDE_ROW;
use super::saber::PALETTE;
use super::*;
use crate::menu_widgets::{BACK_TOKEN, TAB_BASE, cycler_direction, palette_index};
use jkr_ui::{InputEvent, UiEventKind};

impl PlayerMenu {
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        let Some(event) = self.canvas.pointer(event) else {
            return PlayerMenuResult::None;
        };
        let Some(token) = event.token else {
            return PlayerMenuResult::None;
        };
        if event.kind == UiEventKind::Press
            && crate::menu_widgets::numeric::value_row(token).is_none()
        {
            self.numeric = None;
        }
        if event.kind == UiEventKind::Activate {
            if let Some(row) = crate::menu_widgets::numeric::value_row(token) {
                if self.numeric.as_ref().is_none_or(|edit| edit.row != row) {
                    self.begin_numeric(row);
                }
                return PlayerMenuResult::None;
            }
            self.numeric = None;
        } else if self.numeric.is_some() {
            return PlayerMenuResult::None;
        }
        let count = self.row_count();
        if event.kind == UiEventKind::Wheel {
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
            if token == GRID_SCROLL_TOKEN {
                self.scroll_grid(direction);
            } else if direction != 0 && count > 0 {
                self.selected = (self.selected as i32 + direction)
                    .clamp(0, count.saturating_sub(1) as i32)
                    as usize;
            }
            return PlayerMenuResult::None;
        }
        if let Some(tile) = (TILE_BASE..TAB_BASE)
            .contains(&token)
            .then(|| token - TILE_BASE)
        {
            return self.tile_event(event.kind, usize::from(tile), console);
        }
        let row = crate::menu_widgets::numeric::value_row(token).unwrap_or(usize::from(token));
        if matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover) {
            if row < count && !self.name_editing {
                self.selected = row;
            }
            return PlayerMenuResult::None;
        }
        if event.kind == UiEventKind::Drag {
            if crate::menu_widgets::numeric::value_row(token).is_some() {
                return PlayerMenuResult::None;
            }
            if let Some(position) = event.position.filter(|_| row < count) {
                self.selected = row;
                self.set_slider_from_pointer(console, token, position.x);
            }
            return PlayerMenuResult::None;
        }
        if event.kind != UiEventKind::Activate {
            return PlayerMenuResult::None;
        }
        if token == BACK_TOKEN {
            return PlayerMenuResult::Back(self.return_target);
        }
        if let Some(page) = (token >= TAB_BASE)
            .then(|| ProfilePage::ALL.get(usize::from(token - TAB_BASE)))
            .flatten()
        {
            self.set_page(*page);
            return PlayerMenuResult::None;
        }
        if row >= count {
            return PlayerMenuResult::None;
        }
        if self.name_editing {
            // A click elsewhere commits the name like Enter would.
            self.name_editing = false;
            self.apply(console);
        }
        self.selected = row;
        if let Some(position) = event.position {
            if self.set_slider_from_pointer(console, token, position.x) {
                return PlayerMenuResult::None;
            }
        }
        let side_picker = self.page == ProfilePage::Force && row == FORCE_SIDE_ROW;
        let direction = event
            .position
            .zip(self.canvas.rect_for(token))
            .filter(|_| self.selected_is_cycler())
            .map(|(position, rect)| {
                if side_picker {
                    side_direction(rect, position.x)
                } else {
                    cycler_direction(rect, position.x)
                }
            });
        match direction {
            Some(direction) => self.adjust(console, direction),
            None => self.activate(console),
        }
        PlayerMenuResult::None
    }

    /// Set the RGB channel slider under `token` from the pointer's x, or
    /// pick the palette chip there; false when the row is neither.
    fn set_slider_from_pointer(
        &mut self,
        console: &mut ViewerConsole,
        token: u16,
        pointer_x: f32,
    ) -> bool {
        if self.page != ProfilePage::Saber {
            return false;
        }
        let Some(row) = self.saber_rows().get(usize::from(token)).copied() else {
            return false;
        };
        let Some(rect) = self.canvas.rect_for(token) else {
            return false;
        };
        if let Some(channel) = row.channel() {
            let value = (self.canvas.slider_ratio(rect, pointer_x) * 255.0).round() as u8;
            self.saber.set_channel(row.second(), channel, value);
        } else if row.is_blade() {
            let chip = palette_index(rect, pointer_x, PALETTE.len());
            self.saber.select_color(row.second(), PALETTE[chip]);
        } else {
            return false;
        }
        self.saber.apply(console);
        true
    }

    /// Hovering a tile selects the Model row; a click makes it the model.
    fn tile_event(
        &mut self,
        kind: UiEventKind,
        tile: usize,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        match kind {
            UiEventKind::HoverEnter | UiEventKind::Hover if !self.name_editing => {
                self.selected = MODEL_ROW;
            }
            UiEventKind::Activate => {
                if self.name_editing {
                    self.name_editing = false;
                    self.apply(console);
                }
                self.selected = MODEL_ROW;
                self.pick_tile(console, tile);
            }
            _ => {}
        }
        PlayerMenuResult::None
    }
}
