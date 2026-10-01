//! Stable archive-cvar and bind persistence.

use crate::{BindError, BindTable, CvarError, CvarFlags, CvarRegistry, tokenize};
use std::fmt::{Display, Formatter, Write as _};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Load archived cvars and binds from an explicit application-owned path.
pub fn load_config(
    path: &Path,
    cvars: &mut CvarRegistry,
    binds: &mut BindTable,
) -> Result<(), ConfigError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    // Older builds may write their alias default beside a preserved newer name.
    // An explicit canonical archive wins, independently of generated name order.
    let canonical_archives: std::collections::HashSet<String> = contents
        .lines()
        .filter_map(|line| tokenize(line.trim()).ok())
        .filter_map(|tokens| match tokens.as_slice() {
            [command, name, _] if command.eq_ignore_ascii_case("seta") => cvars
                .get(name)
                .filter(|cvar| cvar.name.eq_ignore_ascii_case(name))
                .map(|cvar| cvar.name.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    // A generated config is the complete persisted bind table. Clearing the
    // application defaults first makes `unbind` survive the next launch.
    binds.clear();
    for (line_index, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        let tokens = tokenize(line).map_err(|error| ConfigError::Line {
            line: line_index + 1,
            message: error.to_string(),
        })?;
        match tokens.as_slice() {
            [command, name, value] if command.eq_ignore_ascii_case("seta") => {
                if cvars.get(name).is_none() {
                    cvars.register(crate::CvarDefinition::new(
                        name,
                        value.clone(),
                        CvarFlags::ARCHIVE | CvarFlags::USER_CREATED,
                        "User-created cvar",
                    ))?;
                }
                let cvar = cvars.get(name).expect("registered above");
                if !cvar.name.eq_ignore_ascii_case(name)
                    && canonical_archives.contains(&cvar.name.to_ascii_lowercase())
                {
                    continue;
                }
                if cvar.flags.contains(CvarFlags::ARCHIVE) {
                    cvars
                        .restore_text(name, value)
                        .map_err(|error| ConfigError::Line {
                            line: line_index + 1,
                            message: error.to_string(),
                        })?;
                }
            }
            [command, key, value] if command.eq_ignore_ascii_case("bind") => {
                binds
                    .bind(key, value.clone())
                    .map_err(|error| ConfigError::Line {
                        line: line_index + 1,
                        message: error.to_string(),
                    })?;
            }
            _ => {
                return Err(ConfigError::Line {
                    line: line_index + 1,
                    message: "expected `seta name value` or `bind key command`".to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Atomically save archived cvars and binds in deterministic name order.
pub fn save_config(
    path: &Path,
    cvars: &CvarRegistry,
    binds: &BindTable,
) -> Result<(), ConfigError> {
    let mut contents = String::from("// JKR generated configuration. Edit while JKR is closed.\n");
    for cvar in cvars
        .iter()
        .filter(|cvar| cvar.flags.contains(CvarFlags::ARCHIVE))
        .filter(|cvar| !cvar.flags.contains(CvarFlags::OMIT_DEFAULT) || cvar.value != cvar.default)
    {
        writeln!(
            contents,
            "seta {} \"{}\"",
            cvar.name,
            escape(&cvar.value.as_text())
        )?;
    }
    for (key, command) in binds.iter() {
        writeln!(contents, "bind \"{}\" \"{}\"", escape(key), escape(command))?;
    }
    let parent = path
        .parent()
        .ok_or_else(|| ConfigError::NoParent(path.to_owned()))?;
    fs::create_dir_all(parent)?;
    let temporary = temporary_path(path);
    fs::write(&temporary, contents)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(|| "config".into(), |name| name.to_os_string());
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

/// Failure while reading or writing a shell configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// Filesystem operation failed.
    Io(std::io::Error),
    /// Serializing generated text failed.
    Format(std::fmt::Error),
    /// A persisted cvar value was invalid.
    Cvar(CvarError),
    /// A persisted binding was invalid.
    Bind(BindError),
    /// The requested config path has no parent directory.
    NoParent(PathBuf),
    /// A source line did not match the stable config grammar.
    Line {
        /// One-based source line number.
        line: usize,
        /// Parser or validation diagnostic.
        message: String,
    },
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, formatter),
            Self::Format(error) => Display::fmt(error, formatter),
            Self::Cvar(error) => Display::fmt(error, formatter),
            Self::Bind(error) => Display::fmt(error, formatter),
            Self::NoParent(path) => {
                write!(formatter, "config path {} has no parent", path.display())
            }
            Self::Line { line, message } => write!(formatter, "config line {line}: {message}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<std::fmt::Error> for ConfigError {
    fn from(value: std::fmt::Error) -> Self {
        Self::Format(value)
    }
}

impl From<CvarError> for ConfigError {
    fn from(value: CvarError) -> Self {
        Self::Cvar(value)
    }
}

impl From<BindError> for ConfigError {
    fn from(value: BindError) -> Self {
        Self::Bind(value)
    }
}
