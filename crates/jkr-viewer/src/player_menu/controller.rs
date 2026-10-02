//! Catalogue lifecycle, immediate cvar application and keyboard handling.

use super::rows::{CharacterRow, FORCE_POWER_ROW, FORCE_RESET_ROW, FORCE_SIDE_ROW};
use super::*;
use jkr_client::{ForceSide, LegacyCatalogStatus, LegacySpecies};
use jkr_vfs::VirtualFileSystem;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

impl PlayerMenu {
    pub(crate) fn attach_catalogue(&mut self, vfs: Arc<VirtualFileSystem>) {
        self.loader = Some(LegacyAssetCatalogLoader::new(Arc::clone(&vfs)));
        self.icon_vfs = Some(vfs);
    }

    pub(crate) fn open(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        self.return_target = target;
        self.read_console(console);
        self.saber.open(console);
        self.force.open(console);
        self.name_editing = false;
        self.channel_entry.cancel();
        self.page = ProfilePage::Character;
        self.selected = 0;
        self.resolved_catalogue = false;
        if let Some(loader) = &mut self.loader {
            loader.request();
        }
        self.resolve_current_choice();
        self.reconcile_saber_style();
        self.grid_follow = true;
        self.request_icons_if_ready();
    }

    /// Seed the drafts from the config at startup, so the stage model and
    /// its sabers are the player's own before this screen is ever opened,
    /// and start loading the catalogue those need resolving against.
    pub(crate) fn prime(&mut self, console: &ViewerConsole) {
        self.read_console(console);
        self.saber.open(console);
        if let Some(loader) = &mut self.loader {
            loader.request();
        }
    }

    /// Whether the catalogue has been matched against the drafts yet.
    pub(crate) fn is_resolved(&self) -> bool {
        self.resolved_catalogue
    }

    pub(crate) fn poll(&mut self) {
        if let Some(loader) = &mut self.loader {
            loader.poll();
        }
        self.resolve_current_choice();
        self.reconcile_saber_style();
        self.request_icons_if_ready();
        self.icons.poll();
    }

    pub(super) fn reconcile_saber_style(&mut self) {
        let catalog = catalog_of(&self.loader);
        self.saber.reconcile_style(catalog);
    }

    pub(super) fn status(&self) -> LegacyCatalogStatus {
        self.loader.as_ref().map_or(
            LegacyCatalogStatus::Failed,
            LegacyAssetCatalogLoader::status,
        )
    }

    fn read_console(&mut self, console: &ViewerConsole) {
        self.draft.name.clear();
        self.draft
            .name
            .push_str(console.text_value("name").unwrap_or("Padawan"));
        self.draft.model.clear();
        self.draft
            .model
            .push_str(console.text_value("model").unwrap_or("kyle/default"));
        self.draft.rgb = [
            cvar_u8(console, "char_color_red", 255),
            cvar_u8(console, "char_color_green", 255),
            cvar_u8(console, "char_color_blue", 255),
        ];
    }

    /// Write the character values to the console. Called after every change.
    pub(super) fn apply(&mut self, console: &mut ViewerConsole) {
        console.set_cvar("name", &self.draft.name);
        console.set_cvar("model", &self.draft.model);
        console.set_cvar("char_color_red", &self.draft.rgb[0].to_string());
        console.set_cvar("char_color_green", &self.draft.rgb[1].to_string());
        console.set_cvar("char_color_blue", &self.draft.rgb[2].to_string());
    }

    /// Match the `model` cvar against the catalogue once it is available.
    pub(super) fn resolve_current_choice(&mut self) {
        if self.resolved_catalogue {
            return;
        }
        let Some((choice, variants)) = self.catalog().map(|catalog| {
            let ordinary = catalog
                .characters
                .iter()
                .position(|entry| entry.cvar_value.eq_ignore_ascii_case(&self.draft.model));
            let species = catalog
                .species
                .iter()
                .enumerate()
                .find_map(|(index, species)| {
                    species_indices(species, &self.draft.model)
                        .map(|variants| (Choice::Species(index), variants))
                });
            if let Some(index) = ordinary {
                (Some(Choice::Character(index)), None)
            } else if let Some((choice, variants)) = species {
                (Some(choice), Some(variants))
            } else {
                (
                    (!catalog.characters.is_empty()).then_some(Choice::Character(0)),
                    None,
                )
            }
        }) else {
            return;
        };
        if let Some(variants) = variants {
            self.variants[..3].copy_from_slice(&variants);
        }
        self.choice = choice;
        self.resolved_catalogue = true;
        self.team = self.team_of_choice();
        self.rebuild_tiles();
        self.grid_follow = true;
    }

    /// Index of the current choice in the combined characters+species list.
    pub(super) fn choice_index(&self) -> usize {
        self.choice.map_or(0, |choice| match choice {
            Choice::Character(index) => index,
            Choice::Species(index) => self
                .catalog()
                .map_or(index, |catalog| catalog.characters.len() + index),
        })
    }

    /// Make the catalogue entry at `absolute` the draft model.
    pub(super) fn select_choice(&mut self, absolute: usize) {
        let Some(character_count) = self.catalog().map(|catalog| catalog.characters.len()) else {
            return;
        };
        if absolute < character_count {
            let value = self.catalog().unwrap().characters[absolute]
                .cvar_value
                .clone();
            self.choice = Some(Choice::Character(absolute));
            self.draft.model.clear();
            self.draft.model.push_str(&value);
        } else {
            let species_index = absolute - character_count;
            if self
                .catalog()
                .and_then(|catalog| catalog.species.get(species_index))
                .is_none()
            {
                return;
            }
            self.choice = Some(Choice::Species(species_index));
            self.variants = [0; 4];
            self.apply_species(species_index);
        }
        self.selected = self.selected.min(self.row_count().saturating_sub(1));
    }

    /// Step through the grid's tiles; a model the grid does not list (its
    /// skin belongs to another team) steps in from the grid's edge.
    fn cycle_model(&mut self, direction: isize) {
        let total = self.tiles.len();
        if total == 0 {
            return;
        }
        let slot = match self.tile_position() {
            Some(slot) => wrap(slot, direction, total),
            None if direction > 0 => 0,
            None => total - 1,
        };
        self.select_choice(self.tiles[slot]);
        self.grid_follow = true;
    }

    /// Pointer pick of grid slot `slot`: make it the model and write it.
    pub(super) fn pick_tile(&mut self, console: &mut ViewerConsole, slot: usize) {
        let Some(&absolute) = self.tiles.get(slot) else {
            return;
        };
        self.select_choice(absolute);
        self.apply(console);
    }

    /// Step one species axis (0 heads, 1 torsos, 2 legs, 3 skin colours).
    pub(super) fn cycle_variant(&mut self, axis: usize, direction: isize) {
        let Some(Choice::Species(species_index)) = self.choice else {
            return;
        };
        let Some(species) = self
            .catalog()
            .and_then(|catalog| catalog.species.get(species_index))
        else {
            return;
        };
        let length = match axis {
            0 => species.heads.len(),
            1 => species.torsos.len(),
            2 => species.legs.len(),
            3 => species.colors.len(),
            _ => 0,
        };
        if length == 0 {
            return;
        }
        self.variants[axis] = wrap(self.variants[axis], direction, length);
        self.apply_species(species_index);
    }

    fn apply_species(&mut self, species_index: usize) {
        let Some((model, rgb)) = self
            .catalog()
            .and_then(|catalog| catalog.species.get(species_index))
            .and_then(|species| {
                let (head, torso, legs) = selected_parts(species, self.variants)?;
                Some((
                    species.cvar_value(head, torso, legs),
                    species.colors.get(self.variants[3]).map(|color| color.rgb),
                ))
            })
        else {
            return;
        };
        self.draft.model.clear();
        self.draft.model.push_str(&model);
        if let Some(rgb) = rgb {
            self.draft.rgb = rgb;
        }
    }

    /// Step the selected row by `direction` and write the result at once.
    pub(super) fn adjust(&mut self, console: &mut ViewerConsole, direction: isize) {
        match self.page {
            ProfilePage::Character => {
                let Some(row) = self.character_rows().get(self.selected).copied() else {
                    return;
                };
                match row {
                    CharacterRow::Name => return,
                    CharacterRow::Team => self.cycle_team(direction),
                    CharacterRow::Model => self.cycle_model(direction),
                    _ => self.cycle_variant(row.axis().unwrap_or(0), direction),
                }
                self.apply(console);
            }
            ProfilePage::Saber => {
                let Some(row) = self.saber_rows().get(self.selected).copied() else {
                    return;
                };
                let catalog = catalog_of(&self.loader);
                self.saber.adjust(row, direction, catalog);
                self.saber.apply(console);
            }
            ProfilePage::Force => {
                match self.selected {
                    FORCE_SIDE_ROW => {
                        let side = match self.force.allocation().side {
                            ForceSide::Light => ForceSide::Dark,
                            ForceSide::Dark => ForceSide::Light,
                        };
                        self.force.set_side(side);
                    }
                    FORCE_RESET_ROW => return,
                    row => {
                        self.force.step(row - FORCE_POWER_ROW, direction > 0);
                    }
                }
                self.force.apply(console);
            }
        }
    }

    /// Enter/click on the selected row: edit the name, reset the Force
    /// profile, or step a cycler forward.
    pub(super) fn activate(&mut self, console: &mut ViewerConsole) {
        match (self.page, self.selected) {
            (ProfilePage::Character, 0) => {
                self.name_before_edit.clone_from(&self.draft.name);
                self.name_editing = true;
            }
            (ProfilePage::Force, FORCE_RESET_ROW) => {
                self.force.reset();
                self.force.apply(console);
            }
            _ => self.adjust(console, 1),
        }
    }

    pub(super) fn set_page(&mut self, page: ProfilePage) {
        self.page = page;
        self.selected = 0;
        self.name_editing = false;
        self.channel_entry.cancel();
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        if event.state != ElementState::Pressed {
            return PlayerMenuResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PlayerMenuResult::None;
        };
        if self.name_editing {
            return self.edit_name(event, key, console);
        }
        if let Some(row) = self.channel_entry.row() {
            let text = event.text.as_deref();
            self.channel_entry_key(row, key, text, event.repeat, console);
            return PlayerMenuResult::None;
        }
        if event.repeat {
            return PlayerMenuResult::None;
        }
        // Typing a number on a selected RGB channel starts entering it.
        if let Some(text) = &event.text {
            if self.begin_typed_channel(text) {
                return PlayerMenuResult::None;
            }
        }
        let count = self.row_count().max(1);
        let pages = ProfilePage::ALL.len();
        match key {
            KeyCode::Escape => return PlayerMenuResult::Back(self.return_target),
            KeyCode::Tab | KeyCode::BracketRight => {
                self.set_page(ProfilePage::ALL[(self.page.index() + 1) % pages]);
            }
            KeyCode::BracketLeft => {
                self.set_page(ProfilePage::ALL[(self.page.index() + pages - 1) % pages]);
            }
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1);
            }
            KeyCode::ArrowDown | KeyCode::KeyS => self.selected = (self.selected + 1) % count,
            KeyCode::ArrowLeft | KeyCode::KeyA => self.adjust(console, -1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.adjust(console, 1),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                // Enter types an RGB channel's value; Space still steps it.
                if key == KeyCode::Space || !self.begin_channel_entry(self.selected) {
                    self.activate(console);
                }
            }
            _ => {}
        }
        PlayerMenuResult::None
    }

    fn edit_name(
        &mut self,
        event: &KeyEvent,
        key: KeyCode,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        match key {
            KeyCode::Escape => {
                self.draft.name.clone_from(&self.name_before_edit);
                self.name_editing = false;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                self.name_editing = false;
                if self.draft.name.trim().is_empty() {
                    self.draft.name.clone_from(&self.name_before_edit);
                }
                self.apply(console);
            }
            KeyCode::Backspace => {
                self.draft.name.pop();
            }
            _ => {
                if let Some(text) = event.text.as_deref() {
                    append_legacy_name(&mut self.draft.name, text);
                }
            }
        }
        PlayerMenuResult::None
    }
}

pub(super) fn cvar_u8(console: &ViewerConsole, name: &str, fallback: u8) -> u8 {
    console
        .integer_cvar(name)
        .and_then(|value| u8::try_from(value).ok())
        .unwrap_or(fallback)
}

pub(super) fn wrap(index: usize, direction: isize, length: usize) -> usize {
    (index as isize + direction).rem_euclid(length as isize) as usize
}

fn selected_parts(species: &LegacySpecies, variants: [usize; 4]) -> Option<(&str, &str, &str)> {
    Some((
        species.heads.get(variants[0])?,
        species.torsos.get(variants[1])?,
        species.legs.get(variants[2])?,
    ))
}

fn species_indices(species: &LegacySpecies, model: &str) -> Option<[usize; 3]> {
    let parts = model.strip_prefix(&format!("{}/", species.model))?;
    let mut parts = parts.split('|');
    let head_name = parts.next()?;
    let torso_name = parts.next()?;
    let legs_name = parts.next()?;
    let head = species.heads.iter().position(|value| value == head_name)?;
    let torso = species
        .torsos
        .iter()
        .position(|value| value == torso_name)?;
    let legs = species.legs.iter().position(|value| value == legs_name)?;
    Some([head, torso, legs])
}

pub(super) fn append_legacy_name(name: &mut String, text: &str) {
    // OpenJK `q_shared.h` MAX_NAME_LENGTH is 32 including the NUL terminator.
    for character in text.chars().filter(|character| !character.is_control()) {
        if name.len() + character.len_utf8() > 31 {
            break;
        }
        name.push(character);
    }
}
