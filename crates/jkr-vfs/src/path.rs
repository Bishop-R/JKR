use std::error::Error;
use std::fmt;
use std::path::{Component, Path};

/// A normalized, case-insensitive path inside the JKR virtual filesystem.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VirtualPath(String);

impl VirtualPath {
    pub fn new(input: &str) -> Result<Self, VirtualPathError> {
        if input.is_empty() {
            return Err(VirtualPathError::Empty);
        }
        if input.contains('\0') {
            return Err(VirtualPathError::ContainsNul);
        }
        if input.starts_with(['/', '\\']) {
            return Err(VirtualPathError::Absolute);
        }

        let normalized_separators = input.replace('\\', "/");
        let mut components = Vec::new();
        for component in normalized_separators.split('/') {
            match component {
                "" | "." => continue,
                ".." => return Err(VirtualPathError::ParentTraversal),
                value if value.contains(':') => {
                    return Err(VirtualPathError::InvalidComponent(value.to_owned()));
                }
                value if value.chars().any(char::is_control) => {
                    return Err(VirtualPathError::InvalidComponent(value.to_owned()));
                }
                value => components.push(value.to_ascii_lowercase()),
            }
        }

        if components.is_empty() {
            return Err(VirtualPathError::Empty);
        }
        Ok(Self(components.join("/")))
    }

    pub(crate) fn from_host_relative(path: &Path) -> Result<Self, VirtualPathError> {
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(value) => {
                    let value = value.to_str().ok_or(VirtualPathError::NonUtf8)?;
                    components.push(value);
                }
                Component::CurDir => {}
                Component::ParentDir => return Err(VirtualPathError::ParentTraversal),
                Component::RootDir | Component::Prefix(_) => {
                    return Err(VirtualPathError::Absolute);
                }
            }
        }
        Self::new(&components.join("/"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VirtualPathError {
    Empty,
    Absolute,
    ParentTraversal,
    ContainsNul,
    NonUtf8,
    InvalidComponent(String),
}

impl fmt::Display for VirtualPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("virtual path is empty"),
            Self::Absolute => formatter.write_str("virtual path must be relative"),
            Self::ParentTraversal => formatter.write_str("virtual path contains parent traversal"),
            Self::ContainsNul => formatter.write_str("virtual path contains a NUL byte"),
            Self::NonUtf8 => formatter.write_str("host path is not valid UTF-8"),
            Self::InvalidComponent(component) => {
                write!(formatter, "invalid virtual path component {component:?}")
            }
        }
    }
}

impl Error for VirtualPathError {}
