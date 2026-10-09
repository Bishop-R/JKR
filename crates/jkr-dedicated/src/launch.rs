//! The launch settings an OpenJK start script gives a dedicated server on its command
//! line (`+set fs_basepath ... +set net_port 29071`): `fs_basepath` and `fs_homepath`,
//! which `FS_Startup` reads before any config runs (`Com_StartupVariable`), and `net_ip`
//! and `net_port` (`NET_GetCvars`, `net_ip.cpp:866-870`). The socket opens as
//! `NET_OpenIP` (`net_ip.cpp:817-846`) opens it: at `net_port`, else at the first free one
//! of the nine ports above it, "so multiple dedicated servers can be started without
//! requiring a different net_port for each one", and `net_port` then says which.
//!
//! JKR's own `--game-data`, `--home` and `--bind` win over them.

use crate::cvars::{CVAR_INIT, CVAR_LATCH, CVAR_PROTECTED, Cvars};
use std::io::{Error, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};

/// `PORT_SERVER`: where a server listens when nothing names another port.
pub const PORT_SERVER: u16 = 29_070;
/// How many ports `NET_OpenIP` tries, `net_port` first.
const PORT_TRIES: u16 = 10;

/// A path the command line gave `name` (`fs_basepath`, `fs_homepath`); `None` while it is
/// unset or empty.
pub fn startup_path(cvars: &Cvars, name: &[u8]) -> Option<PathBuf> {
    let value = cvars.string(name);
    (!value.is_empty()).then(|| PathBuf::from(String::from_utf8_lossy(value).into_owned()))
}

/// `FS_Startup`'s and `NET_GetCvars`' registrations: the paths the server runs from, which
/// nobody changes once it runs (`CVAR_INIT | CVAR_PROTECTED`), and where it listens,
/// whose change waits for a restart (`CVAR_LATCH`). The paths read what the server
/// actually uses, whichever option or `+set` gave it.
pub fn register(cvars: &mut Cvars, game_data: Option<&Path>, home: Option<&Path>) {
    for (name, path, about) in [
        (
            &b"fs_basepath"[..],
            game_data,
            &b"The game's directory, which holds base"[..],
        ),
        (
            b"fs_homepath",
            home,
            b"The operator's directory: configs, logs and demos",
        ),
    ] {
        let text = path.map_or_else(Vec::new, |path| {
            path.to_string_lossy().into_owned().into_bytes()
        });
        cvars.get(name, &text, CVAR_INIT | CVAR_PROTECTED, Some(about));
        cvars.set(name, &text);
    }
    cvars.get(b"net_ip", b"localhost", CVAR_LATCH, None);
    cvars.get(
        b"net_port",
        PORT_SERVER.to_string().as_bytes(),
        CVAR_LATCH,
        None,
    );
}

/// `NET_OpenIP`: a UDP socket on `net_ip`'s interface at `net_port`, else at the first of
/// the nine ports above it that is free; `net_port` is then set to the port it got.
pub fn open_ip(cvars: &mut Cvars) -> std::io::Result<UdpSocket> {
    let ip = interface(cvars.string(b"net_ip"))?;
    let port = cvars.integer(b"net_port");
    // `PORT_ANY` (and any negative port) is whatever port the system gives.
    let first = if port < 0 {
        0
    } else {
        u16::try_from(port).map_err(|_| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("net_port {port} is not a port"),
            )
        })?
    };
    let mut refused = None;
    for offset in 0..if first == 0 { 1 } else { PORT_TRIES } {
        let Some(port) = first.checked_add(offset) else {
            break;
        };
        match UdpSocket::bind(SocketAddrV4::new(ip, port)) {
            Ok(socket) => {
                let port = socket.local_addr()?.port();
                cvars.set(b"net_port", port.to_string().as_bytes());
                return Ok(socket);
            }
            Err(error) => refused = Some(error),
        }
    }
    Err(refused.unwrap_or_else(|| Error::new(ErrorKind::AddrInUse, "no free port")))
}

/// `NET_IPSocket`'s interface: every one for `localhost` or nothing, else the address
/// `NET_StringToAdr` makes of the name.
fn interface(name: &[u8]) -> std::io::Result<Ipv4Addr> {
    let name = String::from_utf8_lossy(name);
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("localhost") {
        return Ok(Ipv4Addr::UNSPECIFIED);
    }
    if let Ok(ip) = name.parse() {
        return Ok(ip);
    }
    (name, 0)
        .to_socket_addrs()?
        .find_map(|address| match address {
            SocketAddr::V4(address) => Some(*address.ip()),
            SocketAddr::V6(_) => None,
        })
        .ok_or_else(|| {
            Error::new(
                ErrorKind::AddrNotAvailable,
                format!("net_ip {name} has no IPv4 address"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(cvars: &mut Cvars, name: &[u8], value: &[u8]) {
        cvars.user_set(name, Some(value), &mut |_| {});
    }

    #[test]
    fn localhost_and_nothing_are_every_interface() {
        for name in [&b"localhost"[..], b"LocalHost", b"", b"  "] {
            assert_eq!(interface(name).unwrap(), Ipv4Addr::UNSPECIFIED);
        }
        assert_eq!(
            interface(b"127.0.0.1").unwrap(),
            Ipv4Addr::new(127, 0, 0, 1)
        );
    }

    #[test]
    fn the_command_lines_paths_and_port_are_used() {
        let mut cvars = Cvars::new();
        assert_eq!(startup_path(&cvars, b"fs_basepath"), None);
        set(&mut cvars, b"fs_basepath", b"/srv/jka");
        set(&mut cvars, b"net_port", b"0");
        set(&mut cvars, b"net_ip", b"127.0.0.1");
        let game_data = startup_path(&cvars, b"fs_basepath");
        assert_eq!(game_data.as_deref(), Some(Path::new("/srv/jka")));
        register(&mut cvars, game_data.as_deref(), None);
        assert_eq!(cvars.string(b"fs_basepath"), b"/srv/jka");
        assert_eq!(cvars.string(b"fs_homepath"), b"");
        let socket = open_ip(&mut cvars).unwrap();
        let bound = socket.local_addr().unwrap();
        assert_eq!(bound.ip().to_string(), "127.0.0.1");
        // The port it got, not the 0 it was asked for.
        assert_eq!(cvars.integer(b"net_port"), i32::from(bound.port()));
    }

    #[test]
    fn a_taken_port_moves_the_server_to_the_next() {
        // Find a port whose next one is free, and hold it.
        let (held, port) = (0..20)
            .find_map(|_| {
                let held = UdpSocket::bind("127.0.0.1:0").ok()?;
                let port = held.local_addr().ok()?.port();
                let next = UdpSocket::bind(("127.0.0.1", port.checked_add(1)?)).ok()?;
                drop(next);
                Some((held, port))
            })
            .expect("a free pair of ports");
        let mut cvars = Cvars::new();
        register(&mut cvars, None, None);
        set(&mut cvars, b"net_ip", b"127.0.0.1");
        cvars.set(b"net_port", port.to_string().as_bytes());
        let socket = open_ip(&mut cvars).unwrap();
        assert_eq!(socket.local_addr().unwrap().port(), port + 1);
        assert_eq!(cvars.integer(b"net_port"), i32::from(port + 1));
        drop(held);
    }

    #[test]
    fn a_port_out_of_range_is_refused() {
        let mut cvars = Cvars::new();
        register(&mut cvars, None, None);
        cvars.set(b"net_port", b"70000");
        assert_eq!(
            open_ip(&mut cvars).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
}
