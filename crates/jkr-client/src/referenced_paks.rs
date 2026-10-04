//! Server-advertised content references, shared by downloading and mounting.

/// One positionally paired server pak name and checksum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferencedPak<'a> {
    /// Server virtual path without the `.pk3` extension; validate before downloading.
    pub name: &'a str,
    /// Unseeded archive checksum, not the pure-server proof checksum.
    pub checksum: i32,
}

/// Pair the common prefix of the server's whitespace-separated reference lists.
///
/// OpenJK codemp `FS_PureServerSetReferencedPaks` uses the shorter list. Some
/// servers omit names while still advertising checksums; these unnamed entries
/// are not downloadable references. Never shift a name onto a later checksum.
/// Retains JKR's bounded inventory and strict validation of paired checksums;
/// callers remain responsible for path and downloaded-content validation.
pub fn parse<'a>(checksums: &str, names: &'a str) -> Result<Vec<ReferencedPak<'a>>, &'static str> {
    let mut references = Vec::new();
    for (name, sum) in names.split_whitespace().zip(checksums.split_whitespace()) {
        if references.len() == 1024 {
            return Err("too many referenced paks (limit 1024)");
        }
        references.push(ReferencedPak {
            name,
            checksum: sum.parse().map_err(|_| "invalid referenced checksum")?,
        });
    }
    Ok(references)
}
