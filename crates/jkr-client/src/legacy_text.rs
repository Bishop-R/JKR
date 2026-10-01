//! Decoding for player-authored text that predates Unicode.
//!
//! JKA is a pre-Unicode engine: names, chat and client info are byte strings rendered through
//! a codepage font, so a byte above 0x7F means the Latin-1 character of that value, not part
//! of a UTF-8 sequence. Treating those bytes as UTF-8 and replacing what does not decode
//! turns a name like `Marañ` into `Maraï¿½` on screen, which is what the owner saw in the
//! kill announcement; validating strictly instead loses the name entirely, which is what the
//! crosshair used to do.

use std::borrow::Cow;

/// Decode player-authored bytes for display.
///
/// Valid UTF-8 is returned borrowed and unchanged, which covers ASCII and any client that
/// genuinely sends UTF-8. Anything else is decoded as Latin-1, where each byte is the
/// character of the same value — the meaning JKA's own font gives it. Pure ASCII is identical
/// under both readings, so the common case never allocates and never changes.
pub fn decode_legacy(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => Cow::Owned(bytes.iter().map(|&byte| char::from(byte)).collect()),
    }
}
