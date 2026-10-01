//! Master discovery and `getinfo` row construction for the server browser.

use super::{INFO_TIMEOUT, MASTER_TIMEOUT, MAX_BROWSER_SERVERS, ServerEntry};
use jkr_client::CompatProfile;
use jkr_network::{query_master, query_server_infos};
use jkr_protocol::InfoString;
use std::net::SocketAddr;

/// Ask the master for its list and every server on it for its info, handing
/// each row to `found` as it answers so the browser fills in while slow and
/// dead servers are still timing out.
pub(super) fn discover(
    master: &str,
    found: impl FnMut(ServerEntry),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut addresses = query_master(master, MASTER_TIMEOUT)?;
    addresses.truncate(MAX_BROWSER_SERVERS);
    if addresses.is_empty() {
        return Ok(());
    }
    let mut found = found;
    query_server_infos(&addresses, INFO_TIMEOUT, |address, info, round_trip| {
        let ping_millis = round_trip.as_millis().min(u128::from(u32::MAX)) as u32;
        found(entry_from_info(address, &info, ping_millis));
    })?;
    Ok(())
}

pub(crate) fn entry_from_info(
    address: SocketAddr,
    info: &InfoString,
    ping_millis: u32,
) -> ServerEntry {
    let name = info.get("hostname").unwrap_or("Unnamed server").to_owned();
    let map = info.get("mapname").unwrap_or("?").to_owned();
    let players = info
        .get("clients")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .min(u32::from(u16::MAX)) as u16;
    let capacity = info
        .get("sv_maxclients")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .min(u32::from(u16::MAX)) as u16;
    let gametype_number = info
        .get_i32("g_gametype")
        .or_else(|| info.get_i32("gametype"));
    let gametype = gametype_name(gametype_number).to_owned();
    let profile = CompatProfile::from_server_info(info);
    let password = info.get_i32("needpass").is_some_and(|value| value != 0);
    let display = format!(
        "{name:<30.30} {map:<18.18} {players:>2}/{capacity:<2} {ping_millis:>4}ms {gametype}{} {}",
        if password { " LOCKED" } else { "" },
        profile_name(&profile)
    );
    ServerEntry {
        address,
        name,
        map,
        players,
        capacity,
        ping_millis,
        gametype,
        profile,
        password,
        display,
        mode: gametype_number,
        bots: bot_count(info, players),
        valid_info: super::filters::valid_info(info),
    }
}

/// Bots among `players`, for stock's "real players" browser test.
///
/// Stock's UI reads `filterBots` (`ui_main.c:9162`), but that key exists only inside the
/// info string the client builds for its own menu (`cl_lan.cpp:307`) — no server puts it
/// on the wire. What a `getinfo` reply does carry is `g_humanplayers`
/// (`sv_main.cpp` `SVC_Info`), so derive the count from it and fall back to the `bots` key
/// `CL_SetServerInfo` reads for mods that send one.
fn bot_count(info: &InfoString, players: u16) -> i32 {
    info.get_i32("g_humanplayers")
        .map(|humans| i32::from(players).saturating_sub(humans).max(0))
        .or_else(|| info.get_i32("bots"))
        .or_else(|| info.get_i32("filterBots"))
        .unwrap_or(0)
}

fn profile_name(profile: &CompatProfile) -> &'static str {
    match profile {
        CompatProfile::BaseJka => "BASE",
        CompatProfile::JaPlus { .. } => "JA+",
        CompatProfile::TaystJk => "TAYST",
        CompatProfile::Unknown(_) => "MOD",
    }
}

pub(crate) fn gametype_name(value: Option<i32>) -> &'static str {
    match value {
        Some(0) => "FFA",
        Some(1) => "Holocron",
        Some(2) => "Jedi Master",
        Some(3) => "Duel",
        Some(4) => "Power Duel",
        Some(5) => "Single Player",
        Some(6) => "Team FFA",
        Some(7) => "Siege",
        Some(8) => "CTF",
        Some(9) => "CTY",
        _ => "Unknown",
    }
}
