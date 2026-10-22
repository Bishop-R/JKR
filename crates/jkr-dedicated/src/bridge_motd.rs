//! jaPRO's message of the day (`g_consoleMOTD`, `g_centerMOTD`, `g_centerMOTDTime`): a
//! console line and a centre print shown once a session as a player first enters the
//! game (`ClientBegin`, `g_client.c:3210-3229`) and again on `ammotd`
//! (`Cmd_Showmotd_f`, `g_cmds.c:5684-5691`). jaPRO's advert for its client plugin is not
//! repeated.

use super::*;

/// The options' names.
const CONSOLE_MOTD: &[u8] = b"g_consoleMOTD";
const CENTER_MOTD: &[u8] = b"g_centerMOTD";
const CENTER_MOTD_TIME: &[u8] = b"g_centerMOTDTime";
/// `TEAM_SPECTATOR`, who is not shown the message as it begins.
const TEAM_SPECTATOR: i32 = 3;

impl NativeGame {
    /// The three options, archived with jaPRO's defaults: no message, five seconds.
    pub(super) fn register_motd(&mut self) {
        let archive = crate::cvars::CVAR_ARCHIVE;
        let options: [(&[u8], &[u8], &[u8]); 3] = [
            (CONSOLE_MOTD, b"", b"jaPRO: console message of the day"),
            (
                CENTER_MOTD,
                b"",
                b"jaPRO: centre-printed message of the day",
            ),
            (
                CENTER_MOTD_TIME,
                b"5",
                b"jaPRO: seconds the centre message shows",
            ),
        ];
        for (name, value, about) in options {
            self.cvars.get(name, value, archive, Some(about));
        }
    }

    /// `ClientBegin`'s message of the day: once a session, not to a spectator.
    pub(super) fn motd_on_begin(&mut self, client: usize) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if peer.session.team == TEAM_SPECTATOR || peer.session.saw_motd {
            return;
        }
        peer.session.saw_motd = true;
        self.show_motd(client, false);
    }

    /// The console line now and the centre line from the next second on. `asked` is the
    /// `ammotd` command, which restarts the centre line's time even with no text.
    pub(super) fn show_motd(&mut self, client: usize, asked: bool) {
        let console = self.cvars.string(CONSOLE_MOTD).to_vec();
        let center = self.cvars.string(CENTER_MOTD).to_vec();
        let seconds = self.cvars.integer(CENTER_MOTD_TIME);
        if !console.is_empty() {
            let line = [b"print \"".as_slice(), &console, b"\n\""].concat();
            self.told.push(Told::One(client, line));
        }
        if let Some(peer) = self.peer_mut(client)
            && (asked || !center.is_empty())
        {
            peer.motd
                .start((!center.is_empty()).then_some(center.as_slice()), seconds);
        }
    }

    /// The centre prints owed this frame, one for each second that passed.
    pub(super) fn send_motd(&mut self) {
        for client in 0..self.players.places() {
            let Some((line, due)) = self.peer_mut(client).and_then(|peer| peer.motd.take_due())
            else {
                continue;
            };
            for _ in 0..due {
                self.told.push(Told::One(client, line.clone()));
            }
        }
    }
}
