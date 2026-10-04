//! Pointer routing on the classic profile pages: hovering an entry or one of
//! its cells focuses it, a click activates the entry or picks the cell, and
//! the wheel scrolls the list under it.

use super::view::{
    BLADE_BASE, HILT_BASE, HILTS_SCROLL, PART_BASE, PARTS_SCROLL, TINT_BASE, TINTS_SCROLL,
};
use super::{BLADE_SWATCHES, Item};
use crate::console::ViewerConsole;
use crate::player_menu::grid::{GRID_SCROLL_TOKEN, TILE_BASE};
use crate::player_menu::saber::{SaberStyle, allowed};
use crate::player_menu::{PlayerMenu, PlayerMenuResult, catalog_of};
use jkr_ui::{InputEvent, UiEventKind};

/// What a pointer token names on a classic page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Entry(usize),
    Tile(usize),
    Part(usize),
    Tint(usize),
    Hilt(bool, usize),
    Blade(bool, usize),
    Scroll(u16),
}

fn target(token: u16) -> Option<Target> {
    Some(match token {
        GRID_SCROLL_TOKEN | PARTS_SCROLL | TINTS_SCROLL => Target::Scroll(token),
        token if HILTS_SCROLL.contains(&token) => Target::Scroll(token),
        token if token >= BLADE_BASE[1] && token < BLADE_BASE[1] + 6 => {
            Target::Blade(true, usize::from(token - BLADE_BASE[1]))
        }
        token if token >= BLADE_BASE[0] && token < BLADE_BASE[0] + 6 => {
            Target::Blade(false, usize::from(token - BLADE_BASE[0]))
        }
        token if token >= HILT_BASE[1] && token < HILT_BASE[1] + 100 => {
            Target::Hilt(true, usize::from(token - HILT_BASE[1]))
        }
        token if token >= HILT_BASE[0] && token < HILT_BASE[0] + 100 => {
            Target::Hilt(false, usize::from(token - HILT_BASE[0]))
        }
        token if token >= TINT_BASE && token < TINT_BASE + 40 => {
            Target::Tint(usize::from(token - TINT_BASE))
        }
        token if token >= PART_BASE && token < PART_BASE + 60 => {
            Target::Part(usize::from(token - PART_BASE))
        }
        token if token >= TILE_BASE && token < TILE_BASE + 200 => {
            Target::Tile(usize::from(token - TILE_BASE))
        }
        token if token < 64 => Target::Entry(usize::from(token)),
        _ => return None,
    })
}

impl PlayerMenu {
    pub(in crate::player_menu) fn classic_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        let Some(event) = self.canvas.pointer(event) else {
            return PlayerMenuResult::None;
        };
        let Some(target) = event.token.and_then(target) else {
            return PlayerMenuResult::None;
        };
        if event.kind == UiEventKind::Wheel {
            let rows = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
            self.classic_scroll(target, rows, console);
            return PlayerMenuResult::None;
        }
        // The item a cell belongs to takes focus while it is hovered.
        let owner = match target {
            Target::Entry(index) => Some(index),
            Target::Tile(_) => self.focus_of(Item::Models),
            Target::Part(_) => self.focus_of(Item::Parts),
            Target::Tint(_) => self.focus_of(Item::Tints),
            Target::Hilt(second, _) => {
                self.focus_of(if second { Item::Hilts2 } else { Item::Hilts })
            }
            Target::Blade(second, _) => {
                self.focus_of(if second { Item::Blades2 } else { Item::Blades })
            }
            Target::Scroll(_) => None,
        };
        if matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover) {
            if let Some(index) = owner.filter(|_| !self.name_editing) {
                self.classic.focus = index;
            }
            return PlayerMenuResult::None;
        }
        if event.kind != UiEventKind::Activate {
            return PlayerMenuResult::None;
        }
        if self.name_editing {
            // A click elsewhere commits the name as Enter would.
            self.name_editing = false;
            self.apply(console);
        }
        if let Some(index) = owner {
            self.classic.focus = index;
        }
        match target {
            Target::Entry(_) => self.classic_activate(console),
            Target::Tile(slot) => {
                self.pick_tile(console, slot);
                PlayerMenuResult::None
            }
            Target::Part(index) => {
                self.set_variant(self.classic.part_axis, index);
                self.apply(console);
                PlayerMenuResult::None
            }
            Target::Tint(index) => {
                self.set_variant(3, index);
                self.apply(console);
                PlayerMenuResult::None
            }
            Target::Hilt(second, index) => {
                if let Some(catalog) = catalog_of(&self.loader) {
                    self.saber.select(catalog, index, second);
                    self.saber.apply(console);
                }
                PlayerMenuResult::None
            }
            Target::Blade(second, k) => {
                if let Some(index) = BLADE_SWATCHES.get(k) {
                    self.saber.select_color(second, *index);
                    self.saber.apply(console);
                }
                PlayerMenuResult::None
            }
            Target::Scroll(_) => PlayerMenuResult::None,
        }
    }

    fn focus_of(&self, item: Item) -> Option<usize> {
        self.classic_items().iter().position(|entry| *entry == item)
    }

    /// The wheel over a list: the head grid scrolls by rows; the part,
    /// tint and hilt lists step their choice (they follow it), written at
    /// once like the arrow keys.
    fn classic_scroll(&mut self, target: Target, rows: i32, console: &mut ViewerConsole) {
        if rows == 0 {
            return;
        }
        match target {
            Target::Scroll(GRID_SCROLL_TOKEN) | Target::Tile(_) => self.scroll_grid(rows),
            Target::Scroll(PARTS_SCROLL) | Target::Part(_) => {
                self.cycle_variant(self.classic.part_axis, rows as isize);
                self.apply(console);
            }
            Target::Scroll(TINTS_SCROLL) | Target::Tint(_) => {
                self.cycle_variant(3, rows as isize);
                self.apply(console);
            }
            Target::Scroll(token) if HILTS_SCROLL.contains(&token) => {
                self.scroll_hilts(token == HILTS_SCROLL[1], rows, console);
            }
            Target::Hilt(second, _) => self.scroll_hilts(second, rows, console),
            _ => {}
        }
    }

    /// Wheel over a hilt list steps the chosen hilt, as the list follows
    /// the choice.
    fn scroll_hilts(&mut self, second: bool, rows: i32, console: &mut ViewerConsole) {
        let style = if second {
            SaberStyle::Single
        } else {
            self.saber.style()
        };
        let Some(catalog) = catalog_of(&self.loader) else {
            return;
        };
        let count = catalog
            .saber_hilts
            .iter()
            .filter(|hilt| allowed(hilt, style))
            .count();
        if count == 0 {
            return;
        }
        let current = catalog
            .saber_hilts
            .iter()
            .filter(|hilt| allowed(hilt, style))
            .position(|hilt| hilt.name.eq_ignore_ascii_case(self.saber.hilt(second)))
            .unwrap_or(0);
        let next = (current as i32 + rows).clamp(0, count as i32 - 1) as usize;
        self.saber.select(catalog, next, second);
        self.saber.apply(console);
    }
}
