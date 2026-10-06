//! Siege objective and round announcements (`CG_SiegeObjectiveCompleted`,
//! `CG_SiegeRoundOver`). An objective's message is centre-printed at 30% of the screen
//! height (`CG_DrawSiegeMessageNonMenu`). A round's message, which stock shows in its
//! end-of-round popup (`siege_popmenu`), heads the intermission scoreboard here until
//! the intermission ends. Their announcer lines are the sound adapter's.

use super::ChatOverlay;
use jkr_client::siege_notices::SiegeNoticeCache;
use jkr_protocol::{EntityState, GameState, PlayerState};
use jkr_vfs::VirtualFileSystem;

const EV_SIEGE_ROUNDOVER: u16 = 101;
const EV_SIEGE_OBJECTIVECOMPLETE: u16 = 102;
const PM_INTERMISSION: u8 = 7;
/// `SCREEN_HEIGHT * 0.30`, where `CG_DrawSiegeMessageNonMenu` prints.
const OBJECTIVE_PRINT_HEIGHT: f32 = 144.0;

/// The map's notices and the ended round's message.
#[derive(Default)]
pub(super) struct Notices {
    cache: SiegeNoticeCache,
    round: Option<String>,
    /// Whether the intermission that shows `round` has begun.
    shown: bool,
}

impl ChatOverlay {
    /// Read the current map's `.siege` file again when the server info has changed.
    pub(crate) fn refresh_siege(&mut self, vfs: &VirtualFileSystem, game: &GameState) {
        self.combat.siege.cache.refresh(vfs, game);
    }

    /// The ended round's message, for the intermission scoreboard.
    pub(crate) fn round_notice(&self) -> Option<&str> {
        self.combat.siege.round.as_deref()
    }

    /// One newly seen Siege event (`EV_SIEGE_OBJECTIVECOMPLETE` or `EV_SIEGE_ROUNDOVER`).
    pub(super) fn siege_event(
        &mut self,
        event: u16,
        entity: &EntityState,
        player: &PlayerState,
        localization: &crate::Localization,
    ) {
        let Some(notices) = self.combat.siege.cache.notices() else {
            return;
        };
        let (team, won) = (player.team(), i32::from(entity.event_parameter()));
        match event {
            EV_SIEGE_OBJECTIVECOMPLETE => {
                let objective = i32::from(entity.tracked_entity_num());
                let notice = notices.objective_completed(team, won, objective);
                if let Some(message) = notice.message {
                    let text = siege_text(localization, &message);
                    self.announcement(&text, OBJECTIVE_PRINT_HEIGHT);
                }
            }
            EV_SIEGE_ROUNDOVER => {
                if let Some(message) = notices.round_over(team, won).message {
                    self.combat.siege.round = Some(siege_text(localization, &message));
                    self.combat.siege.shown = false;
                }
            }
            _ => {}
        }
    }

    /// Forget the round's message once its intermission is over, or the map changed.
    pub(super) fn observe_siege_round(&mut self, player: &PlayerState, restarted: bool) {
        let siege = &mut self.combat.siege;
        if restarted {
            siege.round = None;
            siege.shown = false;
        } else if player.movement_type() == PM_INTERMISSION {
            siege.shown = siege.round.is_some();
        } else if siege.shown {
            siege.round = None;
            siege.shown = false;
        }
    }
}

/// The text of a Siege message: a `@` reference is looked up, and one the tables lack
/// reads "??" (`CL_SE_GetStringTextString`).
fn siege_text(localization: &crate::Localization, message: &str) -> String {
    match message.strip_prefix('@') {
        Some(key) => jkr_client::string_table::lookup(&localization.strings, key)
            .unwrap_or("??")
            .to_owned(),
        None => message.to_owned(),
    }
}
