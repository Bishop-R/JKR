//! The animation a body left on respawn plays (`CG_BodyQueueCopy`,
//! OpenJK `codemp/cgame/cg_servercmds.c:1256-1296`).
//!
//! A player may respawn a second after dying (`player_die`, `g_combat.c`:
//! `respawnTime = level.time + 1000`), while most death animations run for 1.3 to
//! 6 seconds. The body the respawn leaves behind (`ircg`) therefore usually copies a
//! player still in the middle of dying. cgame continues the source's death animation
//! on the body from the frame the source was showing (`ci->frame`, the torso frame
//! `CG_Player` keeps while `EF_DEAD`), to the end, and holds the last frame there
//! (`BONE_ANIM_OVERRIDE_FREEZE`). Only a source not in a death animation snaps to a
//! fixed pose (`BOTH_DEAD1`).
//!
//! JKR keeps the source's death clock instead of a frame: the body samples the same
//! clip on the same timeline, so it continues exactly where the dying player was.
//! cgame starts one whole frame later and blends to it over 150 ms; the difference
//! is under one frame and is not reproduced.

use jkr_runtime::{AnimationState, AnimationTrackState};

/// Resolve a body death pose without permitting a respawn's standing animation.
///
/// CG_BodyQueueCopy (`cg_servercmds.c:1256-1296`) falls back to BOTH_DEAD1.
/// This selects the completed pose, for a body whose source's death clock is unknown.
pub fn legacy_body_frame(config: &jkr_model::AnimationConfig, clip: usize) -> Option<usize> {
    if crate::animation_selection::death_animation(clip) {
        let sequence = config.get_exact(crate::legacy_animation_name(clip)?)?;
        Some(sequence.first_frame + sequence.frame_count.saturating_sub(1))
    } else {
        Some(config.get_exact("BOTH_DEAD1")?.first_frame)
    }
}

/// The death clock of a client whose body is being copied, from its presented
/// animation: the torso track (`source->currentState.torsoAnim`, `ci->frame`).
///
/// The `ircg` command can be handled after the snapshot that respawned the client
/// has been applied, when its torso already plays the respawn's animation; the
/// death it was cut from is then the track's outgoing transition. `None` for a
/// source in no death animation, which cgame snaps to `BOTH_DEAD1`.
pub fn legacy_body_clock(source: AnimationState) -> Option<AnimationTrackState> {
    let torso = source.upper;
    if torso.forced_frame.is_none() && crate::animation_selection::death_animation(torso.clip) {
        return Some(AnimationTrackState {
            transition: None,
            ..torso
        });
    }
    let previous = torso.transition?;
    (previous.forced_frame.is_none() && crate::animation_selection::death_animation(previous.clip))
        .then_some(AnimationTrackState {
            clip: previous.clip,
            revision: previous.revision,
            started_at_millis: previous.started_at_millis,
            phase_millis: previous.phase_millis,
            speed_milli: previous.speed_milli,
            forced_frame: None,
            transition: None,
        })
}

/// The animation a body presents: its source's death clock on every bone, as
/// `CG_BodyQueueCopy` sets one animation on `upper_lumbar`, `model_root` and
/// `Motion`; without one, the completed pose of [`legacy_body_frame`].
pub fn legacy_body_animation(
    state: AnimationState,
    clock: Option<AnimationTrackState>,
    config: &jkr_model::AnimationConfig,
) -> AnimationState {
    if let Some(clock) = clock {
        return AnimationState {
            lower: clock,
            upper: clock,
        };
    }
    let completed = |track: AnimationTrackState| AnimationTrackState {
        forced_frame: legacy_body_frame(config, track.clip),
        transition: None,
        ..track
    };
    AnimationState {
        lower: completed(state.lower),
        upper: completed(state.upper),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_runtime::AnimationTrackTransition;

    const BOTH_DEATH1: usize = 9;
    const BOTH_STAND1: usize = 915;

    fn index(name: &str) -> usize {
        crate::legacy_animation::NAMES
            .iter()
            .position(|known| *known == name)
            .unwrap()
    }

    fn track(clip: usize, started_at_millis: i64) -> AnimationTrackState {
        AnimationTrackState {
            clip,
            revision: clip as u64,
            started_at_millis,
            phase_millis: 0,
            speed_milli: 1_000,
            forced_frame: None,
            transition: None,
        }
    }

    fn config() -> jkr_model::AnimationConfig {
        jkr_model::AnimationConfig::parse(
            b"BOTH_DEATH1 100 41 -1 20\nBOTH_DEAD1 141 1 -1 20\nBOTH_STAND1 0 40 0 20\n",
        )
        .unwrap()
    }

    #[test]
    fn indices_match_the_animation_table() {
        assert_eq!(index("BOTH_DEATH1"), BOTH_DEATH1);
        assert_eq!(index("BOTH_STAND1"), BOTH_STAND1);
    }

    #[test]
    fn a_body_copied_mid_death_continues_the_source_clock() {
        // Died at 10 000, respawned (body copied) at 11 000 with 1 050 ms still to play.
        let dying = track(BOTH_DEATH1, 10_000);
        let source = AnimationState {
            lower: dying,
            upper: dying,
        };
        let clock = legacy_body_clock(source).unwrap();
        let body = legacy_body_animation(
            AnimationState {
                lower: track(BOTH_DEATH1, 11_000),
                upper: track(BOTH_DEATH1, 11_000),
            },
            Some(clock),
            &config(),
        );
        for track in [body.lower, body.upper] {
            assert_eq!(track.clip, BOTH_DEATH1);
            assert_eq!(track.forced_frame, None);
            assert_eq!(track.transition, None);
            assert_eq!(track.elapsed_millis(11_000), 1_000);
            assert_eq!(track.elapsed_millis(12_500), 2_500);
        }
    }

    #[test]
    fn the_death_is_found_behind_the_respawn_animation() {
        let respawned = AnimationTrackState {
            transition: Some(AnimationTrackTransition {
                clip: BOTH_DEATH1,
                revision: BOTH_DEATH1 as u64,
                started_at_millis: 10_000,
                phase_millis: 0,
                speed_milli: 1_000,
                forced_frame: None,
                blend_started_at_millis: 11_000,
                blend_duration_millis: 0,
            }),
            ..track(BOTH_STAND1, 11_000)
        };
        let clock = legacy_body_clock(AnimationState {
            lower: respawned,
            upper: respawned,
        })
        .unwrap();
        assert_eq!(clock.clip, BOTH_DEATH1);
        // Unlike the outgoing transition, the body's clock keeps running past the cut.
        assert_eq!(clock.elapsed_millis(11_500), 1_500);
    }

    #[test]
    fn a_source_in_no_death_has_no_clock() {
        let standing = track(BOTH_STAND1, 10_000);
        assert_eq!(
            legacy_body_clock(AnimationState {
                lower: standing,
                upper: standing,
            }),
            None
        );
    }

    #[test]
    fn without_a_clock_the_body_holds_the_completed_pose() {
        let config = config();
        let body = legacy_body_animation(
            AnimationState {
                lower: track(BOTH_DEATH1, 11_000),
                upper: track(BOTH_STAND1, 11_000),
            },
            None,
            &config,
        );
        assert_eq!(body.lower.forced_frame, Some(140));
        assert_eq!(body.upper.forced_frame, Some(141));
    }
}
