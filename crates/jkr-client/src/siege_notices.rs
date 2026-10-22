//! A Siege map's own announcements (`cg_saga.c`): the message and the announcer line its
//! `.siege` file gives for a completed objective (`CG_SiegeObjectiveCompleted`) and for
//! the end of a round (`CG_SiegeRoundOver`). The file is read as `CG_InitSiegeMode` reads
//! it, with the game's own siege text routines ([`jkr_game_jka::siege_text`]).

use crate::LegacyClientInfo;
use jkr_game_jka::siege_map::{SiegeMapInfo, SiegeSide};
use jkr_game_jka::siege_text;
use jkr_protocol::GameState;
use jkr_vfs::VirtualFileSystem;

const GT_SIEGE: i32 = 7;
/// `SIEGETEAM_TEAM1`; every other team reads as team two, as `cg_saga.c`'s `else` does.
const SIEGETEAM_TEAM1: u8 = 1;
const TEAM_SPECTATOR: u8 = 3;
/// `CG_PrecacheSiegeObjectiveAssetsForTeam` looks at `Objective1` to `Objective31`.
const MAX_PRECACHED_OBJECTIVE: i32 = 31;

/// What a client shows and plays for one Siege event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiegeNotice {
    /// The message as the file writes it; a leading `@` makes it a string reference.
    pub message: Option<String>,
    /// The announcer line, played on `CHAN_ANNOUNCER`.
    pub sound: Option<String>,
}

/// The current map's `.siege` file, while the game is Siege.
#[derive(Clone, Debug)]
pub struct SiegeNotices {
    map: SiegeMapInfo,
}

impl SiegeNotices {
    /// `CG_InitSiegeMode`: `maps/<mapname>.siege`, its sides named by the server's
    /// `g_siegeTeam1`/`g_siegeTeam2` when set (`CG_ParseServerinfo` copies them to
    /// `cg_siegeTeam1/2`). `None` outside Siege, or when the file is missing, too long or
    /// malformed (`siege_valid` stays 0).
    pub fn load(vfs: &VirtualFileSystem, game: &GameState) -> Option<Self> {
        let info = LegacyClientInfo::new(game.config_string(0)?);
        if info.integer("g_gametype") != Some(GT_SIEGE) {
            return None;
        }
        let path = format!("maps/{}.siege", info.text("mapname")?);
        let text = vfs.read(&path).ok()??.bytes;
        Self::parse(
            &text,
            ["g_siegeTeam1", "g_siegeTeam2"].map(|key| info.text(key).unwrap_or("")),
        )
    }

    /// A `.siege` file's text, `overrides` naming its sides as `g_siegeTeam1/2` do.
    pub fn parse(text: &[u8], overrides: [&str; 2]) -> Option<Self> {
        SiegeMapInfo::parse(text, overrides)
            .ok()
            .map(|map| Self { map })
    }

    /// `CG_SiegeObjectiveCompleted`: team `won` completed objective `objective`. The
    /// objective is found in the completing side's group, and its message and sound are
    /// the ones written for the viewer's own team (`message_team1`/`sound_team1` on team
    /// one, the `_team2` keys otherwise). A spectator is told nothing.
    pub fn objective_completed(&self, my_team: u8, won: i32, objective: i32) -> SiegeNotice {
        if my_team == TEAM_SPECTATOR {
            return SiegeNotice::default();
        }
        let Some(found) = objective_group(self.map.side(won), objective) else {
            return SiegeNotice::default();
        };
        let suffix = if my_team == SIEGETEAM_TEAM1 {
            "team1"
        } else {
            "team2"
        };
        let value = |key: String| siege_text::paired(&found, &key).ok().flatten();
        SiegeNotice {
            message: value(format!("message_{suffix}")),
            sound: value(format!("sound_{suffix}")).filter(|sound| !sound.is_empty()),
        }
    }

    /// `CG_SiegeRoundOver`: the round ended, `won` the winner. Both come from the viewer's
    /// own side's group: `wonround` and `roundover_sound_wewon` when its team won,
    /// `lostround` and `roundover_sound_welost` otherwise. A spectator is told nothing.
    pub fn round_over(&self, my_team: u8, won: i32) -> SiegeNotice {
        if my_team == TEAM_SPECTATOR {
            return SiegeNotice::default();
        }
        let side = self.map.side(i32::from(my_team));
        let (message, sound) = if i32::from(my_team) == won {
            ("wonround", "roundover_sound_wewon")
        } else {
            ("lostround", "roundover_sound_welost")
        };
        SiegeNotice {
            message: side.value(message).ok().flatten(),
            sound: side
                .value(sound)
                .ok()
                .flatten()
                .filter(|sound| !sound.is_empty()),
        }
    }

    /// `team1Timed` and `team2Timed` (`CG_InitSiegeMode`): each side's `Timed`, in
    /// milliseconds; zero for a side without a clock.
    pub fn timed(&self) -> [i32; 2] {
        [1, 2].map(|team| {
            self.map
                .side(team)
                .value("Timed")
                .ok()
                .flatten()
                .map_or(0, |value| {
                    jkr_game_jka::userinfo::atoi(value.as_bytes()).wrapping_mul(1_000)
                })
        })
    }

    /// Every announcer line the events above can play, each once, so that a client can
    /// load them with the map: both sides' objective lines, as
    /// `CG_PrecacheSiegeObjectiveAssetsForTeam` registers them, and their round lines,
    /// which stock registers when the round ends.
    pub fn sound_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for team in [1, 2] {
            let side = self.map.side(team);
            for key in ["roundover_sound_wewon", "roundover_sound_welost"] {
                paths.extend(side.value(key).ok().flatten());
            }
            for objective in 1..=MAX_PRECACHED_OBJECTIVE {
                let Some(found) = objective_group(side, objective) else {
                    break;
                };
                for key in ["sound_team1", "sound_team2"] {
                    paths.extend(siege_text::paired(&found, key).ok().flatten());
                }
            }
        }
        paths.retain(|path| !path.is_empty());
        paths.sort_unstable();
        paths.dedup();
        paths
    }
}

/// One of a side's objectives as the briefing lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiegeObjective {
    /// `N` of `ObjectiveN`.
    pub number: i32,
    /// `goalname` (`CG_SiegeGetObjectiveDescription`); a leading `@` makes it a string
    /// reference.
    pub goal: String,
    /// `final` above zero (`CG_SiegeGetObjectiveFinal`): the objective that wins the round.
    pub final_objective: bool,
}

impl SiegeNotices {
    /// The objectives `CG_SiegeBriefingDisplay` lists for `team` (`Objective1` to
    /// `Objective15`; team two for any team but one), leaving out those without a
    /// `goalname`.
    pub fn objectives(&self, team: u8) -> Vec<SiegeObjective> {
        let side = self.map.side(i32::from(team));
        (1..16)
            .filter_map(|number| {
                let found = objective_group(side, number)?;
                let value = |key: &str| siege_text::paired(&found, key).ok().flatten();
                let goal = value("goalname").filter(|goal| !goal.is_empty())?;
                let final_objective = value("final")
                    .is_some_and(|value| jkr_game_jka::userinfo::atoi(value.as_bytes()) > 0);
                Some(SiegeObjective {
                    number,
                    goal,
                    final_objective,
                })
            })
            .collect()
    }
}

/// `team<side>_objective<number>` as `CG_ParseSiegeObjectiveStatus` sets it from
/// `CS_SIEGE_OBJECTIVES`: each `-` starts the next objective, `1` after it completes it,
/// and `|` moves on to team two.
pub fn objective_complete(game: &GameState, side: u8, objective: i32) -> bool {
    let Some(status) = game.config_string(jkr_game_jka::siege_class::CS_SIEGE_OBJECTIVES) else {
        return false;
    };
    let (mut team, mut number) = (1u8, 0i32);
    let mut bytes = status.iter().peekable();
    while let Some(&byte) = bytes.next() {
        match byte {
            b'|' => {
                team = 2;
                number = 0;
            }
            b'-' => {
                number += 1;
                let done = bytes.next() == Some(&b'1');
                if team == side && number == objective {
                    return done;
                }
            }
            _ => {}
        }
    }
    false
}

/// `EV_SIEGE_ROUNDOVER` and `EV_SIEGE_OBJECTIVECOMPLETE` (`bg_public.h`).
pub(crate) const EV_SIEGE_ROUNDOVER: u16 = 101;
pub(crate) const EV_SIEGE_OBJECTIVECOMPLETE: u16 = 102;

/// The sound adapter's share of the notices: their announcer lines, interned with the
/// map's other sounds.
pub(crate) struct SiegeAnnouncer {
    notices: SiegeNotices,
    lines: Vec<(String, u16)>,
}

impl SiegeAnnouncer {
    /// The map's notices and every line they can play, interned through `intern`.
    pub(crate) fn load(
        vfs: &VirtualFileSystem,
        game: &GameState,
        mut intern: impl FnMut(&str) -> u16,
    ) -> Option<Self> {
        let notices = SiegeNotices::load(vfs, game)?;
        let lines = notices
            .sound_paths()
            .into_iter()
            .map(|path| {
                let sound = intern(&path);
                (path, sound)
            })
            .collect();
        Some(Self { notices, lines })
    }

    /// The line `S_StartLocalSound` plays on `CHAN_ANNOUNCER` for `event`, `won` being its
    /// `eventParm` and `objective` its `trickedentindex`; `None` for no line.
    pub(crate) fn line(&self, event: u16, my_team: u8, won: i32, objective: i32) -> Option<u16> {
        let notice = match event {
            EV_SIEGE_ROUNDOVER => self.notices.round_over(my_team, won),
            EV_SIEGE_OBJECTIVECOMPLETE => self.notices.objective_completed(my_team, won, objective),
            _ => return None,
        };
        let path = notice.sound?;
        self.lines
            .iter()
            .find(|(line, _)| *line == path)
            .map(|(_, sound)| *sound)
    }
}

/// `ObjectiveN` in a side's group.
fn objective_group(side: &SiegeSide, objective: i32) -> Option<Vec<u8>> {
    let group = side.group.as_ref()?;
    siege_text::value_group(group, format!("Objective{objective}").as_bytes())
        .ok()
        .flatten()
}

/// [`SiegeNotices`] for the map a client is on, read again whenever the server info
/// changes (a new map, game type or side override).
#[derive(Default)]
pub struct SiegeNoticeCache {
    serverinfo: Vec<u8>,
    notices: Option<SiegeNotices>,
}

impl SiegeNoticeCache {
    /// Read the map's file again if the server info changed since it was last read;
    /// compares the bytes and allocates only on a change.
    pub fn refresh(&mut self, vfs: &VirtualFileSystem, game: &GameState) {
        let serverinfo = game.config_string(0).unwrap_or_default();
        if serverinfo != self.serverinfo.as_slice() {
            self.serverinfo.clear();
            self.serverinfo.extend_from_slice(serverinfo);
            self.notices = SiegeNotices::load(vfs, game);
        }
    }

    /// The notices of the map last refreshed, while it is a Siege map.
    pub fn notices(&self) -> Option<&SiegeNotices> {
        self.notices.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the retail files: string-referenced side names, tabs, comments.
    const SIEGE: &str = "Teams\n{\n\tteam1 @SIEGE_TEST_TEAM1\n\tteam2 @SIEGE_TEST_TEAM2\n}\n\n\
        @SIEGE_TEST_TEAM1\n{\n\tTimed 600\n\
        \tObjective1\n\t{\n\t\tgoalname \"@SIEGE_TEST_OBJ_1\"\n\
        \t\tmessage_team1 \"@SIEGE_TEST_OBJ_1_ATTACKERS\"\n\
        \t\tmessage_team2 \"@SIEGE_TEST_OBJ_1_DEFENDERS\"\n\
        \t\tsound_team1 \"sound/chars/protocol/misc/siege2.mp3\"\n\
        \t\tsound_team2 \"sound/chars/protocol/misc/siege6.mp3\"\n\t}\n\
        \twonround \"@SIEGE_TEST_ATTACKERS_WIN\"\n\
        \tlostround \"@SIEGE_TEST_ATTACKERS_LOSE\"\n\
        \troundover_sound_wewon \"sound/chars/protocol/misc/40MOM014.mp3\" // won\n\
        \troundover_sound_welost \"sound/chars/protocol/misc/40MOM012.mp3\"\n}\n\n\
        @SIEGE_TEST_TEAM2\n{\n\twonround \"@SIEGE_TEST_DEFENDERS_WIN\"\n\
        \tlostround \"@SIEGE_TEST_DEFENDERS_LOSE\"\n\
        \troundover_sound_wewon \"sound/chars/protocol/misc/40MOM013.mp3\"\n\
        \troundover_sound_welost \"sound/chars/protocol/misc/40MOM012.mp3\"\n}\n";

    fn notices() -> SiegeNotices {
        SiegeNotices::parse(SIEGE.as_bytes(), ["none", ""]).expect("the test file parses")
    }

    fn notice(message: &str, sound: &str) -> SiegeNotice {
        SiegeNotice {
            message: Some(message.to_owned()),
            sound: Some(sound.to_owned()),
        }
    }

    #[test]
    fn an_objective_is_announced_in_the_viewers_own_words() {
        let notices = notices();
        // Team one completed its first objective: each side hears its own line.
        assert_eq!(
            notices.objective_completed(1, 1, 1),
            notice(
                "@SIEGE_TEST_OBJ_1_ATTACKERS",
                "sound/chars/protocol/misc/siege2.mp3"
            )
        );
        assert_eq!(
            notices.objective_completed(2, 1, 1),
            notice(
                "@SIEGE_TEST_OBJ_1_DEFENDERS",
                "sound/chars/protocol/misc/siege6.mp3"
            )
        );
        // Spectators, objectives the side lacks and a side without objectives: nothing.
        assert_eq!(notices.objective_completed(3, 1, 1), SiegeNotice::default());
        assert_eq!(notices.objective_completed(1, 1, 2), SiegeNotice::default());
        assert_eq!(notices.objective_completed(1, 2, 1), SiegeNotice::default());
    }

    #[test]
    fn a_round_end_is_announced_from_the_viewers_side() {
        let notices = notices();
        assert_eq!(
            notices.round_over(1, 1),
            notice(
                "@SIEGE_TEST_ATTACKERS_WIN",
                "sound/chars/protocol/misc/40MOM014.mp3"
            )
        );
        assert_eq!(
            notices.round_over(2, 1),
            notice(
                "@SIEGE_TEST_DEFENDERS_LOSE",
                "sound/chars/protocol/misc/40MOM012.mp3"
            )
        );
        assert_eq!(notices.round_over(3, 2), SiegeNotice::default());
    }

    #[test]
    fn every_line_is_listed_once_for_loading() {
        assert_eq!(
            notices().sound_paths(),
            [
                "sound/chars/protocol/misc/40MOM012.mp3",
                "sound/chars/protocol/misc/40MOM013.mp3",
                "sound/chars/protocol/misc/40MOM014.mp3",
                "sound/chars/protocol/misc/siege2.mp3",
                "sound/chars/protocol/misc/siege6.mp3",
            ]
        );
    }

    #[test]
    fn each_side_has_its_own_clock() {
        assert_eq!(notices().timed(), [600_000, 0]);
    }

    #[test]
    fn a_side_override_names_another_group() {
        // `g_siegeTeam2` naming team one's group: team two now reads team one's words.
        let notices = SiegeNotices::parse(SIEGE.as_bytes(), ["", "@SIEGE_TEST_TEAM1"])
            .expect("the test file parses");
        assert_eq!(
            notices.round_over(2, 2).message.as_deref(),
            Some("@SIEGE_TEST_ATTACKERS_WIN")
        );
    }
}
