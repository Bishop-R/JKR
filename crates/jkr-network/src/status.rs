//! The connectionless `getstatus` query: the server's full cvar set plus one
//! `score ping "name"` line per connected client.

use crate::{NetworkError, OOB_PREFIX, connectionless_packet};
use jkr_protocol::InfoString;
use std::net::SocketAddr;
use std::time::Duration;

/// One connected client as reported by `statusResponse`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusPlayer {
    pub score: i32,
    pub ping: i32,
    /// Name as sent, colour codes included.
    pub name: String,
}

/// A parsed `statusResponse`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerStatus {
    /// Every server cvar the server publishes (`sv_hostname`, `version`,
    /// `gamename`, limits, mod flags, …).
    pub info: InfoString,
    pub players: Vec<StatusPlayer>,
}

/// Ask `server` for its status and wait at most `timeout` for the reply.
pub fn query_server_status(
    server: SocketAddr,
    timeout: Duration,
) -> Result<ServerStatus, NetworkError> {
    let packet =
        crate::query::first_response(server, &connectionless_packet("getstatus jkr")?, timeout)?;
    parse_status_response(&packet)
}

/// Parse a raw `statusResponse` packet (OOB prefix included).
pub fn parse_status_response(packet: &[u8]) -> Result<ServerStatus, NetworkError> {
    let payload = packet
        .strip_prefix(&OOB_PREFIX)
        .ok_or(NetworkError::MissingConnectionlessPrefix)?;
    let body = payload
        .strip_prefix(b"statusResponse\n")
        .or_else(|| payload.strip_prefix(b"statusResponse\r\n"))
        .ok_or(NetworkError::UnexpectedResponse)?;
    let body = body.split(|byte| *byte == 0).next().unwrap_or_default();
    // Player names are raw bytes in the server's charset; keep what decodes.
    let text = String::from_utf8_lossy(body);
    let mut lines = text.lines();
    let info = InfoString::parse(lines.next().unwrap_or_default())?;
    let players = lines.filter_map(parse_status_player).collect();
    Ok(ServerStatus { info, players })
}

fn parse_status_player(line: &str) -> Option<StatusPlayer> {
    let mut fields = line.trim_end_matches('\r').splitn(3, ' ');
    Some(StatusPlayer {
        score: fields.next()?.parse().ok()?,
        ping: fields.next()?.parse().ok()?,
        name: fields.next()?.trim_matches('"').to_owned(),
    })
}
