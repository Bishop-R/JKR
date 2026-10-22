//! A player breaking glass and weak breakables by running into them (`PM_ClientImpact`,
//! [`jkr_game_jka::client_impact`]): what each brush is to the blow before the move, and
//! the blows dealt once the move is over, through `G_Damage` as `MOD_CRUSH` with the
//! player standing where it was last linked.

use super::*;
use jkr_game_jka::breakables::Breakable;
use jkr_game_jka::client_impact::Brittle;
use jkr_game_jka::map_scenery::Glass;
use jkr_game_jka::means_of_death::MOD_CRUSH;

/// `GT_SIEGE`: nothing is hurt before its round begins.
const GT_SIEGE: i32 = 7;

/// What brush `number` is to a player's blow: a breakable, else a pane (`pane` finds
/// one), else nothing the blow can break. `round_waiting`: a Siege round has not begun.
pub(super) fn brittle<'a>(
    breakables: &[(EntityId, Breakable)],
    pane: impl Fn(u16) -> Option<&'a Glass>,
    round_waiting: bool,
    number: u16,
) -> Option<Brittle> {
    if let Some((_, brush)) = breakables
        .iter()
        .find(|(id, _)| id.legacy_number() == number)
    {
        return Some(Brittle::breakable(brush, round_waiting));
    }
    pane(number).map(|pane| Brittle::pane(pane, round_waiting))
}

/// Whether a Siege round has yet to begin (`gSiegeRoundBegun`), when `G_Damage` hurts
/// nothing.
pub(super) fn round_waiting(gametype: i32, round_begun: bool) -> bool {
    gametype == GT_SIEGE && !round_begun
}

impl NativeGame {
    /// The blows `client`'s move dealt, in order: a pane shattered from where the player
    /// stood (`pos1`) along its velocity (`pos2`), a breakable hurt with its chunks thrown
    /// from there.
    pub(super) fn deal_blows(&mut self, client: usize, origin: [f32; 3], level_time: i32) {
        if self.strikes.blows.is_empty() {
            return;
        }
        let blows = std::mem::take(&mut self.strikes.blows);
        // A blow `G_Damage` turned away (saber only, a Siege round not begun) did nothing.
        for blow in blows.iter().filter(|blow| blow.hurt) {
            let mut direction = blow.velocity;
            jkr_game_jka::player_angle_math::normalize(&mut direction);
            let owner = client as u16;
            if !self.hurt_glass_by(
                blow.entity,
                blow.damage,
                MOD_CRUSH,
                owner,
                Some((origin, direction)),
                level_time,
            ) {
                self.hurt_breakable_from(
                    blow.entity,
                    blow.damage,
                    MOD_CRUSH,
                    owner,
                    Some(origin),
                    level_time,
                );
            }
        }
        self.strikes.blows = blows;
        self.strikes.clear();
    }
}
