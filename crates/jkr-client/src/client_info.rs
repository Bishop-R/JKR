//! Borrowed CS_PLAYERS fields; malformed name bytes cannot hide model/team fields.

/// A current clientinfo view, borrowed directly from the authoritative configstring.
#[derive(Clone, Copy)]
pub struct LegacyClientInfo<'a>(&'a [u8]);

impl<'a> LegacyClientInfo<'a> {
    /// Inspect one configstring without allocating or interpreting the wire codec.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// Stock case-insensitive Info_ValueForKey, with trailing separators tolerated.
    pub fn bytes(self, key: &str) -> Option<&'a [u8]> {
        let mut fields = self
            .0
            .strip_prefix(b"\\")
            .unwrap_or(self.0)
            .split(|b| *b == b'\\');
        while let (Some(k), Some(v)) = (fields.next(), fields.next()) {
            if k.eq_ignore_ascii_case(key.as_bytes()) {
                return Some(v);
            }
        }
        None
    }

    /// Decode only the requested field, not unrelated player-name bytes.
    pub fn text(self, key: &str) -> Option<&'a str> {
        std::str::from_utf8(self.bytes(key)?).ok()
    }

    /// Integer metadata such as t, hc, skill, w/l, tt/tl and dt.
    pub fn integer(self, key: &str) -> Option<i32> {
        self.text(key)?.parse().ok()
    }
}

/// Resolve a body death pose without permitting a respawn's standing animation.
///
/// CG_BodyQueueCopy (`cg_servercmds.c:1256-1296`) falls back to BOTH_DEAD1.
/// This selects the completed pose; source-frame blending is not reproduced.
pub fn legacy_body_frame(config: &jkr_model::AnimationConfig, clip: usize) -> Option<usize> {
    if crate::animation_selection::death_animation(clip) {
        let sequence = config.get_exact(crate::legacy_animation_name(clip)?)?;
        Some(sequence.first_frame + sequence.frame_count.saturating_sub(1))
    } else {
        Some(config.get_exact("BOTH_DEAD1")?.first_frame)
    }
}
