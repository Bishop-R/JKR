//! Row models of the three pages: which rows exist for the current state
//! and what each one is called. Row tokens are row indices on every page.

use super::*;
use jkr_client::SaberColor;

/// Rows of the character page; the part rows only exist for a species.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CharacterRow {
    Name,
    Team,
    Model,
    Head,
    Torso,
    Legs,
    Skin,
}

impl CharacterRow {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Team => "Team colour",
            Self::Model => "Model",
            Self::Head => "Head",
            Self::Torso => "Torso",
            Self::Legs => "Legs",
            Self::Skin => "Skin colour",
        }
    }

    /// Species variant axis this row cycles, if any.
    pub(super) fn axis(self) -> Option<usize> {
        match self {
            Self::Head => Some(0),
            Self::Torso => Some(1),
            Self::Legs => Some(2),
            Self::Skin => Some(3),
            Self::Name | Self::Team | Self::Model => None,
        }
    }
}

const CHARACTER_ROWS: [CharacterRow; 3] =
    [CharacterRow::Name, CharacterRow::Team, CharacterRow::Model];
const SPECIES_ROWS: [CharacterRow; 7] = [
    CharacterRow::Name,
    CharacterRow::Team,
    CharacterRow::Model,
    CharacterRow::Head,
    CharacterRow::Torso,
    CharacterRow::Legs,
    CharacterRow::Skin,
];

/// Rows of the saber page; the second-saber rows only exist for Dual. The
/// channel sliders are always there: they show the stock colour's tint and
/// moving one makes the colour custom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SaberRow {
    Style,
    Hilt,
    Blade,
    Red,
    Green,
    Blue,
    SecondHilt,
    SecondBlade,
    SecondRed,
    SecondGreen,
    SecondBlue,
}

impl SaberRow {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Style => "Style",
            Self::Hilt => "Hilt",
            Self::Blade => "Blade colour",
            Self::Red | Self::SecondRed => "Red",
            Self::Green | Self::SecondGreen => "Green",
            Self::Blue | Self::SecondBlue => "Blue",
            Self::SecondHilt => "Second hilt",
            Self::SecondBlade => "Second blade colour",
        }
    }

    pub(super) fn second(self) -> bool {
        matches!(
            self,
            Self::SecondHilt
                | Self::SecondBlade
                | Self::SecondRed
                | Self::SecondGreen
                | Self::SecondBlue
        )
    }

    /// Whether this row is a blade colour palette.
    pub(super) fn is_blade(self) -> bool {
        matches!(self, Self::Blade | Self::SecondBlade)
    }

    /// RGB channel this row slides, if it is a channel row.
    pub(super) fn channel(self) -> Option<usize> {
        match self {
            Self::Red | Self::SecondRed => Some(0),
            Self::Green | Self::SecondGreen => Some(1),
            Self::Blue | Self::SecondBlue => Some(2),
            _ => None,
        }
    }
}

const SABER_ROW_CAPACITY: usize = 11;

/// The saber page's rows for the current draft, built without allocating.
#[derive(Clone, Copy, Debug)]
pub(super) struct SaberRows {
    rows: [SaberRow; SABER_ROW_CAPACITY],
    len: usize,
}

impl SaberRows {
    fn push(&mut self, row: SaberRow) {
        self.rows[self.len] = row;
        self.len += 1;
    }

    fn extend(&mut self, rows: [SaberRow; 3]) {
        rows.into_iter().for_each(|row| self.push(row));
    }
}

impl std::ops::Deref for SaberRows {
    type Target = [SaberRow];

    fn deref(&self) -> &[SaberRow] {
        &self.rows[..self.len]
    }
}

/// Row index of the Force side picker.
pub(super) const FORCE_SIDE_ROW: usize = 0;
/// Row index of the first Force power; power `i` is row `FORCE_POWER_ROW + i`.
pub(super) const FORCE_POWER_ROW: usize = 1;
/// Row index of the Force reset action, the first of the action buttons.
pub(super) const FORCE_RESET_ROW: usize = 19;
/// Row index of the action returning the draft to the applied profile.
pub(super) const FORCE_DISCARD_ROW: usize = 20;
/// Row index of the action writing the draft to `forcepowers`.
pub(super) const FORCE_APPLY_ROW: usize = 21;
const FORCE_ROWS: usize = 22;

impl PlayerMenu {
    pub(super) fn character_rows(&self) -> &'static [CharacterRow] {
        match self.choice {
            Some(Choice::Species(_)) => &SPECIES_ROWS,
            _ => &CHARACTER_ROWS,
        }
    }

    pub(super) fn saber_rows(&self) -> SaberRows {
        let mut rows = SaberRows {
            rows: [SaberRow::Style; SABER_ROW_CAPACITY],
            len: 0,
        };
        rows.push(SaberRow::Style);
        rows.push(SaberRow::Hilt);
        rows.push(SaberRow::Blade);
        rows.extend([SaberRow::Red, SaberRow::Green, SaberRow::Blue]);
        if self.saber.style() == saber::SaberStyle::Dual {
            rows.push(SaberRow::SecondHilt);
            rows.push(SaberRow::SecondBlade);
            rows.extend([
                SaberRow::SecondRed,
                SaberRow::SecondGreen,
                SaberRow::SecondBlue,
            ]);
        }
        rows
    }

    /// Number of selectable rows on the current page.
    pub(super) fn row_count(&self) -> usize {
        match self.page {
            ProfilePage::Character => self.character_rows().len(),
            ProfilePage::Saber => self.saber_rows().len(),
            ProfilePage::Force => FORCE_ROWS,
        }
    }

    /// Whether the selected row is a `<  >` cycler (as opposed to a text
    /// field or an action).
    pub(super) fn selected_is_cycler(&self) -> bool {
        match self.page {
            ProfilePage::Character => self
                .character_rows()
                .get(self.selected)
                .is_some_and(|row| *row != CharacterRow::Name),
            ProfilePage::Saber => self
                .saber_rows()
                .get(self.selected)
                .is_some_and(|row| row.channel().is_none() && !row.is_blade()),
            ProfilePage::Force => self.selected < FORCE_RESET_ROW,
        }
    }
}

/// Swatch of a `color1`/`color2` value; `rgb` is the custom tint for RGB.
pub(super) fn saber_color(index: u8, rgb: [u8; 3]) -> jkr_ui::Color {
    use jkr_ui::Color;
    match SaberColor::from_index(index).unwrap_or(SaberColor::Blue) {
        SaberColor::Red => Color::new(1.0, 0.12, 0.12, 1.0),
        SaberColor::Orange => Color::new(1.0, 0.45, 0.08, 1.0),
        SaberColor::Yellow => Color::new(1.0, 0.87, 0.12, 1.0),
        SaberColor::Green => Color::new(0.12, 0.92, 0.32, 1.0),
        SaberColor::Blue => Color::new(0.16, 0.48, 1.0, 1.0),
        SaberColor::Purple => Color::new(0.72, 0.24, 1.0, 1.0),
        SaberColor::Rgb => {
            let [red, green, blue] = rgb.map(|channel| f32::from(channel) / 255.0);
            Color::new(red, green, blue, 1.0)
        }
    }
}
