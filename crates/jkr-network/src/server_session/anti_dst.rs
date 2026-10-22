//! TaystJK's `sv_antiDST` (`sv_client.cpp:1274-1283,1458-1466`): the Dark Side Tools cheat
//! gives itself away in a userinfo model starting with `darksidetools` and in client
//! commands starting with `jkaDST_`. A remote client doing either is named on the server's
//! console and, with the setting on, kicked.

use super::LegacyGameHost;
use super::hosts::View;
use crate::{LegacyPeerAddress, LegacyReliableState};

/// Whether `prefix` begins `text`, any case (`Q_stricmpn`).
fn starts(text: &[u8], prefix: &[u8]) -> bool {
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// A userinfo `model` the cheat injects.
pub(super) fn dst_model(model: &[u8]) -> bool {
    starts(model, b"darksidetools")
}

/// A client command the cheat sends.
pub(super) fn dst_command(command: &[u8]) -> bool {
    starts(command, b"jkaDST_")
}

impl<G: LegacyGameHost> View<'_, G> {
    /// The cheat seen on `client` (its `what`: an injection or a command): said on the
    /// console, and with `sv_antiDST` the client kicked, its zombie time running from now.
    /// Whether it was kicked; nothing happens for the server's own loopback client.
    pub(super) fn dst_sighted(
        &mut self,
        client: usize,
        what: &str,
        executing: Option<(usize, &mut LegacyReliableState)>,
    ) -> bool {
        if self.slots[client].peer.address == Some(LegacyPeerAddress::Loopback) {
            return false;
        }
        let name = String::from_utf8_lossy(&self.slots[client].name).into_owned();
        let line = format!("^1Detected DST {what} from client ^7{name}\n");
        self.game.console_log(line.as_bytes());
        if !self.settings.anti_dst {
            return false;
        }
        self.drop(client, b"@@@WAS_KICKED", executing);
        self.slots[client].peer.last_packet_time = self.clock.server_time;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cheat_is_known_by_its_names() {
        assert!(dst_model(b"DarkSideTools/default"));
        assert!(!dst_model(b"kyle"));
        assert!(dst_command(b"jkadst_aimbot"));
        assert!(!dst_command(b"jkadst"));
    }
}
