//! jaPRO's CTF scores (`g_fixCTFScores`, `g_team.c`): smaller points for captures (15)
//! and defence (1 each, 4 for fragging the carrier), a grab scored by the grabber's speed
//! and whether it was an "e-grab" (the grabber's own flag gone), no point lost for a
//! suicide, and announcements of their own in place of the stock CTF messages — among
//! them the flag timer's capture line (time held, top and average speed across), which
//! jaPRO prints for every capture of a flag grabbed at its base. Here the timer runs only
//! with these scores on, so a stock game keeps stock's capture message.

/// `TEAM_RED`.
const TEAM_RED: i32 = 1;
/// The capture's points.
pub const CAPTURE: i32 = 15;
/// Fragging the enemy carrier's points.
pub const FRAG_CARRIER: i32 = 4;
/// A defence's points: the carrier's danger, the base, the carrier's escort.
pub const DEFENCE: i32 = 1;
/// `CTF_RECOVERY_BONUS`: what a return gives, whatever the announcement says.
const RECOVERY: f32 = 10.0;

/// The colour word of the flag of `team`.
fn flag_word(team: i32) -> &'static [u8] {
    if team == TEAM_RED {
        b"^1red"
    } else {
        b"^4blue"
    }
}

/// `print "<text>"` for everyone.
fn print(parts: &[&[u8]]) -> Vec<u8> {
    let mut text = b"print \"".to_vec();
    for part in parts {
        text.extend_from_slice(part);
    }
    text.extend_from_slice(b"\"");
    text
}

/// `Team_TouchEnemyFlag`'s grab: `name` took `team`'s flag at `speed`, which stood at
/// its base (`at_base`) while the grabber's own flag was there too (`own_at_base`).
/// Points for speed (1 above 1000, 2 above 2000), one more for an e-grab; the
/// announcement, sent before the flag's status changes.
pub fn grab(
    name: &[u8],
    team: i32,
    speed: f32,
    at_base: bool,
    own_at_base: bool,
) -> (i32, Vec<u8>) {
    let mut points = if speed > 2_000.0 {
        2
    } else if speed > 1_000.0 {
        1
    } else {
        0
    };
    let ups = format!("{speed:.0}");
    let word = flag_word(team);
    let text = if at_base && own_at_base {
        print(&[
            name,
            b" ^5grabbed the ",
            word,
            b"^5 flag at ^3",
            ups.as_bytes(),
            b"^5 ups (+^3",
            points.to_string().as_bytes(),
            b"^5)\n",
        ])
    } else if !own_at_base {
        points += 1;
        print(&[
            name,
            b" ^5e-grabbed the ",
            word,
            b"^5 flag at ^3",
            ups.as_bytes(),
            b"^5 ups (+^3",
            points.to_string().as_bytes(),
            b"^5)\n",
        ])
    } else {
        print(&[name, b" ^5grabbed the ", word, b"^5 flag\n"])
    };
    (points, text)
}

/// `Team_TouchOurFlag`'s return: `team`'s dropped flag lay `own` from its base and
/// `enemy` from the other. The announcement's share of the way and its points; the
/// return still gives the full `CTF_RECOVERY_BONUS`, as in jaPRO.
pub fn returned(name: &[u8], team: i32, own: f32, enemy: f32) -> Vec<u8> {
    let percent = if enemy + own != 0.0 {
        own / (enemy + own) * 100.0
    } else {
        1.0
    };
    let word = flag_word(team);
    if percent < 10.0 {
        return print(&[
            name,
            b"^5 has returned the ",
            word,
            b"^5 flag ^3<10^5 percent of the way to enemy base (+^30^5)\n",
        ]);
    }
    let points = (RECOVERY * percent * 0.01 + 0.5) as i32;
    print(&[
        name,
        b"^5 has returned the ",
        word,
        b"^5 flag ^3",
        format!("{percent:.0}").as_bytes(),
        b"^5 percent of the way to enemy base (+^3",
        points.to_string().as_bytes(),
        b"^5)\n",
    ])
}

/// jaPRO's flag timer (`pers.stats.startTimeFlag`, `topSpeedFlag`, `displacementFlag`
/// and its samples): from a grab at the base to the capture.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlagTimer {
    /// When the flag was grabbed at its base; 0 while none is timed.
    pub start: i32,
    top: f32,
    displacement: f32,
    samples: i32,
}

impl FlagTimer {
    /// A grab: timed from now if the flag stood at its base, else not at all.
    pub fn grabbed(&mut self, at_base: bool, level_time: i32) {
        if at_base {
            self.start = level_time;
            self.top = 0.0;
            self.displacement = 0.0;
        } else {
            self.start = 0;
        }
    }

    /// `G_RunFrame`'s sample, once a frame at `fps` (`sv_fps`) in CTF: the speed across
    /// while timed; cleared otherwise.
    pub fn sample(&mut self, velocity: [f32; 3], fps: f32) {
        if self.start == 0 {
            *self = Self::default();
            return;
        }
        let across = (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt();
        self.displacement += across / fps;
        self.samples += 1;
        if across > self.top {
            self.top = across;
        }
    }

    /// The capture's announcement by `name` of `team`'s flag at `level_time`, timed if
    /// the timer runs (and then stopped), else the plain one.
    pub fn captured(&mut self, name: &[u8], team: i32, level_time: i32, fps: f32) -> Vec<u8> {
        let word = flag_word(team);
        if self.start == 0 {
            return print(&[name, b"^5 has captured the ", word, b"^5 flag (+^315^5)\n"]);
        }
        let seconds = (level_time - self.start) as f32 / 1_000.0;
        let average = if self.samples != 0 {
            ((self.displacement * fps) / self.samples as f32 + 0.5).floor() as i32
        } else {
            self.top as i32
        };
        let top = (self.top + 0.5).floor() as i32;
        *self = Self::default();
        print(&[
            name,
            b"^5 has captured the ",
            word,
            b"^5 flag in ^3",
            format!("{seconds:.2}").as_bytes(),
            b"^5 seconds with max of ^3",
            top.to_string().as_bytes(),
            b"^5 ups and average ^3",
            average.to_string().as_bytes(),
            b"^5 ups (+^3",
            CAPTURE.to_string().as_bytes(),
            b"^5)\n",
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grabs_are_scored_by_speed_and_e_grabs() {
        let (points, text) = grab(b"Kyle", 1, 1_500.0, true, true);
        assert_eq!(points, 1);
        assert_eq!(
            text,
            b"print \"Kyle ^5grabbed the ^1red^5 flag at ^31500^5 ups (+^31^5)\n\"".to_vec()
        );
        let (points, text) = grab(b"Kyle", 2, 2_500.0, true, false);
        assert_eq!(points, 3);
        assert!(text.starts_with(b"print \"Kyle ^5e-grabbed the ^4blue^5 flag at ^32500"));
        let (points, text) = grab(b"Kyle", 1, 100.0, false, true);
        assert_eq!(points, 0);
        assert_eq!(
            text,
            b"print \"Kyle ^5grabbed the ^1red^5 flag\n\"".to_vec()
        );
    }

    #[test]
    fn returns_tell_how_far_and_captures_time_the_run() {
        assert_eq!(
            returned(b"Jan", 1, 300.0, 700.0),
            b"print \"Jan^5 has returned the ^1red^5 flag ^330^5 percent of the way to enemy base (+^33^5)\n\"".to_vec()
        );
        assert!(
            returned(b"Jan", 2, 50.0, 950.0)
                .ends_with(b"^3<10^5 percent of the way to enemy base (+^30^5)\n\"")
        );
        let mut timer = FlagTimer::default();
        timer.grabbed(true, 1_000);
        timer.sample([600.0, 800.0, 0.0], 20.0);
        timer.sample([0.0, 500.0, 50.0], 20.0);
        assert_eq!(
            timer.captured(b"Jan", 2, 13_345, 20.0),
            b"print \"Jan^5 has captured the ^4blue^5 flag in ^312.35^5 seconds with max of ^31000^5 ups and average ^3750^5 ups (+^315^5)\n\"".to_vec()
        );
        assert_eq!(timer, FlagTimer::default());
        assert_eq!(
            timer.captured(b"Jan", 1, 20_000, 20.0),
            b"print \"Jan^5 has captured the ^1red^5 flag (+^315^5)\n\"".to_vec()
        );
    }
}
