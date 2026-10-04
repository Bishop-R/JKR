//! Portable client-shell primitives.
//!
//! This crate contains no renderer, window-system, filesystem-location, or
//! legacy-network policy. Applications register their cvars and commands,
//! select an explicit persistence path, and present the console however they
//! choose.

#![warn(missing_docs)]

mod bind;
mod command;
mod command_buffer;
mod config;
mod cvar;
pub mod key_names;

mod shell;
mod shell_error;

pub use bind::{BindError, BindTable};
pub use command::{CommandError, CommandRegistry, split_commands, tokenize};
pub use command_buffer::{
    CommandBuffer, CommandBufferError, CommandFileResolver, MAX_COMMAND_BUFFER_BYTES,
    MAX_EXEC_DEPTH, NoCommandFiles,
};
pub use config::{ConfigError, load_config, save_config};
pub use cvar::{Cvar, CvarChange, CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
pub use shell::{CommandSource, CompletionKey, ConsoleLine, ConsoleLineKind, Shell};
pub use shell_error::ShellError;
