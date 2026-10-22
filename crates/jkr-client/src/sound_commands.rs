//! Allocation-free `CG_KillLoopSounds_f` reliable-command operands.

/// Source and optional tracker of `kls`. Invalid entity numbers are ignored.
/// Keep codemp's argc checks: a missing first argument means entity zero, and
/// a single supplied argument also clears tracker zero (`atoi("")`).
pub fn loop_stop_targets(command: &[u8]) -> Option<[Option<u16>; 2]> {
    let mut remaining = command;
    if token(&mut remaining)? != b"kls" {
        return None;
    }
    let first = token(&mut remaining);
    let second = token(&mut remaining);
    let entity = |value: &[u8]| {
        let n = crate::team_info::legacy_atoi(value);
        (0..1024).contains(&n).then_some(n as u16)
    };
    Some([
        entity(first.unwrap_or_default()),
        first.and_then(|_| entity(second.unwrap_or_default())),
    ])
}

fn token<'a>(text: &mut &'a [u8]) -> Option<&'a [u8]> {
    let start = text.iter().position(|b| !b.is_ascii_whitespace())?;
    *text = &text[start..];
    let quoted = text.first() == Some(&b'"');
    if quoted {
        *text = &text[1..];
    }
    let end = text
        .iter()
        .position(|b| {
            if quoted {
                *b == b'"'
            } else {
                b.is_ascii_whitespace()
            }
        })
        .unwrap_or(text.len());
    let value = &text[..end];
    *text = &text[end + usize::from(quoted && end < text.len())..];
    Some(value)
}
