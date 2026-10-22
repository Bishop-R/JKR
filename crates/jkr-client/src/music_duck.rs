//! The music held down under a stinger (`CGCam_SetMusicMult` and `CG_SE_UpdateMusic`,
//! OpenJK `codemp/cgame/cg_view.c:1945-2058`): when the local player becomes the Jedi Master
//! or dies to someone (`cg_event.c:2434-2437`, `3328-3331`), the music plays at 0.3 of its
//! volume for five seconds, then comes back up a tenth every 200 ms (`s_musicMult`).

/// How far a stinger holds the music down, and for how long.
pub const STINGER_MULTIPLIER: f32 = 0.3;
pub const STINGER_MS: i64 = 5_000;
/// One step back up.
const STEP: f32 = 0.1;
const STEP_MS: i64 = 200;

/// `cgScreenEffects`' music fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicDuck {
    /// `music_volume_multiplier`: none until a stinger sets one.
    multiplier: f32,
    /// `music_volume_time`.
    until: i64,
    /// `music_volume_set`: the multiplier was applied.
    set: bool,
}

impl MusicDuck {
    /// `CGCam_SetMusicMult(multiplier, duration)` at `now` (milliseconds).
    pub fn set(&mut self, multiplier: f32, duration: i64, now: i64) {
        self.multiplier = multiplier.clamp(0.1, 1.0);
        self.until = now + duration;
        self.set = false;
    }

    /// `CG_SE_UpdateMusic` at `now`: the multiplier the music plays at from now
    /// (`s_musicMult`), which is whole until a stinger.
    pub fn update(&mut self, now: i64) -> f32 {
        if self.multiplier < 0.1 {
            self.multiplier = 1.0;
            return 1.0;
        }
        if self.until < now {
            if self.multiplier != 1.0 || self.set {
                self.multiplier = (self.multiplier + STEP).min(1.0);
                if self.multiplier == 1.0 {
                    self.set = false;
                } else {
                    self.until = now + STEP_MS;
                }
            }
            return self.multiplier;
        }
        self.set = true;
        self.multiplier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stinger_holds_the_music_down_then_lets_it_back_up() {
        let mut duck = MusicDuck::default();
        assert_eq!(duck.update(0), 1.0);
        duck.set(STINGER_MULTIPLIER, STINGER_MS, 1_000);
        assert_eq!(duck.update(1_016), 0.3);
        assert_eq!(duck.update(6_000), 0.3);
        // From 5 s on, a tenth every 200 ms, as the float sums it.
        assert_eq!(duck.update(6_016), 0.4);
        assert_eq!(duck.update(6_100), 0.4);
        let (mut now, mut last) = (6_016, 0.4);
        for _ in 0..6 {
            now += 201;
            last = duck.update(now);
        }
        assert_eq!(last, 1.0);
        assert_eq!(duck.update(9_000), 1.0);
        // A second stinger while the music comes back up starts over.
        duck.set(STINGER_MULTIPLIER, STINGER_MS, 10_000);
        assert_eq!(duck.update(10_016), 0.3);
    }
}
