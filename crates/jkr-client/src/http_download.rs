//! HTTP downloads as TaystJK's client makes them (jk2mv's scheme; `cl_main.cpp:1545-1568,
//! 3730-3748`, `cl_parse.cpp:749-768`). A server that offers them says so in its `getinfo`
//! reply: `mvhttp` names a port on the server's own address, `mvhttpurl` a web address.
//! A missing pak is then fetched as `<that>/<remote path>` instead of block by block over
//! the game's own channel.
//!
//! Only plain `http://` is spoken, as TaystJK's client does. Where it is not offered, or
//! a fetch fails, the pak is left to the stock download rather than ending the connection
//! as TaystJK's does.

use crate::download::PakRequest;
use jkr_protocol::InfoString;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::Duration;

/// How long a connection may take to open, and a read to wait.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// The most of a reply's head that is read.
const HEAD_LIMIT: usize = 64 * 1024;
/// The most of a body of unannounced length that is held before it is handed on.
const UNSIZED_LIMIT: usize = 256 * 1024 * 1024;
/// How many redirects are followed.
const REDIRECTS: usize = 3;
/// What one read hands on.
const CHUNK: usize = 64 * 1024;

/// Where a server's paks can be fetched, as its `getinfo` reply says: a non-zero `mvhttp`
/// is a port on the server's own address (`strtol`), else an `mvhttpurl` that names an
/// `http://` address, without a final `/`.
pub fn base_url(server: SocketAddr, info: &InfoString) -> Option<String> {
    let port = info.get("mvhttp").map_or(0, leading_number);
    if port != 0 {
        return Some(format!("http://{}:{port}", server.ip()));
    }
    let url = info.get("mvhttpurl")?;
    if !url.to_ascii_lowercase().contains("http://") {
        return None;
    }
    Some(url.strip_suffix('/').unwrap_or(url).to_owned())
}

/// `strtol(text, NULL, 10)`: the leading number, zero without one.
fn leading_number(text: &str) -> i64 {
    let text = text.trim_start();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let end = digits
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end]
        .parse::<i64>()
        .map_or(0, |number| sign * number)
}

/// Ask the server (`getinfo`) where its paks can be fetched; `None` where it offers no
/// HTTP downloads or does not answer.
pub fn query_base_url(server: SocketAddr) -> Option<String> {
    use jkr_network::query::{Answer, Request, run};
    let responses = run(
        server,
        Request::Ping,
        Duration::from_secs(2),
        Duration::from_millis(500),
    )
    .ok()?;
    responses
        .into_iter()
        .find_map(|response| match response.answer {
            Answer::Info(info) => base_url(server, &info),
            _ => None,
        })
}

/// What a fetch hands the download worker, in order.
#[derive(Debug)]
pub enum Fetched {
    /// The pak's length.
    Size(u64),
    /// The next bytes.
    Data(Vec<u8>),
    /// Everything has been handed on.
    Done,
    /// The fetch failed.
    Failed(String),
}

/// Fetch `request` from `base` on a thread of its own; what it gets arrives in order on
/// the receiver. Dropping the receiver stops it.
pub fn fetch(base: &str, request: &PakRequest) -> Receiver<Fetched> {
    let (sender, receiver) = sync_channel(16);
    let url = format!("{base}/{}", request.remote);
    std::thread::spawn(move || {
        if let Err(error) = fetch_into(&url, &sender) {
            let _ = sender.send(Fetched::Failed(error));
        }
    });
    receiver
}

fn fetch_into(url: &str, sender: &SyncSender<Fetched>) -> Result<(), String> {
    let mut url = url.to_owned();
    for _ in 0..=REDIRECTS {
        let (host, port, path) = parse_url(&url)?;
        let address = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|error| format!("{host}: {error}"))?
            .next()
            .ok_or_else(|| format!("{host}: no address"))?;
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
            .map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .map_err(|error| error.to_string())?;
        let authority = if port == 80 {
            host.clone()
        } else {
            format!("{host}:{port}")
        };
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {authority}\r\nUser-Agent: JKR\r\nAccept: */*\r\nConnection: close\r\n\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|error| error.to_string())?;
        let mut reader = BufReader::new(stream);
        let head = read_head(&mut reader)?;
        match head.status {
            200 => return read_body(&mut reader, &head, sender),
            301 | 302 | 303 | 307 | 308 => {
                let location = head.location.ok_or("redirect without a location")?;
                url = if location.starts_with('/') {
                    format!("http://{authority}{location}")
                } else {
                    location
                };
            }
            status => return Err(format!("HTTP {status}")),
        }
    }
    Err("too many redirects".into())
}

/// `http://host[:port]/path` taken apart; anything else is refused.
fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("http://"))
        .map(|_| &url[7..])
        .ok_or_else(|| format!("not an http:// address: {url}"))?;
    let (authority, path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .map_err(|_| format!("bad port in {url}"))?,
        ),
        None => (authority, 80),
    };
    if host.is_empty() {
        return Err(format!("no host in {url}"));
    }
    Ok((host.to_owned(), port, path.to_owned()))
}

/// A reply's status and the headers that matter.
#[derive(Debug, Default)]
struct Head {
    status: u16,
    length: Option<u64>,
    chunked: bool,
    location: Option<String>,
}

fn read_head(reader: &mut impl BufRead) -> Result<Head, String> {
    let mut head = Head::default();
    let mut read = 0;
    let mut line = String::new();
    let mut first = true;
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        read += bytes;
        if bytes == 0 || read > HEAD_LIMIT {
            return Err("bad HTTP reply".into());
        }
        let text = line.trim_end_matches(['\r', '\n']);
        if first {
            first = false;
            head.status = text
                .split_whitespace()
                .nth(1)
                .and_then(|status| status.parse().ok())
                .ok_or("bad HTTP status line")?;
            continue;
        }
        if text.is_empty() {
            return Ok(head);
        }
        let Some((name, value)) = text.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            head.length = value.parse().ok();
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            head.chunked = value.to_ascii_lowercase().contains("chunked");
        } else if name.eq_ignore_ascii_case("location") {
            head.location = Some(value.to_owned());
        }
    }
}

/// Hand the body on: streamed when its length is announced, else gathered first.
fn read_body(
    reader: &mut impl BufRead,
    head: &Head,
    sender: &SyncSender<Fetched>,
) -> Result<(), String> {
    let send = |fetched| sender.send(fetched).map_err(|_| "cancelled".to_owned());
    if let (Some(length), false) = (head.length, head.chunked) {
        send(Fetched::Size(length))?;
        let mut left = length;
        while left > 0 {
            let mut chunk = vec![0; CHUNK.min(left as usize)];
            let read = reader.read(&mut chunk).map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("truncated HTTP body".into());
            }
            chunk.truncate(read);
            left -= read as u64;
            send(Fetched::Data(chunk))?;
        }
        return send(Fetched::Done);
    }
    let body = if head.chunked {
        read_chunked(reader)?
    } else {
        let mut body = Vec::new();
        reader
            .take(UNSIZED_LIMIT as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|error| error.to_string())?;
        body
    };
    if body.len() > UNSIZED_LIMIT {
        return Err("HTTP body too large".into());
    }
    send(Fetched::Size(body.len() as u64))?;
    for chunk in body.chunks(CHUNK) {
        send(Fetched::Data(chunk.to_vec()))?;
    }
    send(Fetched::Done)
}

/// A `Transfer-Encoding: chunked` body, whole.
fn read_chunked(reader: &mut impl BufRead) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        let size = line.trim().split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size, 16).map_err(|_| "bad HTTP chunk size")?;
        if size == 0 {
            return Ok(body);
        }
        if body.len() + size > UNSIZED_LIMIT {
            return Err("HTTP body too large".into());
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader
            .read_exact(&mut body[start..])
            .map_err(|error| error.to_string())?;
        line.clear();
        reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn info(text: &str) -> InfoString {
        InfoString::parse(text).unwrap()
    }

    #[test]
    fn the_base_comes_from_mvhttp_or_mvhttpurl() {
        let server: SocketAddr = "10.0.0.2:29070".parse().unwrap();
        assert_eq!(
            base_url(server, &info("\\mvhttp\\18200")).as_deref(),
            Some("http://10.0.0.2:18200")
        );
        assert_eq!(
            base_url(
                server,
                &info("\\mvhttp\\0\\mvhttpurl\\http://dl.example.org/jka/")
            )
            .as_deref(),
            Some("http://dl.example.org/jka")
        );
        assert_eq!(base_url(server, &info("\\mvhttpurl\\https://x.org")), None);
        assert_eq!(base_url(server, &info("\\hostname\\x")), None);
        assert_eq!(leading_number("18200abc"), 18_200);
        assert_eq!(leading_number("abc"), 0);
    }

    #[test]
    fn urls_are_taken_apart() {
        assert_eq!(
            parse_url("http://host:18200/base/x.pk3").unwrap(),
            ("host".into(), 18_200, "/base/x.pk3".into())
        );
        assert_eq!(
            parse_url("HTTP://host").unwrap(),
            ("host".into(), 80, "/".into())
        );
        assert!(parse_url("https://host/x").is_err());
    }

    /// One reply from a local server, and everything the fetch hands on.
    fn serve(reply: &'static [u8]) -> Vec<String> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap() == 1 {
                request.push(byte[0]);
            }
            stream.write_all(reply).unwrap();
        });
        let request = PakRequest {
            remote: "base/x.pk3".into(),
            filename: "x.pk3".into(),
            checksum: 0,
        };
        let receiver = fetch(&format!("http://127.0.0.1:{port}"), &request);
        let mut out = Vec::new();
        for fetched in receiver.iter() {
            let end = matches!(fetched, Fetched::Done | Fetched::Failed(_));
            out.push(match fetched {
                Fetched::Size(size) => format!("size {size}"),
                Fetched::Data(data) => format!("data {}", String::from_utf8_lossy(&data)),
                Fetched::Done => "done".into(),
                Fetched::Failed(error) => format!("failed {error}"),
            });
            if end {
                break;
            }
        }
        out
    }

    #[test]
    fn a_sized_body_is_streamed() {
        assert_eq!(
            serve(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello"),
            ["size 5", "data hello", "done"]
        );
    }

    #[test]
    fn a_chunked_body_is_gathered() {
        assert_eq!(
            serve(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nhel\r\n2\r\nlo\r\n0\r\n\r\n"),
            ["size 5", "data hello", "done"]
        );
    }

    #[test]
    fn a_refusal_fails() {
        assert_eq!(
            serve(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n"),
            ["failed HTTP 403"]
        );
    }
}
