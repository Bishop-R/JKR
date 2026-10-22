//! A pure server's check of its clients' pk3 files (`SV_VerifyPaks_f`,
//! `codemp/server/sv_client.cpp:925-1042`).
//!
//! After every gamestate a client on a pure server answers with
//! `cp <cgame> <ui> @ <referenced>... <encoded>`: the pure checksums of the pk3 its cgame
//! and its ui came from, then those of the pk3 files it referenced, then those checksums
//! folded with the gamestate's checksum feed and their count. The server accepts it when
//! the cgame and the ui came from the pk3 files it would load them from itself, every
//! referenced pk3 is one it loaded, none is named twice, and the fold matches.

/// What a pure server knows of its own pk3 files, as it checks a client against them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegacyPureManifest {
    /// The gamestate's checksum feed (`sv.checksumFeed`).
    pub checksum_feed: i32,
    /// Every loaded pk3's pure checksum, the feed folded in (`FS_LoadedPakPureChecksums`).
    pub loaded: Vec<i32>,
    /// The pure checksum of the pk3 the server would load `cgamex86.dll` from
    /// (`FS_FileIsInPAK`); `None` when no pk3 holds it, and then no client is pure.
    pub cgame: Option<i32>,
    /// The same for `uix86.dll`.
    pub ui: Option<i32>,
}

/// The pk3 files a stock server checks at most (`nClientChkSum[1024]`,
/// `nServerChkSum[1024]`).
const MAX_CHECKSUMS: usize = 1_024;

impl LegacyPureManifest {
    /// `SV_VerifyPaks_f`'s verdict on a `cp` command, `arguments[0]` being `cp` itself, as
    /// `Cmd_TokenizeString` splits it.
    pub fn verify(&self, arguments: &[&[u8]]) -> bool {
        let (Some(cgame), Some(ui)) = (self.cgame, self.ui) else {
            return false;
        };
        // "must be at least 6: cl_paks cgame ui @ firstref ... numChecksums"
        if arguments.len() < 6 {
            return false;
        }
        let argument = |index: usize| arguments.get(index).copied().unwrap_or(b"");
        let is_delimiter = |value: &[u8]| value.first() == Some(&b'@');
        if is_delimiter(argument(1)) || c_atoi(argument(1)) != cgame {
            return false;
        }
        if is_delimiter(argument(2)) || c_atoi(argument(2)) != ui {
            return false;
        }
        if !is_delimiter(argument(3)) {
            return false;
        }
        let sent = &arguments[4..arguments.len().min(4 + MAX_CHECKSUMS)];
        let Some((encoded, referenced)) = sent.split_last() else {
            return false;
        };
        let referenced: Vec<i32> = referenced.iter().map(|value| c_atoi(value)).collect();
        // "make sure none of the client check sums are the same"
        if referenced
            .iter()
            .enumerate()
            .any(|(index, checksum)| referenced[index + 1..].contains(checksum))
        {
            return false;
        }
        // "check if the client has provided any pure checksums of pk3 files not loaded by
        // the server"
        let loaded = &self.loaded[..self.loaded.len().min(MAX_CHECKSUMS)];
        if referenced.iter().any(|checksum| !loaded.contains(checksum)) {
            return false;
        }
        // "check if the number of checksums was correct"
        let folded = referenced
            .iter()
            .fold(self.checksum_feed, |fold, checksum| fold ^ checksum)
            ^ referenced.len() as i32;
        folded == c_atoi(encoded)
    }
}

/// `atoi`: leading white space, a sign and the digits that follow, wrapped to an `int` as
/// `strtol`'s `long` is narrowed; zero for none.
fn c_atoi(value: &[u8]) -> i32 {
    let mut digits = value
        .iter()
        .skip_while(|byte| byte.is_ascii_whitespace())
        .peekable();
    let negative = match digits.peek() {
        Some(b'-') => {
            digits.next();
            true
        }
        Some(b'+') => {
            digits.next();
            false
        }
        _ => false,
    };
    let magnitude = digits
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0_i64, |total, digit| {
            total
                .saturating_mul(10)
                .saturating_add(i64::from(digit - b'0'))
        });
    (if negative { -magnitude } else { magnitude }) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three loaded pk3 files, the cgame and the ui both in the last.
    fn manifest() -> LegacyPureManifest {
        LegacyPureManifest {
            checksum_feed: 0x1234_5678,
            loaded: vec![175_791_103, 853_496_622, 1_199_741_723],
            cgame: Some(1_199_741_723),
            ui: Some(1_199_741_723),
        }
    }

    /// A stock client's `cp` for `referenced` (`FS_ReferencedPakPureChecksums`).
    fn command(cgame: i32, ui: i32, referenced: &[i32], feed: i32) -> String {
        let folded =
            referenced.iter().fold(feed, |fold, value| fold ^ value) ^ referenced.len() as i32;
        let mut text = format!("cp {cgame} {ui} @ ");
        for value in referenced {
            text.push_str(&format!("{value} "));
        }
        text.push_str(&format!("{folded} "));
        text
    }

    fn verify(manifest: &LegacyPureManifest, text: &str) -> bool {
        let arguments: Vec<&[u8]> = text.split_whitespace().map(str::as_bytes).collect();
        manifest.verify(&arguments)
    }

    #[test]
    fn a_stock_client_on_the_same_pk3_files_is_pure() {
        let manifest = manifest();
        let text = command(1_199_741_723, 1_199_741_723, &[175_791_103], 0x1234_5678);
        assert!(verify(&manifest, &text));
        let both = command(
            1_199_741_723,
            1_199_741_723,
            &[175_791_103, 853_496_622],
            0x1234_5678,
        );
        assert!(verify(&manifest, &both));
    }

    #[test]
    fn a_client_off_the_server_files_is_not() {
        let manifest = manifest();
        let feed = 0x1234_5678;
        // Another cgame, another ui, a pk3 the server lacks, one named twice, a stale feed.
        for text in [
            command(853_496_622, 1_199_741_723, &[175_791_103], feed),
            command(1_199_741_723, 175_791_103, &[175_791_103], feed),
            command(1_199_741_723, 1_199_741_723, &[42], feed),
            command(
                1_199_741_723,
                1_199_741_723,
                &[175_791_103, 175_791_103],
                feed,
            ),
            command(1_199_741_723, 1_199_741_723, &[175_791_103], feed ^ 1),
        ] {
            assert!(!verify(&manifest, &text), "{text}");
        }
        // Too short, or without the delimiter.
        assert!(!verify(&manifest, "cp 1199741723 1199741723 @ 5"));
        assert!(!verify(&manifest, "cp 1199741723 1199741723 x 175791103 1"));
        // A server none of whose pk3 files holds the cgame accepts nobody.
        let without = LegacyPureManifest {
            cgame: None,
            ..manifest.clone()
        };
        let text = command(1_199_741_723, 1_199_741_723, &[175_791_103], feed);
        assert!(!verify(&without, &text));
    }

    #[test]
    fn checksums_read_as_atoi_reads_them() {
        assert_eq!(c_atoi(b"-1342311474"), -1_342_311_474);
        assert_eq!(c_atoi(b"  +17abc"), 17);
        assert_eq!(c_atoi(b"@"), 0);
        assert_eq!(c_atoi(b""), 0);
    }
}
