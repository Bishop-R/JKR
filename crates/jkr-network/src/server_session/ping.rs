//! `SV_CalcPings` (`sv_main.cpp:851`): a client's ping is the mean time between the
//! server sending one of its last 32 messages and the client acknowledging it.
use super::{LegacyGameHost, LegacyServerSession};
use crate::{LegacyClientPhase, LegacyPeerAddress};

/// `PACKET_BACKUP`: the messages a client's ping is measured over.
pub(super) const MESSAGE_BACKUP: usize = 32;

/// When one message went out (`messageSent`) and when the client first said it had it
/// (`messageAcked`; -1 until then), in server time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MessageTiming {
    pub sent: i32,
    pub acked: i32,
}

/// Who stands behind a client as `SV_CalcPings` asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stand {
    /// No entity (`gentity` null).
    Nobody,
    Bot,
    Player,
}

/// One client's ping, and whether the game is told it (`ps->ping`): 999 for a client
/// not in the world or without an entity, 0 for a bot, otherwise the mean round trip of
/// the acknowledged messages (a time of zero or less counts as none), truncated, at
/// most 999 — 999 again when none was acknowledged. Under TaystJK's `sv_pingFix`
/// (`ping_fix`) a human's is at least 1, as bot filters take 0 for a bot.
pub(super) fn calc_ping(
    phase: LegacyClientPhase,
    stand: Stand,
    timings: &[MessageTiming; MESSAGE_BACKUP],
    ping_fix: bool,
) -> (i32, bool) {
    if phase != LegacyClientPhase::Active || stand == Stand::Nobody {
        return (999, false);
    }
    if stand == Stand::Bot {
        return (0, false);
    }
    let (total, count) = timings.iter().filter(|timing| timing.acked > 0).fold(
        (0_i32, 0_i32),
        |(total, count), timing| {
            (
                total.wrapping_add(timing.acked.wrapping_sub(timing.sent)),
                count + 1,
            )
        },
    );
    if count == 0 {
        (999, true)
    } else if ping_fix {
        ((total / count).clamp(1, 999), true)
    } else {
        ((total / count).min(999), true)
    }
}

/// `SV_SendMessageToClient`: message `sequence` goes out at `now`, not acknowledged yet.
pub(super) fn record_sent(timings: &mut [MessageTiming; MESSAGE_BACKUP], sequence: i32, now: i32) {
    timings[(sequence & (MESSAGE_BACKUP as i32 - 1)) as usize] = MessageTiming {
        sent: now,
        acked: -1,
    };
}

impl<G: LegacyGameHost> LegacyServerSession<G> {
    /// `SV_CalcPings`: measure every client's ping and tell the game each one it keeps
    /// ([`LegacyGameHost::client_ping`]). The reference runs it at the top of every
    /// server frame, before the game's; call it before the game's frame.
    pub fn calc_pings(&mut self) {
        for (client, slot) in self.slots.iter_mut().enumerate() {
            let stand = if slot.peer.address == Some(LegacyPeerAddress::Bot) {
                Stand::Bot
            } else {
                Stand::Player
            };
            let (ping, told) = calc_ping(slot.phase, stand, &slot.timings, self.settings.ping_fix);
            slot.ping = ping;
            if told {
                self.game.client_ping(client, ping);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A round trip under a millisecond is 0 as the reference measures it, and 1 under
    /// TaystJK's `sv_pingFix`, as bot filters read 0 as a bot.
    #[test]
    fn ping_fix_keeps_a_human_above_zero() {
        let mut timings = [MessageTiming::default(); MESSAGE_BACKUP];
        record_sent(&mut timings, 1, 500);
        timings[1].acked = 500;
        let active = LegacyClientPhase::Active;
        assert_eq!(calc_ping(active, Stand::Player, &timings, false), (0, true));
        assert_eq!(calc_ping(active, Stand::Player, &timings, true), (1, true));
        assert_eq!(calc_ping(active, Stand::Bot, &timings, true), (0, false));
    }
}
