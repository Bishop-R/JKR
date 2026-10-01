//! Retail's "Team Color" chooser over the model grid (`UI_SKIN_COLOR`,
//! `codemp/ui/ui_main.c`): the grid lists the ordinary skins of one team
//! (`default`, `red` or `blue`) plus every species, and switching teams
//! keeps the model, swapping in its skin for the new team when it has one.
//! Skins retail never listed (siege, boss, ...) are part of Default here
//! (owner request), so nothing the catalogue found is unreachable.

use super::controller::wrap;
use super::*;

/// Which skin set the grid lists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum TeamSkin {
    #[default]
    Default,
    Red,
    Blue,
}

impl TeamSkin {
    const ALL: [Self; 3] = [Self::Default, Self::Red, Self::Blue];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Red => "Red team",
            Self::Blue => "Blue team",
        }
    }

    /// The set a catalogue skin belongs to; anything but a team skin is
    /// Default.
    fn of(skin: &str) -> Self {
        match skin {
            "red" => Self::Red,
            "blue" => Self::Blue,
            _ => Self::Default,
        }
    }

    /// Skin name a model needs for this set.
    fn skin(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Red => "red",
            Self::Blue => "blue",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|team| *team == self).unwrap_or(0)
    }
}

impl PlayerMenu {
    /// Refill the grid's tile list for the current team from the catalogue:
    /// matching characters first (catalogue order), then every species.
    pub(super) fn rebuild_tiles(&mut self) {
        self.tiles.clear();
        let Some(catalog) = catalog_of(&self.loader) else {
            return;
        };
        let team = self.team;
        let characters = catalog
            .characters
            .iter()
            .enumerate()
            .filter(|(_, entry)| TeamSkin::of(&entry.skin) == team)
            .map(|(index, _)| index);
        let species = (0..catalog.species.len()).map(|index| catalog.characters.len() + index);
        self.tiles.extend(characters.chain(species));
    }

    /// Team set of the current choice: the character's skin, or the team
    /// already shown for a species (which belongs to every set).
    pub(super) fn team_of_choice(&self) -> TeamSkin {
        match (self.choice, self.catalog()) {
            (Some(Choice::Character(index)), Some(catalog)) => catalog
                .characters
                .get(index)
                .map_or(self.team, |entry| TeamSkin::of(&entry.skin)),
            _ => self.team,
        }
    }

    /// Whether any character of the catalogue belongs to `team`.
    fn team_has_characters(&self, team: TeamSkin) -> bool {
        self.catalog().is_some_and(|catalog| {
            catalog
                .characters
                .iter()
                .any(|entry| TeamSkin::of(&entry.skin) == team)
        })
    }

    /// Step the team chooser to the next set that has characters, moving
    /// the current model onto its skin for that set when it has one.
    pub(super) fn cycle_team(&mut self, direction: isize) {
        let mut index = self.team.index();
        for _ in 1..TeamSkin::ALL.len() {
            index = wrap(index, direction, TeamSkin::ALL.len());
            if self.team_has_characters(TeamSkin::ALL[index]) {
                break;
            }
        }
        self.team = TeamSkin::ALL[index];
        self.rebuild_tiles();
        self.grid_follow = true;
        let Some(Choice::Character(current)) = self.choice else {
            return;
        };
        let skin = self.team.skin();
        let matching = self.catalog().and_then(|catalog| {
            let model = &catalog.characters.get(current)?.model;
            catalog
                .characters
                .iter()
                .position(|entry| entry.model == *model && entry.skin == skin)
        });
        if let Some(absolute) = matching {
            self.select_choice(absolute);
        }
    }

    /// Slot of the current choice in the grid, if the grid lists it.
    pub(super) fn tile_position(&self) -> Option<usize> {
        let current = self.choice_index();
        self.tiles.iter().position(|&absolute| absolute == current)
    }
}
