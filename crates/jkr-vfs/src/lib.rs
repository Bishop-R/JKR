//! Host-independent virtual filesystem for loose JKA assets and PK3 archives.

mod cache_identity;
mod path;
mod pk3_directory;
mod pk3_fingerprint;
pub use cache_identity::AssetCacheIdentity;

pub use path::{VirtualPath, VirtualPathError};

use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use md4::{Digest, Md4};

const DEFAULT_MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MountId(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetSource {
    pub mount_id: MountId,
    pub mount_name: Arc<str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Asset {
    pub bytes: Vec<u8>,
    pub source: AssetSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountSummary {
    pub id: MountId,
    pub name: Arc<str>,
    pub entries: usize,
}

/// CRC catalogue used by the id Tech 3 pure-server proof.
///
/// The ordinary checksum is feed-independent. The pure checksum prepends the
/// server's per-gamestate feed before hashing the same ZIP entry CRC sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pk3Fingerprint {
    entry_crcs: Vec<u32>,
}

impl Pk3Fingerprint {
    /// Read ordered, nonempty ZIP-entry CRCs from the validated central directory.
    pub fn open(archive_path: impl AsRef<Path>) -> Result<Self, VfsError> {
        let archive_path = archive_path.as_ref();
        let file = File::open(archive_path).map_err(|source| VfsError::Io {
            operation: "open PK3 for checksum",
            path: archive_path.to_owned(),
            source,
        })?;
        let archive = zip::ZipArchive::new(file).map_err(|source| VfsError::Zip {
            path: archive_path.to_owned(),
            source,
        })?;
        let entry_crcs = pk3_fingerprint::catalog(archive).map_err(|source| VfsError::Io {
            operation: "read PK3 checksum catalogue",
            path: archive_path.to_owned(),
            source,
        })?;
        Ok(Self { entry_crcs })
    }

    pub fn checksum(&self) -> i32 {
        block_checksum(self.entry_crcs.iter().copied())
    }

    pub fn pure_checksum(&self, checksum_feed: i32) -> i32 {
        block_checksum(std::iter::once(checksum_feed as u32).chain(self.entry_crcs.iter().copied()))
    }

    pub fn entry_count(&self) -> usize {
        self.entry_crcs.len()
    }
}

/// `Com_BlockChecksum` over a file's bytes (`qcommon/md4.cpp`): the four words of its MD4
/// digest XORed together — a map's `sv_mapChecksum` (`CM_LoadMap`) for its `.bsp`.
pub fn file_checksum(bytes: &[u8]) -> i32 {
    let digest = Md4::digest(bytes);
    digest
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("MD4 word is four bytes")))
        .fold(0_u32, std::ops::BitXor::bitxor) as i32
}

fn block_checksum(values: impl IntoIterator<Item = u32>) -> i32 {
    let mut hasher = Md4::new();
    for value in values {
        hasher.update(value.to_le_bytes());
    }
    let digest = hasher.finalize();
    digest
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("MD4 word is four bytes")))
        .fold(0_u32, std::ops::BitXor::bitxor) as i32
}

/// Ordered asset mounts. The most recently mounted source has highest priority.
#[derive(Clone)]
pub struct VirtualFileSystem {
    mounts: Vec<MountedSource>,
    next_mount_id: u64,
    max_asset_bytes: u64,
    read_diagnostics: bool,
}

impl Default for VirtualFileSystem {
    fn default() -> Self {
        Self {
            mounts: Vec::new(),
            next_mount_id: 0,
            max_asset_bytes: DEFAULT_MAX_ASSET_BYTES,
            read_diagnostics: false,
        }
    }
}

impl VirtualFileSystem {
    /// Enable source-resolution diagnostics on asset reads, disabled by default.
    pub fn set_read_diagnostics(&mut self, enabled: bool) {
        self.read_diagnostics = enabled;
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_asset_bytes(max_asset_bytes: u64) -> Self {
        Self {
            max_asset_bytes,
            ..Self::default()
        }
    }

    pub fn mount_directory(&mut self, root: impl AsRef<Path>) -> Result<MountId, VfsError> {
        let source = DirectorySource::scan(root.as_ref())?;
        self.add_mount(source.name.clone(), Arc::new(source))
    }

    pub fn mount_pk3(&mut self, archive: impl AsRef<Path>) -> Result<MountId, VfsError> {
        let source = Pk3Source::open(archive.as_ref())?;
        self.add_mount(source.name.clone(), Arc::new(source))
    }

    /// Mounts an in-memory collection using the same normalized path and
    /// precedence rules as directory and PK3 mounts.
    ///
    /// This is useful to embedders and to deterministic tests which should not
    /// need host temporary files merely to exercise VFS-backed adapters.
    pub fn mount_memory<I, P, B>(
        &mut self,
        name: impl Into<Arc<str>>,
        entries: I,
    ) -> Result<MountId, VfsError>
    where
        I: IntoIterator<Item = (P, B)>,
        P: AsRef<str>,
        B: Into<Vec<u8>>,
    {
        let source = MemorySource::new(name.into(), entries)?;
        self.add_mount(source.name.clone(), Arc::new(source))
    }

    pub fn read(&self, path: &str) -> Result<Option<Asset>, VfsError> {
        let path = VirtualPath::new(path)?;

        for mount in self.mounts.iter().rev() {
            if let Some(bytes) = mount.source.read(&path, self.max_asset_bytes)? {
                if self.read_diagnostics {
                    eprintln!(
                        "asset read {path}: {} bytes from {}",
                        bytes.len(),
                        mount.name
                    );
                }
                return Ok(Some(Asset {
                    bytes,
                    source: AssetSource {
                        mount_id: mount.id,
                        mount_name: Arc::clone(&mount.name),
                    },
                }));
            }
        }

        if self.read_diagnostics {
            eprintln!("asset read {path}: not found");
        }
        Ok(None)
    }

    pub fn contains(&self, path: &str) -> Result<bool, VfsError> {
        let path = VirtualPath::new(path)?;
        Ok(self
            .mounts
            .iter()
            .rev()
            .any(|mount| mount.source.contains(&path)))
    }

    /// Reads from one explicitly selected mount, without changing normal search precedence.
    /// Missing mounts or paths return `None`; normal asset size limits still apply.
    pub fn read_from_mount(&self, id: MountId, path: &str) -> Result<Option<Asset>, VfsError> {
        let path = VirtualPath::new(path)?;
        let Some(mount) = self.mounts.iter().find(|mount| mount.id == id) else {
            return Ok(None);
        };
        Ok(mount
            .source
            .read(&path, self.max_asset_bytes)?
            .map(|bytes| Asset {
                bytes,
                source: AssetSource {
                    mount_id: mount.id,
                    mount_name: Arc::clone(&mount.name),
                },
            }))
    }

    pub fn mounts(&self) -> impl DoubleEndedIterator<Item = MountSummary> + '_ {
        self.mounts.iter().map(|mount| MountSummary {
            id: mount.id,
            name: Arc::clone(&mount.name),
            entries: mount.source.entry_count(),
        })
    }

    /// `FS_ListFilteredFiles` for a directory: the names, relative to `directory`, of
    /// the files in it (or one directory below) ending in `extension`. The
    /// highest-priority source comes first, each source in its own order (an archive
    /// in its central directory's), and a name only the first time it is met — the
    /// order a game reading "every file of a kind" joins them in.
    pub fn list_files(&self, directory: &str, extension: &str) -> Vec<String> {
        let prefix = format!("{}/", directory.trim_end_matches('/').to_ascii_lowercase());
        let extension = extension.to_ascii_lowercase();
        let mut listed: Vec<String> = Vec::new();
        let mut paths = Vec::new();
        for mount in self.mounts.iter().rev() {
            paths.clear();
            mount.source.append_in_order(&mut paths);
            for path in &paths {
                let Some(name) = path.as_str().strip_prefix(&prefix) else {
                    continue;
                };
                if name.matches('/').count() > 1
                    || !name.ends_with(&extension)
                    || listed.iter().any(|known| known == name)
                {
                    continue;
                }
                listed.push(name.to_owned());
            }
        }
        listed
    }

    /// Returns the normalized union of paths visible across all mounts.
    pub fn paths(&self) -> Vec<VirtualPath> {
        let mut paths = BTreeSet::new();
        for mount in &self.mounts {
            mount.source.append_paths(&mut paths);
        }
        paths.into_iter().collect()
    }

    fn add_mount(
        &mut self,
        name: Arc<str>,
        source: Arc<dyn AssetSourceBackend>,
    ) -> Result<MountId, VfsError> {
        let id = MountId(self.next_mount_id);
        self.next_mount_id = self
            .next_mount_id
            .checked_add(1)
            .ok_or(VfsError::TooManyMounts)?;
        self.mounts.push(MountedSource {
            id,
            name,
            source,
            cache_identity: cache_identity::next(),
        });
        Ok(id)
    }
}

#[derive(Clone)]
struct MountedSource {
    id: MountId,
    name: Arc<str>,
    source: Arc<dyn AssetSourceBackend>,
    cache_identity: u64,
}

trait AssetSourceBackend: Send + Sync {
    fn cacheable(&self) -> bool {
        false
    }
    fn read(&self, path: &VirtualPath, max_bytes: u64) -> Result<Option<Vec<u8>>, VfsError>;
    fn contains(&self, path: &VirtualPath) -> bool;
    fn entry_count(&self) -> usize;
    fn append_paths(&self, output: &mut BTreeSet<VirtualPath>);
    /// Every path in the source's own order: an archive's central directory, else sorted.
    fn append_in_order(&self, output: &mut Vec<VirtualPath>) {
        let mut paths = BTreeSet::new();
        self.append_paths(&mut paths);
        output.extend(paths);
    }
}

struct MemorySource {
    name: Arc<str>,
    files: HashMap<VirtualPath, Vec<u8>>,
}

impl MemorySource {
    fn new<I, P, B>(name: Arc<str>, entries: I) -> Result<Self, VfsError>
    where
        I: IntoIterator<Item = (P, B)>,
        P: AsRef<str>,
        B: Into<Vec<u8>>,
    {
        let mut files = HashMap::new();
        for (path, bytes) in entries {
            let path = VirtualPath::new(path.as_ref())?;
            files.insert(path, bytes.into());
        }
        Ok(Self { name, files })
    }
}

impl AssetSourceBackend for MemorySource {
    fn cacheable(&self) -> bool {
        true
    }
    fn read(&self, path: &VirtualPath, max_bytes: u64) -> Result<Option<Vec<u8>>, VfsError> {
        let Some(bytes) = self.files.get(path) else {
            return Ok(None);
        };
        ensure_size(path, bytes.len() as u64, max_bytes)?;
        Ok(Some(bytes.clone()))
    }

    fn contains(&self, path: &VirtualPath) -> bool {
        self.files.contains_key(path)
    }

    fn entry_count(&self) -> usize {
        self.files.len()
    }

    fn append_paths(&self, output: &mut BTreeSet<VirtualPath>) {
        output.extend(self.files.keys().cloned());
    }
}

struct DirectorySource {
    name: Arc<str>,
    files: HashMap<VirtualPath, PathBuf>,
}

impl DirectorySource {
    fn scan(root: &Path) -> Result<Self, VfsError> {
        let root = root.canonicalize().map_err(|source| VfsError::Io {
            operation: "canonicalize directory mount",
            path: root.to_owned(),
            source,
        })?;
        if !root.is_dir() {
            return Err(VfsError::NotDirectory(root));
        }

        let mut files = HashMap::new();
        let mut pending = vec![root.clone()];
        while let Some(directory) = pending.pop() {
            let entries = fs::read_dir(&directory).map_err(|source| VfsError::Io {
                operation: "scan directory mount",
                path: directory.clone(),
                source,
            })?;

            for entry in entries {
                let entry = entry.map_err(|source| VfsError::Io {
                    operation: "read directory entry",
                    path: directory.clone(),
                    source,
                })?;
                let file_type = entry.file_type().map_err(|source| VfsError::Io {
                    operation: "inspect directory entry",
                    path: entry.path(),
                    source,
                })?;

                if file_type.is_dir() {
                    pending.push(entry.path());
                    continue;
                }

                if !file_type.is_file() {
                    // Symlinks and device files are deliberately not exposed.
                    continue;
                }

                let entry_path = entry.path();
                let relative =
                    entry_path
                        .strip_prefix(&root)
                        .map_err(|_| VfsError::EscapedDirectoryRoot {
                            root: root.clone(),
                            path: entry_path.clone(),
                        })?;
                let virtual_path = VirtualPath::from_host_relative(relative)?;
                if let Some(previous) = files.insert(virtual_path.clone(), entry_path.clone()) {
                    return Err(VfsError::CaseCollision {
                        virtual_path,
                        first: previous,
                        second: entry_path,
                    });
                }
            }
        }

        let name: Arc<str> = root.to_string_lossy().into_owned().into();
        Ok(Self { name, files })
    }
}

impl AssetSourceBackend for DirectorySource {
    fn read(&self, path: &VirtualPath, max_bytes: u64) -> Result<Option<Vec<u8>>, VfsError> {
        let Some(host_path) = self.files.get(path) else {
            return Ok(None);
        };
        let metadata = host_path.metadata().map_err(|source| VfsError::Io {
            operation: "inspect loose asset",
            path: host_path.clone(),
            source,
        })?;
        ensure_size(path, metadata.len(), max_bytes)?;
        fs::read(host_path)
            .map(Some)
            .map_err(|source| VfsError::Io {
                operation: "read loose asset",
                path: host_path.clone(),
                source,
            })
    }

    fn contains(&self, path: &VirtualPath) -> bool {
        self.files.contains_key(path)
    }

    fn entry_count(&self) -> usize {
        self.files.len()
    }

    fn append_paths(&self, output: &mut BTreeSet<VirtualPath>) {
        output.extend(self.files.keys().cloned());
    }
}

struct Pk3Source {
    name: Arc<str>,
    archive_path: PathBuf,
    entries: HashMap<VirtualPath, usize>,
    archive: Mutex<zip::ZipArchive<File>>,
}

impl Pk3Source {
    fn open(archive_path: &Path) -> Result<Self, VfsError> {
        let archive_path = archive_path.canonicalize().map_err(|source| VfsError::Io {
            operation: "canonicalize PK3",
            path: archive_path.to_owned(),
            source,
        })?;
        let file = File::open(&archive_path).map_err(|source| VfsError::Io {
            operation: "open PK3",
            path: archive_path.clone(),
            source,
        })?;
        let archive = zip::ZipArchive::new(file).map_err(|source| VfsError::Zip {
            path: archive_path.clone(),
            source,
        })?;
        let mut entries = HashMap::new();

        // Like OpenJK codemp FS_LoadZipFile, mount from the central directory.
        // Opening each local header here makes an unused broken map prevent
        // startup. Payload/header validation belongs in read(). file_names()
        // and by_index() use the same central-directory index order.
        for (index, name) in archive.file_names().enumerate() {
            if name.ends_with(['/', '\\']) {
                continue;
            }

            let virtual_path =
                VirtualPath::new(name).map_err(|source| VfsError::InvalidArchivePath {
                    archive: archive_path.clone(),
                    entry: name.to_owned(),
                    source,
                })?;
            // A later entry wins, matching the VFS mount precedence rule.
            entries.insert(virtual_path, index);
        }

        let name: Arc<str> = archive_path.to_string_lossy().into_owned().into();
        Ok(Self {
            name,
            archive_path,
            entries,
            archive: Mutex::new(archive),
        })
    }
}

impl AssetSourceBackend for Pk3Source {
    fn cacheable(&self) -> bool {
        true
    }
    fn read(&self, path: &VirtualPath, max_bytes: u64) -> Result<Option<Vec<u8>>, VfsError> {
        let Some(index) = self.entries.get(path).copied() else {
            return Ok(None);
        };
        let mut archive = self.archive.lock().map_err(|_| VfsError::Io {
            operation: "lock PK3",
            path: self.archive_path.clone(),
            source: io::Error::other("PK3 reader lock was poisoned"),
        })?;
        let mut entry = archive.by_index(index).map_err(|source| VfsError::Zip {
            path: self.archive_path.clone(),
            source,
        })?;
        ensure_size(path, entry.size(), max_bytes)?;

        let mut bytes = Vec::with_capacity(entry.size().min(usize::MAX as u64) as usize);
        entry
            .by_ref()
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|source| VfsError::Io {
                operation: "decompress PK3 asset",
                path: self.archive_path.clone(),
                source,
            })?;
        ensure_size(path, bytes.len() as u64, max_bytes)?;
        Ok(Some(bytes))
    }

    fn contains(&self, path: &VirtualPath) -> bool {
        self.entries.contains_key(path)
    }

    fn entry_count(&self) -> usize {
        self.entries.len()
    }

    fn append_paths(&self, output: &mut BTreeSet<VirtualPath>) {
        output.extend(self.entries.keys().cloned());
    }

    fn append_in_order(&self, output: &mut Vec<VirtualPath>) {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_unstable_by_key(|(_, index)| **index);
        output.extend(entries.into_iter().map(|(path, _)| path.clone()));
    }
}

fn ensure_size(path: &VirtualPath, actual: u64, maximum: u64) -> Result<(), VfsError> {
    if actual > maximum {
        return Err(VfsError::AssetTooLarge {
            path: path.clone(),
            actual,
            maximum,
        });
    }
    Ok(())
}

#[derive(Debug)]
pub enum VfsError {
    VirtualPath(VirtualPathError),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Zip {
        path: PathBuf,
        source: zip::result::ZipError,
    },
    InvalidArchivePath {
        archive: PathBuf,
        entry: String,
        source: VirtualPathError,
    },
    NotDirectory(PathBuf),
    EscapedDirectoryRoot {
        root: PathBuf,
        path: PathBuf,
    },
    CaseCollision {
        virtual_path: VirtualPath,
        first: PathBuf,
        second: PathBuf,
    },
    AssetTooLarge {
        path: VirtualPath,
        actual: u64,
        maximum: u64,
    },
    TooManyMounts,
}

impl fmt::Display for VfsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VirtualPath(error) => error.fmt(formatter),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} {}: {source}",
                path.display()
            ),
            Self::Zip { path, source } => {
                write!(
                    formatter,
                    "invalid PK3 archive {}: {source}",
                    path.display()
                )
            }
            Self::InvalidArchivePath {
                archive,
                entry,
                source,
            } => write!(
                formatter,
                "unsafe entry {entry:?} in PK3 {}: {source}",
                archive.display()
            ),
            Self::NotDirectory(path) => {
                write!(
                    formatter,
                    "directory mount is not a directory: {}",
                    path.display()
                )
            }
            Self::EscapedDirectoryRoot { root, path } => write!(
                formatter,
                "asset {} escaped directory mount {}",
                path.display(),
                root.display()
            ),
            Self::CaseCollision {
                virtual_path,
                first,
                second,
            } => write!(
                formatter,
                "case-insensitive path collision for {virtual_path}: {} and {}",
                first.display(),
                second.display()
            ),
            Self::AssetTooLarge {
                path,
                actual,
                maximum,
            } => write!(
                formatter,
                "asset {path} is {actual} bytes, exceeding the {maximum}-byte limit"
            ),
            Self::TooManyMounts => formatter.write_str("virtual filesystem mount ID overflow"),
        }
    }
}

impl Error for VfsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::VirtualPath(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::Zip { source, .. } => Some(source),
            Self::InvalidArchivePath { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<VirtualPathError> for VfsError {
    fn from(value: VirtualPathError) -> Self {
        Self::VirtualPath(value)
    }
}
