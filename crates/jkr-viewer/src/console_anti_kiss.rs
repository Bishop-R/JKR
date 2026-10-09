//! TaystJK's `cg_antiAmKiss` (`cg_view.c:2573-2579`): on a JA+ server, a player JA+'s
//! `amkiss` holds in the kissed pose (`BOTH_KISSEE`) pushes free at once (`force_throw`),
//! every frame the pose lasts.

use super::*;

/// `BOTH_KISSEE` in the animation table JA+, jaPRO and TaystJK extend (`anims.h:1757`).
const BOTH_KISSEE: u16 = 1493;

impl ViewerConsole {
    /// One look at the local player: with `cg_antiAmKiss` on, `force_throw` while it is held
    /// in the kissed pose, playing (not following, not spectating), on a JA+ server —
    /// `japlus` is only asked then.
    pub(crate) fn anti_am_kiss(
        &mut self,
        player: &jkr_protocol::PlayerState,
        japlus: impl FnOnce() -> bool,
    ) {
        const PMF_FOLLOW: u16 = 4096;
        let kissed = [player.leg_animation(), player.torso_animation()].contains(&BOTH_KISSEE);
        if !kissed || player.movement_flags() & PMF_FOLLOW != 0 || player.is_spectator() {
            return;
        }
        if self.integer_cvar("cg_antiAmKiss").unwrap_or(0) != 0 && japlus() {
            self.queue_script("force_throw\n");
        }
    }
}

#[cfg(test)]
mod tests {
    /// The kissed pose is the table's own `BOTH_KISSEE`.
    #[test]
    fn the_kissed_pose_is_the_tables() {
        assert_eq!(
            jkr_game_jka::legacy_animation_index("BOTH_KISSEE"),
            Some(usize::from(super::BOTH_KISSEE))
        );
    }
}
