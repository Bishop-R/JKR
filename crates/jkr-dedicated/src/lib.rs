//! JKR's headless server: platform composition of the native server core
//! (`jkr-server`), the JKA game rules (`jkr-game-jka`) and the protocol-26 endpoint
//! (`jkr-network`). The executable in `main.rs` adds only the socket, the clocks and
//! the operator's options.

pub(crate) mod admin;
pub mod bridge;
pub mod collision;
pub mod command_buffer;
pub mod config_files;
pub mod cvars;
pub(crate) mod damage_numbers;
pub mod frame_clock;
pub mod http_downloads;
pub mod launch;
pub mod map;
pub mod master;
pub(crate) mod motd;
pub mod peer;
pub(crate) mod players;
pub mod pure_files;
pub mod visibility;
