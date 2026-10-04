//! Searchable browser of console commands and cvars, opened with F3 in the open console
//! or with `consolebrowser`: names, descriptions, cvar values and defaults, and inline
//! editing of the selected cvar. Drawing is in `console_browser_view.rs`.

use crate::menu_widgets::MenuCanvas;
use jkr_shell::{CommandSource, CvarFlags, Shell};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "console_browser_pointer.rs"]
mod pointer;
#[path = "console_browser_view.rs"]
mod view;

/// Filter tabs; `Tab` and `Shift+Tab` cycle them.
const TABS: [&str; 4] = ["ALL", "COMMANDS", "CVARS", "CHANGED"];
/// Longest search text.
const FILTER_LIMIT: usize = 64;
/// Longest cvar value typed in place.
const VALUE_LIMIT: usize = 256;

/// What the console does after the browser handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum BrowserAction {
    None,
    /// Close the browser and return to the console.
    Close,
    /// Set the cvar to the typed value.
    Set {
        name: String,
        value: String,
    },
    /// Restore the cvar's default.
    Reset(String),
    /// Close the browser and start a console line with the command.
    Insert(String),
}

/// What one entry names.
enum Kind {
    Command(CommandSource),
    Cvar {
        value: String,
        default: String,
        read_only: bool,
        archived: bool,
    },
}

/// One command or cvar, formatted when the browser opens or a cvar changes so drawing
/// does not allocate.
struct Entry {
    name: String,
    description: String,
    /// Lowercase name and description, for case-insensitive search.
    search_name: String,
    search_description: String,
    kind: Kind,
}

impl Entry {
    fn changed(&self) -> bool {
        matches!(&self.kind, Kind::Cvar { value, default, .. } if value != default)
    }

    fn in_tab(&self, tab: usize) -> bool {
        match tab {
            1 => matches!(self.kind, Kind::Command(_)),
            2 => matches!(self.kind, Kind::Cvar { .. }),
            3 => self.changed(),
            _ => true,
        }
    }

    /// Search rank: name prefix, then name substring, then description substring.
    fn rank(&self, query: &str) -> Option<u8> {
        if query.is_empty() || self.search_name.starts_with(query) {
            Some(0)
        } else if self.search_name.contains(query) {
            Some(1)
        } else if self.search_description.contains(query) {
            Some(2)
        } else {
            None
        }
    }
}

pub(crate) struct Browser {
    open: bool,
    /// Every command and cvar, sorted by name.
    entries: Vec<Entry>,
    /// Indices into `entries` that the tab and search keep, in display order.
    visible: Vec<usize>,
    filter: String,
    tab: usize,
    /// Index into `visible`.
    selected: usize,
    /// First visible row on screen.
    first: usize,
    /// Rows that fit, as measured by the last `append`.
    rows: usize,
    /// The value being typed for the selected cvar.
    editing: Option<String>,
    /// `"<shown> of <total>"` line under the title.
    summary: String,
    /// Outcome of the last edit or refusal, shown in the footer.
    status: String,
    status_error: bool,
    ui: MenuCanvas,
}

impl Browser {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            entries: Vec::new(),
            visible: Vec::new(),
            filter: String::with_capacity(FILTER_LIMIT),
            tab: 0,
            selected: 0,
            first: 0,
            rows: 1,
            editing: None,
            summary: String::new(),
            status: String::new(),
            status_error: false,
            ui: MenuCanvas::with_text_capacity(256),
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Open on a fresh snapshot of `shell`, keeping the last tab and search.
    pub(crate) fn open(&mut self, shell: &Shell) {
        self.open = true;
        self.editing = None;
        self.status.clear();
        self.refresh(shell);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.editing = None;
    }

    pub(crate) fn draw_list(&self) -> &jkr_ui::DrawList {
        self.ui.draw_list()
    }

    /// Show the outcome of an action in the footer.
    pub(crate) fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = text.into();
        self.status_error = error;
    }

    /// Re-read every command and cvar from `shell`, keeping the selected entry.
    pub(crate) fn refresh(&mut self, shell: &Shell) {
        let selected = self.selected_entry().map(|entry| entry.name.clone());
        self.entries.clear();
        for (name, description, source) in shell.command_help() {
            self.entries
                .push(entry(name, description, Kind::Command(source)));
        }
        self.entries
            .sort_by(|left, right| left.search_name.cmp(&right.search_name));
        self.entries
            .dedup_by(|later, earlier| later.search_name == earlier.search_name);
        for cvar in shell.cvars.iter() {
            let kind = Kind::Cvar {
                value: cvar.value.as_text(),
                default: cvar.default.as_text(),
                read_only: cvar.flags.contains(CvarFlags::READ_ONLY),
                archived: cvar.flags.contains(CvarFlags::ARCHIVE),
            };
            self.entries
                .push(entry(&cvar.name, &cvar.description, kind));
        }
        self.entries
            .sort_by(|left, right| left.search_name.cmp(&right.search_name));
        self.rebuild_visible(selected.as_deref());
    }

    /// Apply the tab and search, then select `keep` again if it is still shown.
    fn rebuild_visible(&mut self, keep: Option<&str>) {
        let query = self.filter.to_lowercase();
        let mut ranked = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.in_tab(self.tab))
            .filter_map(|(index, entry)| entry.rank(&query).map(|rank| (rank, index)))
            .collect::<Vec<_>>();
        ranked.sort_by_key(|&(rank, _)| rank);
        self.visible.clear();
        self.visible
            .extend(ranked.into_iter().map(|(_, index)| index));
        self.selected = keep
            .and_then(|name| {
                self.visible
                    .iter()
                    .position(|&index| self.entries[index].name == name)
            })
            .unwrap_or(0);
        self.first = 0;
        self.reveal_selection();
        self.summary = format!(
            "{} of {} commands and cvars",
            self.visible.len(),
            self.entries.len()
        );
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(*self.visible.get(self.selected)?)
    }

    /// Move the selection by `rows` (negative = up), clamped to the list.
    fn move_selection(&mut self, rows: isize) {
        let last = self.visible.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(rows).min(last);
        self.reveal_selection();
    }

    /// Scroll so the selection is on screen.
    fn reveal_selection(&mut self) {
        let rows = self.rows.max(1);
        self.first = self
            .first
            .min(self.selected)
            .max((self.selected + 1).saturating_sub(rows));
    }

    /// Scroll by `rows` (negative = up) without moving the selection.
    fn scroll_by(&mut self, rows: isize) {
        let max_first = self.visible.len().saturating_sub(self.rows);
        self.first = self.first.saturating_add_signed(rows).min(max_first);
    }

    /// Scroll so the list shows `ratio` (0 = top, 1 = bottom) of the matches.
    fn scroll_to_ratio(&mut self, ratio: f32) {
        let max_first = self.visible.len().saturating_sub(self.rows);
        self.first = (ratio.clamp(0.0, 1.0) * max_first as f32).round() as usize;
    }

    fn set_tab(&mut self, tab: usize) {
        self.tab = tab % TABS.len();
        self.editing = None;
        let keep = self.selected_entry().map(|entry| entry.name.clone());
        self.rebuild_visible(keep.as_deref());
    }

    /// Enter on the selected entry: edit a cvar, insert a command.
    fn activate(&mut self) -> BrowserAction {
        let Some(entry) = self.selected_entry() else {
            return BrowserAction::None;
        };
        match &entry.kind {
            Kind::Command(_) => BrowserAction::Insert(entry.name.clone()),
            Kind::Cvar {
                read_only: true, ..
            } => {
                let text = format!("{} is read-only", entry.name);
                self.set_status(text, true);
                BrowserAction::None
            }
            Kind::Cvar { value, .. } => {
                self.editing = Some(value.clone());
                BrowserAction::None
            }
        }
    }

    /// Apply an active edit, or start editing/inserting the selected entry.
    fn accept(&mut self) -> BrowserAction {
        if let Some(value) = self.editing.take() {
            return self
                .selected_entry()
                .map_or(BrowserAction::None, |entry| BrowserAction::Set {
                    name: entry.name.clone(),
                    value: value.trim().to_owned(),
                });
        }
        self.activate()
    }

    /// Cancel only the edit first; otherwise return to the console.
    fn cancel(&mut self) -> BrowserAction {
        if self.editing.take().is_some() {
            BrowserAction::None
        } else {
            BrowserAction::Close
        }
    }

    /// Delete on the selected entry: restore a cvar's default.
    fn reset_selected(&mut self) -> BrowserAction {
        let Some(entry) = self.selected_entry() else {
            return BrowserAction::None;
        };
        match &entry.kind {
            Kind::Cvar {
                read_only: false, ..
            } => BrowserAction::Reset(entry.name.clone()),
            Kind::Cvar { .. } => {
                let text = format!("{} is read-only", entry.name);
                self.set_status(text, true);
                BrowserAction::None
            }
            Kind::Command(_) => BrowserAction::None,
        }
    }

    /// Handle a key while the browser is open; `shift` is the console's Shift state.
    pub(crate) fn handle_key(&mut self, event: &KeyEvent, shift: bool) -> BrowserAction {
        if event.state != ElementState::Pressed {
            return BrowserAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return BrowserAction::None;
        };
        if let Some(value) = &mut self.editing {
            match key {
                KeyCode::Escape => return self.cancel(),
                KeyCode::F3 if !event.repeat => return BrowserAction::Close,
                KeyCode::Enter | KeyCode::NumpadEnter => return self.accept(),
                KeyCode::Backspace => {
                    value.pop();
                }
                _ if !event.repeat => type_into(value, event, VALUE_LIMIT),
                _ => {}
            }
            return BrowserAction::None;
        }
        match key {
            KeyCode::Escape => return BrowserAction::Close,
            KeyCode::F3 if !event.repeat => return BrowserAction::Close,
            KeyCode::Enter | KeyCode::NumpadEnter => return self.activate(),
            KeyCode::Delete => return self.reset_selected(),
            KeyCode::ArrowUp => self.move_selection(-1),
            KeyCode::ArrowDown => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-(self.rows.max(1) as isize)),
            KeyCode::PageDown => self.move_selection(self.rows.max(1) as isize),
            KeyCode::Home => self.move_selection(isize::MIN),
            KeyCode::End => self.move_selection(isize::MAX),
            KeyCode::Tab => self.set_tab(if shift {
                self.tab + TABS.len() - 1
            } else {
                self.tab + 1
            }),
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.rebuild_visible(None);
                }
            }
            _ if !event.repeat => {
                let before = self.filter.len();
                type_into(&mut self.filter, event, FILTER_LIMIT);
                if self.filter.len() != before {
                    self.rebuild_visible(None);
                }
            }
            _ => {}
        }
        BrowserAction::None
    }
}

fn entry(name: &str, description: &str, kind: Kind) -> Entry {
    Entry {
        name: name.to_owned(),
        description: description.to_owned(),
        search_name: name.to_lowercase(),
        search_description: description.to_lowercase(),
        kind,
    }
}

/// Append the key's text to `field`, or the clipboard's on Ctrl+V; control characters
/// are dropped and `field` stays within `limit` bytes.
fn type_into(field: &mut String, event: &KeyEvent, limit: usize) {
    let pasted;
    let text = match event.text.as_deref() {
        Some("\u{16}") => {
            pasted = super::clipboard::paste().unwrap_or_default();
            pasted.as_str()
        }
        Some(text) => text,
        None => return,
    };
    for character in text.chars().filter(|character| !character.is_control()) {
        if field.len() + character.len_utf8() > limit {
            break;
        }
        field.push(character);
    }
}
