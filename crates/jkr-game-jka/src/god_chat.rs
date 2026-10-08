//! jaPRO's god chat (`g_godChat`; `g_active.c`, `g_combat.c`, `w_force.c`): in a
//! free-for-all, a player who has been typing — its chat field or console open, which
//! sends `BUTTON_TALK` and shows the talk balloon ([`crate::pmove_talk`]) — for more than
//! three seconds cannot be hurt outside a duel, and no Force reaches it; one typing at all
//! deals no damage and reaches no one with the Force. The three seconds are jaPRO's, "to
//! prevent some abuse". A DEMP2's shock, which `G_Damage` deals first, still lands. Off by
//! default, as in jaPRO.

use crate::pmove_talk::BUTTON_TALK;

/// How long a player types before nothing reaches it.
pub const GRACE_MS: i32 = 3_000;

/// A player's typing as god chat reads it (`pers.chatting`, `pers.lastChatTime`): since
/// when it has been typing while the rule holds for it; `None` otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Typing(Option<i32>);

impl Typing {
    /// A think at `level_time` on a command with `buttons`; `held` is whether the rule
    /// holds for the player (`g_godChat` on, a free-for-all, the player in it). Unlike
    /// jaPRO, which keeps the time of a player who stops playing while typing, the time
    /// is forgotten whenever the rule does not hold, so nobody comes back sheltered at
    /// once.
    pub fn think(&mut self, buttons: u16, held: bool, level_time: i32) {
        self.0 = match self.0 {
            _ if !held || buttons & BUTTON_TALK == 0 => None,
            Some(since) => Some(since),
            None => Some(level_time),
        };
    }

    /// Typing while the rule holds: its blows deal nothing and its Force reaches no one.
    pub fn typing(self) -> bool {
        self.0.is_some()
    }

    /// Typing for more than [`GRACE_MS`] at `level_time`: no Force reaches it, and
    /// nothing hurts it outside a duel (the caller's part).
    pub fn sheltered(self, level_time: i32) -> bool {
        self.0.is_some_and(|since| since + GRACE_MS < level_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelter_follows_three_seconds_of_typing_under_the_rule() {
        let mut typing = Typing::default();
        typing.think(BUTTON_TALK, true, 1_000);
        typing.think(BUTTON_TALK, true, 1_050);
        assert!(typing.typing());
        assert!(!typing.sheltered(4_000));
        assert!(typing.sheltered(4_001));
        // The balloon down ends it; raised again, the wait starts over.
        typing.think(0, true, 5_000);
        assert!(!typing.typing() && !typing.sheltered(9_000));
        typing.think(BUTTON_TALK, true, 6_000);
        assert!(!typing.sheltered(8_000) && typing.sheltered(9_001));
        // Outside the rule nothing is kept.
        typing.think(BUTTON_TALK, false, 9_100);
        assert_eq!(typing, Typing::default());
    }
}
