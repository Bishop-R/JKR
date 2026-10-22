//! What a spectator is told. Free in the world, `CG_DrawSpectator` (`cg_draw.c`) says
//! "SPECTATOR" and how to join, or in a duel who is fighting whom and that the viewer
//! waits to play; following a player, `CG_DrawFollow` says so above that player's name
//! (in a power duel, which side the player is on) and the warmup notice gives way.
//! The duel panel (`duel_panel.rs`) draws the portraits, health bars and scores.

use super::*;
use jkr_protocol::PlayerState;

/// `PMF_FOLLOW`, `TEAM_SPECTATOR`, `CS_CLIENT_DUELISTS`, `GT_DUEL`, `GT_POWERDUEL`.
const PMF_FOLLOW: u16 = 4_096;
const TEAM_SPECTATOR: u8 = 3;
const CS_CLIENT_DUELISTS: usize = 30;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
/// `DUELTEAM_LONE`, `DUELTEAM_DOUBLE` (the `dt` key of a player's info).
const DUELTEAM_LONE: &str = "1";
const DUELTEAM_DOUBLE: &str = "2";

/// The spectator's two lines and the follower's two, empty when they do not show.
#[derive(Default)]
pub(crate) struct Lines {
    title: String,
    hint: String,
    follow: String,
    followed: String,
    /// The player shown is dead (`STAT_HEALTH`, a signed short, at or below 0).
    dead: bool,
}

impl Lines {
    pub(crate) fn new() -> Self {
        Self {
            title: String::with_capacity(96),
            hint: String::with_capacity(64),
            follow: String::with_capacity(32),
            followed: String::with_capacity(48),
            dead: false,
        }
    }

    /// `CG_Draw2D` draws the status (health, armour, Force, weapon, ammo) only for a
    /// living player in the game: not for a free spectator, nor while dead.
    pub(crate) fn hides_status(&self) -> bool {
        self.spectating() || self.dead
    }

    /// Whether the viewer follows a player (the warmup notice gives way).
    pub(crate) fn following(&self) -> bool {
        !self.follow.is_empty()
    }

    /// Whether the viewer is a free spectator.
    pub(crate) fn spectating(&self) -> bool {
        !self.title.is_empty()
    }

    /// The four lines, in `spectator_title`, `spectator_hint`, `follow_title` and
    /// `follow_name` order.
    pub(crate) fn text(&self, index: usize) -> &str {
        [&self.title, &self.hint, &self.follow, &self.followed]
            .get(index)
            .map_or("", |line| line.as_str())
    }

    /// The lines for `player` (the snapshot's player state) on this server.
    pub(crate) fn update(
        &mut self,
        game: &GameState,
        player: &PlayerState,
        localization: &Localization,
        colors: bool,
    ) {
        self.title.clear();
        self.hint.clear();
        self.follow.clear();
        self.followed.clear();
        self.dead = player.health() <= 0;
        let string = |key: &str, english: &'static str| {
            localization
                .strings
                .get(key)
                .filter(|text| !text.is_empty())
                .map_or(english, String::as_str)
        };
        let gametype = game
            .config_string(0)
            .and_then(|info| jkr_client::LegacyClientInfo::new(info).integer("g_gametype"))
            .unwrap_or(0);
        let duel = matches!(gametype, GT_DUEL | GT_POWERDUEL);
        if player.movement_flags() & PMF_FOLLOW != 0 {
            let client = player.client_num();
            let side = (gametype == GT_POWERDUEL)
                .then(|| {
                    game.config_string(1_131 + usize::from(client))
                        .and_then(|info| jkr_protocol::info_value(info, b"dt"))
                })
                .flatten();
            self.follow.push_str(match side {
                Some(side) if side == DUELTEAM_LONE.as_bytes() => {
                    string("FOLLOWINGLONE", "Following lone duelist")
                }
                Some(side) if side == DUELTEAM_DOUBLE.as_bytes() => {
                    string("FOLLOWINGDOUBLE", "Following paired duelist")
                }
                _ => string("FOLLOWING", "Following"),
            });
            family::name(
                &mut self.followed,
                &client_name_and_team(game, client).0,
                colors,
            );
            return;
        }
        if player.team() != TEAM_SPECTATOR {
            return;
        }
        // `cgs.duelist1`..`3`, -1 when none.
        let mut duelists = [-1_i32; 3];
        if let Some(value) = game
            .config_string(CS_CLIENT_DUELISTS)
            .and_then(|value| std::str::from_utf8(value).ok())
        {
            for (slot, number) in duelists.iter_mut().zip(value.split('|')) {
                *slot = number.parse().unwrap_or(-1);
            }
        }
        let name = |client: i32| client_name_and_team(game, client as u16).0;
        let [first, second, third] = duelists;
        if duel && first >= 0 && second >= 0 {
            family::name(&mut self.title, &name(first), colors);
            self.title.push_str("^7 ");
            self.title.push_str(string("SPECHUD_VERSUS", "versus"));
            self.title.push(' ');
            family::name(&mut self.title, &name(second), colors);
            if gametype == GT_POWERDUEL && third >= 0 {
                self.title.push_str("^7 ");
                self.title.push_str(string("AND", "and"));
                self.title.push(' ');
                family::name(&mut self.title, &name(third), colors);
            }
        } else {
            self.title.push_str(string("SPECTATOR", "SPECTATOR"));
        }
        self.hint.push_str(if duel {
            string("WAITING_TO_PLAY", "waiting to play")
        } else {
            string(
                "SPEC_CHOOSEJOIN",
                "Choose 'Join' from the menu to enter the game",
            )
        });
    }
}
