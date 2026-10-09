//! A player named on the console, as EternalJK's commands find one
//! (`CG_ClientNumberFromString`, `CG_SanitizeString2`, `cg_consolecmds.c:328-433`): `-`
//! for the player under the crosshair, a slot number from 0 to 31, or else the one player
//! whose name, cleaned, contains the cleaned text.

/// `MAX_NAME_LENGTH`: where the UI cuts a name, and the cleaning with it.
const MAX_NAME_LENGTH: usize = 32;

/// `CG_SanitizeString2`: colour codes (`^` and a digit) and a lone `^` gone, control
/// characters gone, lower case, no more of the name than the UI shows.
pub fn sanitize_name(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len());
    let mut index = 0;
    while index < name.len() && index < MAX_NAME_LENGTH - 1 {
        let byte = name[index];
        if byte == b'^' {
            index += if name.get(index + 1).is_some_and(u8::is_ascii_digit) {
                2
            } else {
                1
            };
            continue;
        }
        if byte >= 32 {
            out.push(byte.to_ascii_lowercase());
        }
        index += 1;
    }
    out
}

/// The slot `text` names among `names` (each slot's name, `None` where nobody is), with
/// `crosshair` the player under the crosshair. `Err` carries what EternalJK prints, if
/// anything.
pub fn client_number_from_string(
    text: &str,
    names: &[Option<&[u8]>],
    crosshair: Option<usize>,
) -> Result<usize, Option<String>> {
    let bytes = text.as_bytes();
    if bytes.first() == Some(&b'-') {
        return crosshair.ok_or(None);
    }
    let slot = match bytes {
        [digit] if digit.is_ascii_digit() => Some(usize::from(digit - b'0')),
        [b'1' | b'2', digit] if digit.is_ascii_digit() => text.parse().ok(),
        [b'3', b'0' | b'1'] => text.parse().ok(),
        _ => None,
    };
    if let Some(slot) = slot {
        return match names.get(slot) {
            Some(Some(_)) => Ok(slot),
            _ => Err(Some(format!("Client '{slot}' is not active"))),
        };
    }
    let wanted = sanitize_name(bytes);
    let mut matches = names.iter().enumerate().filter_map(|(slot, name)| {
        let name = sanitize_name((*name)?);
        // `strstr`: an empty text is found in every name.
        (wanted.is_empty()
            || name
                .windows(wanted.len())
                .any(|window| window == wanted.as_slice()))
        .then_some(slot)
    });
    match (matches.next(), matches.next()) {
        (Some(slot), None) => Ok(slot),
        (Some(_), Some(_)) => Err(Some(format!("More than one user '{text}' on the server"))),
        // `atoi` of the text is not 0: a number nobody has, which goes unsaid.
        (None, _) if leading_number(text) != 0 => Err(None),
        (None, _) => Err(Some(format!("User '{text}' is not on the server"))),
    }
}

/// `atoi`: the leading number of `text`, 0 without one.
fn leading_number(text: &str) -> i64 {
    let text = text.trim_start();
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end]
        .parse::<i64>()
        .map_or(0, |number| sign * number)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Colours, case and control characters do not count; a slot is a slot.
    #[test]
    fn players_are_found_by_slot_or_by_part_of_the_name() {
        let names: [Option<&[u8]>; 4] = [Some(b"^1Ka^7ne"), None, Some(b"Kyle"), Some(b"Jan")];
        assert_eq!(sanitize_name(b"^1Ka^7ne^x\x01"), b"kanex");
        assert_eq!(client_number_from_string("ane", &names, None), Ok(0));
        assert_eq!(client_number_from_string("2", &names, None), Ok(2));
        assert_eq!(
            client_number_from_string("1", &names, None),
            Err(Some("Client '1' is not active".to_owned()))
        );
        assert_eq!(
            client_number_from_string("k", &names, None),
            Err(Some("More than one user 'k' on the server".to_owned()))
        );
        assert_eq!(
            client_number_from_string("luke", &names, None),
            Err(Some("User 'luke' is not on the server".to_owned()))
        );
        assert_eq!(client_number_from_string("-", &names, Some(3)), Ok(3));
        assert_eq!(client_number_from_string("45x", &names, None), Err(None));
    }
}
