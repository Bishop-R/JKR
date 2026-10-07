//! jaPRO's centre message of the day (`csMessage`, `csTimeLeft`): a line sent as a centre
//! print once a second, from the player's once-a-second actions (`ClientTimerActions`,
//! `g_active.c:1771-1774`), for as many seconds as `g_centerMOTDTime` says.

/// One player's centre message and the seconds it has left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Countdown {
    /// `csMessage`: the text as jaPRO keeps it, `^7` before and a line break after.
    message: Vec<u8>,
    /// `csTimeLeft`.
    seconds_left: i32,
    /// Seconds passed since the last [`Self::take_due`] that owe a centre print.
    due: u32,
}

impl Countdown {
    /// Keeps `text` (when given) and shows the message for `seconds` from now
    /// (`Cmd_Showmotd_f` sets the time even with no text, and repeats the last one).
    pub fn start(&mut self, text: Option<&[u8]>, seconds: i32) {
        if let Some(text) = text {
            self.message = [b"^7".as_slice(), text, b"\n"].concat();
        }
        self.seconds_left = seconds;
    }

    /// `seconds` whole seconds of the player's time passed: one print owed for each while
    /// time is left.
    pub fn tick(&mut self, seconds: u32) {
        for _ in 0..seconds {
            if self.seconds_left == 0 {
                break;
            }
            self.seconds_left -= 1;
            self.due += 1;
        }
    }

    /// The centre prints owed since the last call, as the server command
    /// (`cp "^7<message>\n"`), and how many.
    pub fn take_due(&mut self) -> Option<(Vec<u8>, u32)> {
        let due = std::mem::take(&mut self.due);
        (due > 0).then(|| {
            (
                [b"cp \"^7".as_slice(), &self.message, b"\n\""].concat(),
                due,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Countdown;

    /// Five seconds of "Welcome", one centre print a second, then none; the command shows
    /// the last text again.
    #[test]
    fn the_message_is_printed_once_a_second_while_its_time_lasts() {
        let mut countdown = Countdown::default();
        assert_eq!(countdown.take_due(), None);
        countdown.start(Some(b"Welcome"), 5);
        countdown.tick(1);
        assert_eq!(
            countdown.take_due(),
            Some((b"cp \"^7^7Welcome\n\n\"".to_vec(), 1))
        );
        countdown.tick(3);
        assert_eq!(countdown.take_due().map(|(_, due)| due), Some(3));
        countdown.tick(4);
        assert_eq!(countdown.take_due().map(|(_, due)| due), Some(1));
        countdown.tick(1);
        assert_eq!(countdown.take_due(), None);
        countdown.start(None, 2);
        countdown.tick(1);
        assert_eq!(
            countdown.take_due(),
            Some((b"cp \"^7^7Welcome\n\n\"".to_vec(), 1))
        );
    }
}
