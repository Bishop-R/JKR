//! The "Objectives" page: what stock's `briefing` command and Siege objectives screen list
//! (`CG_SiegeBriefingDisplay`) — the player's side's objectives from the map's `.siege`
//! file, each marked done or open from `CS_SIEGE_OBJECTIVES`, the one that wins the round
//! marked as the final one.

use jkr_client::siege_notices::{SiegeNotices, objective_complete};
use jkr_protocol::GameState;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Info lines the page can carry: `Objective1` to `Objective15`.
const LINES: usize = 15;

/// Retained "status  /  goal" lines, refreshed when the page opens.
pub(super) struct State {
    lines: [String; LINES],
    len: usize,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            lines: std::array::from_fn(|_| String::with_capacity(96)),
            len: 0,
        }
    }

    /// The lines to show, in order.
    pub(super) fn lines(&self) -> &[String] {
        &self.lines[..self.len]
    }

    /// Rebuild the lines for `team`'s side of the map `notices` describe, its goals read
    /// through the string tables `strings`.
    pub(super) fn refresh(
        &mut self,
        notices: Option<&SiegeNotices>,
        game: &GameState,
        team: u8,
        strings: &HashMap<String, String>,
    ) {
        self.len = 0;
        let objectives = notices.map_or_else(Vec::new, |notices| notices.objectives(team));
        if objectives.is_empty() {
            self.push(format_args!("No objectives for this side"));
            return;
        }
        for objective in objectives {
            let status = if objective_complete(game, team, objective.number) {
                "Done"
            } else {
                "Open"
            };
            let goal = jkr_client::string_table::resolve(strings, &objective.goal);
            let last = if objective.final_objective {
                "  /  final"
            } else {
                ""
            };
            self.push(format_args!("{status}  /  {goal}{last}"));
        }
    }

    fn push(&mut self, text: std::fmt::Arguments<'_>) {
        if let Some(line) = self.lines.get_mut(self.len) {
            line.clear();
            let _ = line.write_fmt(text);
            self.len += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIEGE: &str = "Teams\n{\n\tteam1 \"Attackers\"\n\tteam2 \"Defenders\"\n}\n\
        Attackers\n{\n\
        \tObjective1\n\t{\n\t\tgoalname \"@TEST_DOOR\"\n\t\tfinal -1\n\t}\n\
        \tObjective2\n\t{\n\t\tgoalname \"Take the core\"\n\t\tfinal 1\n\t}\n\
        }\nDefenders\n{\n}\n";

    #[test]
    fn objectives_read_their_goal_status_and_finality() {
        let notices = SiegeNotices::parse(SIEGE.as_bytes(), ["", ""]).expect("siege file");
        let mut game = GameState::empty_local(0);
        game.replace_config_string(
            jkr_game_jka::siege_class::CS_SIEGE_OBJECTIVES,
            b"t1-1-0|t2".to_vec(),
        )
        .expect("objectives");
        let strings = HashMap::from([("TEST_DOOR".to_owned(), "Open the door".to_owned())]);
        let mut state = State::new();
        state.refresh(Some(&notices), &game, 1, &strings);
        assert_eq!(
            state.lines(),
            ["Done  /  Open the door", "Open  /  Take the core  /  final"]
        );
        state.refresh(Some(&notices), &game, 2, &strings);
        assert_eq!(state.lines(), ["No objectives for this side"]);
    }
}
