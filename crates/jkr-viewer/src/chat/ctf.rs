//! Stock CTF event text is console output; it never enters player chat history.

use jkr_protocol::{EntityState, GameState};

pub(super) fn message(
    event: &EntityState,
    game: &GameState,
    strings: &crate::Localization,
) -> Option<String> {
    const REFERENCES: [&str; 5] = [
        "FRAGGED_FLAG_CARRIER",
        "FLAG_RETURNED",
        "PLAYER_RETURNED_FLAG",
        "PLAYER_CAPTURED_FLAG",
        "PLAYER_GOT_FLAG",
    ];
    let reference = *REFERENCES.get(usize::from(event.event_parameter()))?;
    let client = event.integer_field(58)?; // trickedentindex
    if !(0..32).contains(&client) {
        return None;
    }
    let info = game
        .config_string(1131 + client as usize)
        .unwrap_or_default();
    let name = jkr_client::LegacyClientInfo::new(info)
        .bytes("n")
        .unwrap_or_default();
    let text = strings.strings.get(reference).filter(|s| !s.is_empty())?;
    let team = event.integer_field(74).unwrap_or(50); // trickedentindex2
    let team = match team {
        1 => Some("RED"),
        2 => Some("BLUE"),
        3 => Some("SPECTATOR"),
        i if i < 50 => Some("FREE"),
        _ => None,
    };
    let text = team.map_or_else(|| text.clone(), |team| text.replace("%s", team));
    Some(format!("{}^7 {text}", String::from_utf8_lossy(name)))
}
