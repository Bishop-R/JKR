//! Viewer filesystem policy for portable shell scripts.

use jkr_shell::CommandFileResolver;
use std::fs;
use std::path::{Component, Path};

pub(super) struct Resolver<'a> {
    config_directory: &'a Path,
    vfs: Option<&'a jkr_vfs::VirtualFileSystem>,
}

impl<'a> Resolver<'a> {
    pub(super) fn new(
        config_directory: &'a Path,
        vfs: Option<&'a jkr_vfs::VirtualFileSystem>,
    ) -> Self {
        Self {
            config_directory,
            vfs,
        }
    }
}

impl CommandFileResolver for Resolver<'_> {
    fn file_command(
        &mut self,
        name: &str,
        args: &[String],
        dump: &str,
    ) -> Result<Vec<String>, String> {
        self.command(name, args, dump)
    }
    fn read_command_file(&mut self, path: &str) -> Result<Option<String>, String> {
        validate_relative_path(path)?;
        let host_path = self.config_directory.join(path);
        match fs::read(&host_path) {
            Ok(bytes) => return decode(path, bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("could not read {}: {error}", host_path.display())),
        }
        let Some(vfs) = self.vfs else {
            return Ok(None);
        };
        let asset = vfs.read(path).map_err(|error| error.to_string())?;
        asset.map(|asset| decode(path, asset.bytes)).transpose()
    }
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err("exec path must stay inside the config or VFS root".to_owned());
    }
    Ok(())
}

fn decode(path: &str, bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("{path} is not valid UTF-8 command text"))
}

#[path = "console_files.rs"]
mod files;
