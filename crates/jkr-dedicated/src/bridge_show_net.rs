//! jaPRO's `showNet` (`Cmd_ShowNet_f`, no gates): each playing client's network settings —
//! its rate and snaps (from its userinfo, against `sv_maxRate` and `sv_fps`), the packets,
//! time nudge and frame cap a jaPRO client reports in `cg_displayNetSettings`, and the
//! frame rate its moves arrive at — in yellow where they hurt it, `?` where unknown.

use super::*;

/// `TEAM_SPECTATOR`.
const TEAM_SPECTATOR: i32 = 3;
/// `Q3_INFINITE`: a time nudge never reported.
const UNREPORTED: i32 = 0x7FFF_FFFF;
/// `msg`'s room in `Cmd_ShowNet_f` (`1024 - 128`).
const MESSAGE_ROOM: usize = 896;

/// What `ClientUserinfoChanged` keeps of a client's net settings (`pers.rate`,
/// `pers.snaps`, `pers.maxPackets`, `pers.timenudge`, `pers.maxFPS`).
struct NetSettings {
    rate: i32,
    snaps: i32,
    max_packets: i32,
    time_nudge: i32,
    max_fps: i32,
}

impl NetSettings {
    fn read(userinfo: &[u8]) -> Self {
        let number = |key: &[u8]| {
            jkr_game_jka::userinfo::atoi(
                jkr_protocol::info_value(userinfo, key).unwrap_or_default(),
            )
        };
        let reported = jkr_protocol::info_value(userinfo, b"cg_displayNetSettings")
            .filter(|value| !value.is_empty());
        let mut parts = reported.iter().flat_map(|value| {
            value
                .split(|&byte| byte == b' ')
                .filter(|part| !part.is_empty())
                .map(jkr_game_jka::userinfo::atoi)
        });
        let (max_packets, time_nudge, max_fps) = if reported.is_some() {
            (
                parts.next().unwrap_or(0),
                parts.next().unwrap_or(0),
                parts.next().unwrap_or(0),
            )
        } else {
            (0, UNREPORTED, 0)
        };
        Self {
            rate: number(b"rate").clamp(0, 90_000),
            snaps: number(b"snaps").clamp(0, 1_000),
            max_packets,
            time_nudge,
            max_fps,
        }
    }
}

impl NativeGame {
    /// `showNet` from `client`: whether `command` was it. Alone, every playing client;
    /// `follow`, the one a spectator follows; a name or number, that client — everyone
    /// where `follow` follows nobody or the name finds nobody, as in jaPRO.
    pub(super) fn show_net_command(&mut self, client: usize, text: &[u8]) -> bool {
        let words: Vec<&[u8]> = jkr_network::LegacyTokens::new(text).collect();
        if !words
            .first()
            .is_some_and(|word| word.eq_ignore_ascii_case(b"shownet"))
        {
            return false;
        }
        let only = match words[..] {
            [_] => None,
            [_, word] if word.eq_ignore_ascii_case(b"follow") => {
                let Some(peer) = self.peer(client) else {
                    return true;
                };
                let following =
                    peer.session.spectator_state == jkr_game_jka::client_begin::SPECTATOR_FOLLOW;
                following.then(|| peer.session.spectator_client.max(0) as usize)
            }
            [_, name] => self.admin_target(client, name),
            _ => {
                self.told.push(Told::One(
                    client,
                    b"print \"Usage: showNet <follow (optional)>\n\"".to_vec(),
                ));
                return true;
            }
        };
        self.told.push(Told::One(
            client,
            b"print \"^5   Rate    Snaps     Maxpackets  Timenudge   MaxFPS   FPS   Name\n\""
                .to_vec(),
        ));
        let (max_rate, fps) = (
            self.cvars.integer(b"sv_maxRate"),
            self.cvars.integer(b"sv_fps"),
        );
        let mut message = String::new();
        for other in 0..self.players.places() {
            let Some(peer) = self.peer(other) else {
                continue;
            };
            if peer.session.team == TEAM_SPECTATOR
                || only.is_some_and(|only| only != other)
                || peer.name.is_empty()
            {
                continue;
            }
            let row = Self::net_row(other, peer, max_rate, fps);
            if message.len() + row.len() >= MESSAGE_ROOM {
                self.told.push(Told::One(
                    client,
                    format!("print \"{}\"", std::mem::take(&mut message)).into_bytes(),
                ));
            }
            message += &row;
        }
        self.told.push(Told::One(
            client,
            format!("print \"{message}\"").into_bytes(),
        ));
        true
    }

    /// One client's row: number, rate, snaps, packets, nudge, frame cap, frame rate, name.
    fn net_row(number: usize, peer: &Peer, max_rate: i32, fps: i32) -> String {
        let name = String::from_utf8_lossy(&peer.name);
        let cells: [String; 6] = if peer.session.bot_force.is_some() {
            std::array::from_fn(|_| String::from("^7Bot"))
        } else {
            let net = NetSettings::read(&peer.userinfo);
            let marked = |bad: bool, value: i32| format!("^{}{value}", if bad { 3 } else { 7 });
            let unknown = || String::from("^3?");
            [
                if net.rate < max_rate || max_rate == 0 {
                    marked(true, net.rate)
                } else {
                    marked(false, max_rate)
                },
                if net.snaps < fps || fps == 0 {
                    marked(true, net.snaps)
                } else {
                    marked(false, fps)
                },
                if net.max_packets == 0 {
                    unknown()
                } else {
                    marked(net.max_packets < 30, net.max_packets)
                },
                if net.time_nudge == UNREPORTED {
                    unknown()
                } else {
                    marked(!(-150..=0).contains(&net.time_nudge), net.time_nudge)
                },
                if net.max_fps == 0 {
                    unknown()
                } else {
                    marked(net.max_fps > 333 || net.max_fps < 60, net.max_fps)
                },
                if peer.command_msec != 0 {
                    marked(false, 1_000 / peer.command_msec)
                } else {
                    unknown()
                },
            ]
        };
        let [rate, snaps, packets, nudge, max_fps, real_fps] = cells;
        format!(
            "{:<2}{rate:<10}{snaps:<12}{packets:<14}{nudge:<14}{max_fps:<11}{real_fps:<8}^7{name}\n",
            format!("^5{number:>2}^3:")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_settings_are_read_from_the_userinfo() {
        let net = NetSettings::read(b"\\rate\\95000\\snaps\\40\\cg_displayNetSettings\\30 -20 125");
        assert_eq!(
            (
                net.rate,
                net.snaps,
                net.max_packets,
                net.time_nudge,
                net.max_fps
            ),
            (90_000, 40, 30, -20, 125)
        );
        let stock = NetSettings::read(b"\\rate\\25000\\snaps\\20");
        assert_eq!(
            (stock.max_packets, stock.time_nudge, stock.max_fps),
            (0, UNREPORTED, 0)
        );
    }
}
