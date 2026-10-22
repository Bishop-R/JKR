//! TaystJK's HTTP download server (`sv_httpDownloads`, `sv_httpServerPort`;
//! `qcommon/net_http.cpp`, `server/sv_init.cpp:755-775`): the paks a client may download
//! from the game's own channel, also served over plain HTTP, much faster. Only the
//! server's own machine and the addresses of the clients in the game are answered; a
//! request for anything else is refused. The game's `getinfo` reply tells clients where
//! to ask (`mvhttp`).
//!
//! A thread accepts connections and serves each on a thread of its own, at most 64 at a
//! time; the game publishes what may be fetched and by whom.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// `HTTPSRV_STDPORT`: the first port tried when none is named; fifteen more follow.
pub const STANDARD_PORT: u16 = 18_200;
/// `HTTPSRV_CONN_LIMIT`.
const CONNECTIONS: usize = 64;
/// `HTTPSRV_READ_LIMIT`: the most a request may hold.
const REQUEST_LIMIT: usize = 2_048;
/// `HTTPSRV_TIMEOUT_MS`.
const TIMEOUT: Duration = Duration::from_secs(10);

/// What the game publishes for the server's threads.
#[derive(Debug, Default)]
struct Shared {
    /// The files that may be fetched: each request path, lower case, and the file.
    files: Vec<(String, PathBuf)>,
    /// The addresses of the clients in the game.
    clients: Vec<Ipv4Addr>,
}

/// A running HTTP download server; dropping it stops it.
#[derive(Debug)]
pub struct HttpDownloads {
    port: u16,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl HttpDownloads {
    /// `NET_HTTP_StartServer`: listening on every interface at `port`, or at the first free
    /// one of 18200 to 18215 when `port` is 0.
    pub fn start(port: u16) -> std::io::Result<Self> {
        let listener = if port != 0 {
            TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))?
        } else {
            (STANDARD_PORT..=STANDARD_PORT + 15)
                .find_map(|port| TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).ok())
                .ok_or(std::io::ErrorKind::AddrInUse)?
        };
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let (shared, stop) = (
            Arc::new(Mutex::new(Shared::default())),
            Arc::new(AtomicBool::new(false)),
        );
        let thread = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            std::thread::spawn(move || accept_loop(&listener, &shared, &stop))
        };
        Ok(Self {
            port,
            shared,
            stop,
            thread: Some(thread),
        })
    }

    /// The port it listens on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The files that may be fetched now: each request path (`base/<pak>.pk3`) and the file.
    pub fn set_files(&self, files: impl IntoIterator<Item = (String, PathBuf)>) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.files = files
                .into_iter()
                .map(|(name, path)| (name.to_ascii_lowercase(), path))
                .collect();
        }
    }

    /// `NET_HTTP_AllowClient`/`NET_HTTP_DenyClient`: the addresses of the clients in the game.
    pub fn set_clients(&self, clients: impl IntoIterator<Item = Ipv4Addr>) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.clients = clients.into_iter().collect();
        }
    }
}

impl Drop for HttpDownloads {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Mutex<Shared>>, stop: &AtomicBool) {
    let active = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, from)) => {
                if active.load(Ordering::Relaxed) >= CONNECTIONS {
                    let _ = reply(&mut stream, "503 Service Unavailable");
                    continue;
                }
                active.fetch_add(1, Ordering::Relaxed);
                let (shared, active) = (Arc::clone(shared), Arc::clone(&active));
                std::thread::spawn(move || {
                    serve(stream, from, &shared);
                    active.fetch_sub(1, Ordering::Relaxed);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// One connection: an answered address's `GET` (or `HEAD`) of a file that may be fetched
/// gets it, anything else `403 Forbidden`.
fn serve(mut stream: TcpStream, from: SocketAddr, shared: &Mutex<Shared>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(TIMEOUT));
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let allowed = match from.ip() {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || shared
                    .lock()
                    .is_ok_and(|shared| shared.clients.contains(&ip))
        }
        // "Connection dropped: IPv6 not allowed".
        IpAddr::V6(_) => false,
    };
    if !allowed {
        let _ = reply(&mut stream, "403 Forbidden");
        return;
    }
    let Some((method, path)) = read_request(&stream) else {
        return;
    };
    let file = (method == "GET" || method == "HEAD")
        .then(|| {
            let wanted = path.trim_start_matches('/').to_ascii_lowercase();
            shared.lock().ok().and_then(|shared| {
                shared
                    .files
                    .iter()
                    .find(|(name, _)| *name == wanted)
                    .map(|(_, file)| file.clone())
            })
        })
        .flatten()
        .and_then(|file| std::fs::File::open(file).ok());
    let Some(mut file) = file else {
        let _ = reply(&mut stream, "403 Forbidden");
        return;
    };
    let length = file.metadata().map_or(0, |metadata| metadata.len());
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(head.as_bytes()).is_ok() && method == "GET" {
        let _ = std::io::copy(&mut file, &mut stream);
    }
}

/// The request line's method and path (without its query), once the whole head has come
/// within the limit and the time.
fn read_request(stream: &TcpStream) -> Option<(String, String)> {
    let mut reader = BufReader::new(stream.take(REQUEST_LIMIT as u64));
    let mut first = String::new();
    reader.read_line(&mut first).ok()?;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    let mut words = first.split_whitespace();
    let method = words.next()?.to_owned();
    let path = words.next()?;
    let path = path.split(['?', '#']).next().unwrap_or(path).to_owned();
    Some((method, path))
}

fn reply(stream: &mut TcpStream, status: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        reply
    }

    #[test]
    fn only_published_files_are_served() {
        let directory = std::env::temp_dir().join(format!("jkr-http-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let pak = directory.join("mymap.pk3");
        std::fs::write(&pak, b"PK-bytes").unwrap();
        // Port 0 would scan from 18200; a free port of the system's choice keeps the test
        // clear of a real server.
        let free = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let server = HttpDownloads::start(free).unwrap();
        server.set_files([("base/MyMap.pk3".to_owned(), pak.clone())]);
        let ok = get(server.port(), "/base/mymap.pk3");
        assert!(ok.starts_with("HTTP/1.1 200 OK\r\n"), "{ok}");
        assert!(ok.contains("Content-Length: 8\r\n") && ok.ends_with("PK-bytes"));
        assert!(get(server.port(), "/base/other.pk3").starts_with("HTTP/1.1 403"));
        assert!(get(server.port(), "/../mymap.pk3").starts_with("HTTP/1.1 403"));
        drop(server);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
