use super::*;

const CONFIG: &str = "\
BOTH_GESTURE1 100 80 -1 20
BOTH_RUN1 10 20 0 20
BOTH_WALKBACK1 40 20 0 -20
";

fn config() -> AnimationConfig {
    AnimationConfig::parse(CONFIG.as_bytes()).unwrap()
}

fn parse(text: &str) -> LegacyAnimationEvents {
    LegacyAnimationEvents::parse(text, &config(), &mut |_| None)
}

#[test]
fn saber_spin_lines_become_spin_sounds_on_absolute_frames() {
    let events = parse(
        "UPPEREVENTS\n{\n\
         BOTH_GESTURE1 AEV_SOUNDCHAN 30 CHAN_AUTO sound/weapons/saber/saberspinoff.wav 0 0 0\n\
         BOTH_GESTURE1 AEV_SOUNDCHAN 41 CHAN_AUTO sound/weapons/saber/saberspinoff.wav 0 0 0\n\
         }\n",
    );
    let upper = events.track(true);
    assert_eq!(upper.len(), 2);
    assert_eq!(upper[0].key_frame, 130);
    assert_eq!(upper[1].key_frame, 141);
    assert_eq!(upper[0].sound, LegacyAnimationSound::SaberSpin { kind: 0 });
    assert!(events.track(false).is_empty());
}

#[test]
fn sound_lines_keep_variants_channels_and_chances() {
    let events = parse(
        "UPPEREVENTS {\n\
         BOTH_RUN1 AEV_SOUNDCHAN 2 CHAN_VOICE sound/weapons/melee/swing%d.wav 1 6 40\n\
         BOTH_RUN1 AEV_SOUND 4 sound/player/roll1.wav 0 0 0\n\
         BOTH_RUN1 AEV_SOUNDCHAN 5 CHAN_WEAPON sound/weapons/saber/saberhup%d.mp3 4 6 0\n\
         }",
    );
    let upper = events.track(true);
    assert_eq!(
        upper[0].sound,
        LegacyAnimationSound::File {
            paths: (1..=4)
                .map(|n| format!("sound/weapons/melee/swing{n}.wav"))
                .collect(),
            channel: CHAN_VOICE,
        }
    );
    assert_eq!(upper[0].probability, 40);
    assert_eq!(
        upper[1].sound,
        LegacyAnimationSound::File {
            paths: vec!["sound/player/roll1.wav".to_owned()],
            channel: CHAN_AUTO,
        }
    );
    assert_eq!(
        upper[2].sound,
        LegacyAnimationSound::SaberSwing { weight: 1 }
    );
}

#[test]
fn unknown_animations_custom_sounds_and_other_events_play_nothing() {
    let events = parse(
        "LOWEREVENTS\n{\n\
         BOTH_NOT_A_THING AEV_SOUND 1 sound/x.wav 0 0 0\n\
         BOTH_RUN1 AEV_FOOTSTEP 3 footstep_r 0\n\
         BOTH_RUN1 AEV_SOUNDCHAN 6 CHAN_AUTO *jump1.wav 0 0 0\n\
         }\n",
    );
    let lower = events.track(false);
    assert_eq!(lower.len(), 2);
    assert!(
        lower
            .iter()
            .all(|event| event.sound == LegacyAnimationSound::Silent)
    );
}

#[test]
fn a_later_line_on_the_same_frame_and_type_replaces_the_earlier() {
    let events = parse(
        "UPPEREVENTS {\n\
         BOTH_RUN1 AEV_SOUND 2 sound/a.wav 0 0 0\n\
         BOTH_RUN1 AEV_SOUND 3 sound/b.wav 0 0 0\n\
         BOTH_RUN1 AEV_SOUND 2 sound/c.wav 0 0 0\n\
         }",
    );
    let paths: Vec<_> = events
        .track(true)
        .iter()
        .map(|event| match &event.sound {
            LegacyAnimationSound::File { paths, .. } => paths[0].as_str(),
            _ => "",
        })
        .collect();
    assert_eq!(paths, ["sound/c.wav", "sound/b.wav"]);
}

#[test]
fn includes_fill_the_same_blocks() {
    let mut requested = Vec::new();
    let events = LegacyAnimationEvents::parse(
        "include _base // the shared file\n",
        &config(),
        &mut |name| {
            requested.push(name.to_owned());
            Some("UPPEREVENTS { BOTH_RUN1 AEV_SOUND 1 sound/a.wav 0 0 0 }".to_owned())
        },
    );
    assert_eq!(requested, ["_base"]);
    assert_eq!(events.track(true)[0].key_frame, 11);
}

#[test]
fn passage_fires_on_exact_frames_and_close_jumps_within_one_animation() {
    let forward = |old_frame, frame, same| LegacyFramePassage {
        old_frame,
        frame,
        same_animation: same,
    };
    assert!(forward(129, 130, None).fires(130));
    assert!(!forward(129, 131, None).fires(130));
    assert!(forward(129, 131, Some((false, None))).fires(130));
    assert!(!forward(120, 140, Some((false, None))).fires(130));
    // Wrapping round a loop of frames 10..30.
    assert!(forward(28, 11, Some((false, Some((10, 30))))).fires(29));
    // Backwards playback passes frames from high to low.
    assert!(forward(52, 49, Some((true, None))).fires(50));
}

#[test]
fn tracker_plays_a_spin_when_the_torso_reaches_its_frame() {
    let events = parse(
        "UPPEREVENTS {\n\
         BOTH_GESTURE1 AEV_SOUNDCHAN 30 CHAN_AUTO sound/weapons/saber/saberspinoff.wav 0 0 0\n\
         }",
    );
    let config = config();
    let clip = jkr_game_jka::legacy_animation_index("BOTH_GESTURE1").unwrap();
    let mut tracker = LegacyAnimationEventTracker::new(64);
    let mut played = Vec::new();
    for frame in [127.5, 128.2, 129.9, 130.4, 130.9, 131.0] {
        tracker.begin_frame();
        tracker.observe(3, true, clip, frame, &events, &config, |sound| {
            played.push((sound.path.to_owned(), sound.channel));
        });
    }
    assert_eq!(
        played,
        [("sound/weapons/saber/saberspinoff.wav".to_owned(), CHAN_AUTO)]
    );
}

#[test]
fn tracker_does_not_fire_for_a_track_it_lost_sight_of() {
    let events = parse("UPPEREVENTS { BOTH_GESTURE1 AEV_SOUND 30 sound/a.wav 0 0 0 }");
    let config = config();
    let clip = jkr_game_jka::legacy_animation_index("BOTH_GESTURE1").unwrap();
    let mut tracker = LegacyAnimationEventTracker::new(8);
    let mut count = 0;
    tracker.begin_frame();
    tracker.observe(1, true, clip, 129.0, &events, &config, |_| count += 1);
    tracker.begin_frame();
    tracker.begin_frame();
    tracker.observe(1, true, clip, 130.0, &events, &config, |_| count += 1);
    assert_eq!(count, 0);
}

#[test]
fn atoi_reads_leading_digits_like_c() {
    assert_eq!(atoi("42"), 42);
    assert_eq!(atoi("-7x"), -7);
    assert_eq!(atoi("x"), 0);
    assert_eq!(atoi(""), 0);
}
