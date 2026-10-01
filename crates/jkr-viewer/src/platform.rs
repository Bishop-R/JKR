//! Platform composition for per-user JKR state.

use std::env;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

/// Ask the native interface-listing utility; no external probe packets or DNS tricks.
pub(crate) fn interface_addresses() -> Result<String, Error> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "windows") {
        ("ipconfig", &[])
    } else if cfg!(target_os = "linux") {
        ("ip", &["-brief", "address", "show"])
    } else {
        ("ifconfig", &[])
    };
    let output = std::process::Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Resolve the native per-user configuration file without touching retail or
/// repository data.
pub(crate) fn user_config_file() -> Result<PathBuf, Error> {
    let root = if cfg!(target_os = "windows") {
        env_path("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env_path("HOME")?.join("Library/Application Support")
    } else if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        PathBuf::from(path)
    } else {
        env_path("HOME")?.join(".config")
    };
    Ok(root.join("jkr/config.cfg"))
}

fn env_path(name: &str) -> Result<PathBuf, Error> {
    env::var_os(name)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("{name} is not set")))
}
