//! Protocol-26 downloads before cgame activation (`cl_parse.cpp:634-719`).

use crate::ClientError;
use jkr_network::{LegacyConnection, NetworkError, ServerMessage};
use jkr_protocol::{GameState, InfoString, MessageReader, ServiceCommand};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A validated remote request and its feed-independent referenced checksum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PakRequest {
    /// Wire path, including the game-directory prefix.
    pub remote: String,
    /// Plain basename in the application's chosen write directory.
    pub filename: String,
    /// `sv_referencedPaks` checksum (not the pure checksum).
    pub checksum: i32,
}

/// Host-owned download capability. Called only on the connection worker.
pub trait DownloadStorage: Send {
    /// Reattach the worker's bounded progress channel and cancellation token on map changes.
    fn feedback(
        &mut self,
        progress: std::sync::mpsc::SyncSender<String>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    );
    /// Check cancellation while waiting for server packets.
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
    /// Compare references against a cached local checksum inventory; no disk I/O.
    fn missing(&self, game: &GameState) -> Result<Vec<PakRequest>, String>;
    /// Open a temporary file for an explicitly requested pak.
    fn begin(&mut self, request: &PakRequest) -> Result<(), String>;
    /// Accept the declared size before writing any payload.
    fn size(&mut self, bytes: u64) -> Result<(), String>;
    /// Append one in-order block and update progress.
    fn append(&mut self, bytes: &[u8]) -> Result<(), String>;
    /// Verify and publish a complete pak, then update the cached inventory.
    fn finish(&mut self) -> Result<(), String>;
    /// Remove temporary output on refusal, timeout, cancellation or malformed data.
    fn abort(&mut self);
}

/// Parsed download block; the small payload is bounded by the stock message size.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Block {
    /// Low 16 bits of the monotonically increasing block index.
    pub(crate) number: u16,
    /// Present only on the initial block zero.
    pub(crate) size: Option<u64>,
    /// Empty payload denotes EOF, not a data-less keepalive.
    pub(crate) data: Vec<u8>,
}

/// Read precisely the fields written by `sv_client.cpp:859-870`.
pub(crate) fn block(reader: &mut MessageReader<'_>, first: bool) -> Result<Block, ClientError> {
    let number = reader.read_i16()? as u16;
    let size = if number == 0 && first {
        let size = reader.read_i32()?;
        if size < 0 {
            let reason = reader.read_c_string(1024)?;
            return Err(ClientError::Download(
                String::from_utf8_lossy(&reason).into_owned(),
            ));
        }
        Some(size as u64)
    } else {
        None
    };
    let length = reader.read_i16()?;
    // MSG_ReadShort is signed; every nonnegative i16 fits stock MAX_MSGLEN (49152).
    if length < 0 {
        return Err(ClientError::Download(
            "invalid download block length".into(),
        ));
    }
    let mut data = Vec::with_capacity(length as usize);
    for _ in 0..length {
        data.push(reader.read_u8()?);
    }
    Ok(Block { number, size, data })
}

/// Transfer missing paks, then request and return a fresh gamestate.
pub(crate) fn run(
    connection: &mut impl Transport,
    message: ServerMessage,
    storage: &mut dyn DownloadStorage,
    sequence: &mut i32,
    pending: &mut VecDeque<(i32, Vec<u8>)>,
) -> Result<ServerMessage, ClientError> {
    let initial = jkr_protocol::decode_initial_gamestate(&message.payload)?;
    let requests = storage
        .missing(&initial.game_state)
        .map_err(ClientError::Download)?;
    if requests.is_empty() {
        return Ok(message);
    }
    let info = initial
        .game_state
        .config_string(1)
        .ok_or(ClientError::MissingSystemInfo)?;
    let info =
        InfoString::parse(std::str::from_utf8(info).map_err(|_| ClientError::NonUtf8SystemInfo)?)?;
    let server_id = info
        .get_i32("sv_serverid")
        .ok_or(ClientError::MissingServerId)?;
    for command in &initial.server_commands {
        connection.record_server_command(command.sequence, &command.command);
    }
    let http = connection
        .server_address()
        .and_then(crate::http_download::query_base_url);
    let mut exchange = Exchange {
        connection,
        sequence,
        pending,
        server_id,
        message_ack: message.sequence,
        reliable_ack: initial.game_state.server_command_sequence,
    };
    exchange.acknowledge(initial.reliable_acknowledge);
    let result = (|| {
        for request in &requests {
            // TaystJK's HTTP download first where the server offers one; a failed fetch
            // leaves the pak to the stock download below.
            if let Some(base) = &http {
                storage.begin(request).map_err(ClientError::Download)?;
                match http_fetch(&mut exchange, base, request, storage, &initial.game_state) {
                    Ok(()) => continue,
                    Err(error) => {
                        storage.abort();
                        eprintln!(
                            "download: HTTP {base}/{} failed ({error}); asking the server instead",
                            request.remote
                        );
                    }
                }
            }
            storage.begin(request).map_err(ClientError::Download)?;
            exchange.queue(format!("download {}", request.remote).into_bytes())?;
            let mut expected = 0_u32;
            let mut progress = Instant::now();
            let deadline = progress + Duration::from_secs(1800);
            loop {
                storage.check().map_err(ClientError::Download)?;
                if progress.elapsed() > Duration::from_secs(30) || Instant::now() > deadline {
                    return Err(ClientError::Download("download timed out".into()));
                }
                exchange.send()?;
                let Some(message) = exchange.receive()? else {
                    continue;
                };
                let mut reader = MessageReader::new(&message.payload);
                exchange.acknowledge(reader.read_i32()?);
                let mut complete = false;
                loop {
                    match reader.read_service_command()? {
                        ServiceCommand::End => break,
                        ServiceCommand::Nop => {}
                        ServiceCommand::ServerCommand => exchange.server_command(&mut reader)?,
                        ServiceCommand::SetGame => {
                            reader.read_c_string(256)?;
                        }
                        ServiceCommand::Snapshot => {
                            skip_snapshot(&mut reader, &message, &initial.game_state)?;
                        }
                        ServiceCommand::Download => {
                            let block = block(&mut reader, expected == 0)?;
                            if block.number != expected as u16 {
                                continue; // Stock ignores repeats/out-of-order blocks.
                            }
                            if let Some(size) = block.size {
                                storage.size(size).map_err(ClientError::Download)?;
                            }
                            storage.append(&block.data).map_err(ClientError::Download)?;
                            exchange.queue(format!("nextdl {expected}").into_bytes())?;
                            expected += 1;
                            progress = Instant::now();
                            if block.data.is_empty() {
                                storage.finish().map_err(ClientError::Download)?;
                                exchange.send()?;
                                exchange.send()?;
                                complete = true;
                                break;
                            }
                        }
                        command => return Err(ClientError::UnexpectedCommand(command)),
                    }
                }
                if complete {
                    break;
                }
            }
        }
        // cl_main.cpp:1342-1353: filesystem restart, donedl, wait for gamestate.
        exchange.queue(b"donedl".to_vec())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            storage.check().map_err(ClientError::Download)?;
            exchange.send()?;
            let Some(message) = exchange.receive()? else {
                continue;
            };
            if let Ok(initial) = jkr_protocol::decode_initial_gamestate(&message.payload) {
                if !storage
                    .missing(&initial.game_state)
                    .map_err(ClientError::Download)?
                    .is_empty()
                {
                    return Err(ClientError::Download(
                        "server changed required paks during download; reconnect to retry".into(),
                    ));
                }
                exchange.acknowledge(initial.reliable_acknowledge);
                for command in &initial.server_commands {
                    exchange
                        .connection
                        .record_server_command(command.sequence, &command.command);
                }
                return Ok(message);
            }
            let mut reader = MessageReader::new(&message.payload);
            exchange.acknowledge(reader.read_i32()?);
            loop {
                match reader.read_service_command()? {
                    ServiceCommand::End => break,
                    ServiceCommand::Nop => {}
                    ServiceCommand::ServerCommand => exchange.server_command(&mut reader)?,
                    ServiceCommand::SetGame => {
                        reader.read_c_string(256)?;
                    }
                    ServiceCommand::Snapshot => {
                        skip_snapshot(&mut reader, &message, &initial.game_state)?;
                    }
                    // A late terminator is harmless; never acknowledge it twice.
                    ServiceCommand::Download => {
                        block(&mut reader, false)?;
                    }
                    command => return Err(ClientError::UnexpectedCommand(command)),
                }
            }
        }
        Err(ClientError::Download("no gamestate after donedl".into()))
    })();
    if result.is_err() {
        storage.abort();
        let _ = exchange.queue(b"stopdl".to_vec());
        let _ = exchange.send();
    }
    result
}

/// One pak over HTTP into `storage` (begun already), the channel kept alive meanwhile:
/// every second the pending commands go out and what the server sent is read, as the
/// reference's client keeps sending packets while its HTTP download runs.
fn http_fetch<T: Transport>(
    exchange: &mut Exchange<'_, T>,
    base: &str,
    request: &PakRequest,
    storage: &mut dyn DownloadStorage,
    game: &GameState,
) -> Result<(), String> {
    use crate::http_download::Fetched;
    use std::sync::mpsc::RecvTimeoutError;
    let fetched = crate::http_download::fetch(base, request);
    let mut kept_alive = Instant::now();
    loop {
        storage.check()?;
        match fetched.recv_timeout(Duration::from_millis(100)) {
            Ok(Fetched::Size(size)) => storage.size(size)?,
            Ok(Fetched::Data(data)) => storage.append(&data)?,
            Ok(Fetched::Done) => return storage.finish(),
            Ok(Fetched::Failed(error)) => return Err(error),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err("the fetch stopped".into()),
        }
        if kept_alive.elapsed() >= Duration::from_secs(1) {
            kept_alive = Instant::now();
            keep_alive(exchange, game).map_err(|error| error.to_string())?;
        }
    }
}

/// Send what is pending and read one message, taking its reliable commands.
fn keep_alive<T: Transport>(
    exchange: &mut Exchange<'_, T>,
    game: &GameState,
) -> Result<(), ClientError> {
    exchange.send()?;
    let Some(message) = exchange.receive()? else {
        return Ok(());
    };
    let mut reader = MessageReader::new(&message.payload);
    exchange.acknowledge(reader.read_i32()?);
    loop {
        match reader.read_service_command()? {
            ServiceCommand::End => return Ok(()),
            ServiceCommand::Nop => {}
            ServiceCommand::ServerCommand => exchange.server_command(&mut reader)?,
            ServiceCommand::SetGame => {
                reader.read_c_string(256)?;
            }
            ServiceCommand::Snapshot => skip_snapshot(&mut reader, &message, game)?,
            ServiceCommand::Download => {
                block(&mut reader, false)?;
            }
            command => return Err(ClientError::UnexpectedCommand(command)),
        }
    }
}

// SV_SendClientSnapshot writes a full, NOT_ACTIVE snapshot before download blocks
// (`sv_snapshot.cpp:142-148,838-848`). Reuse the production decoder, not a second codec.
fn skip_snapshot(
    reader: &mut MessageReader<'_>,
    message: &ServerMessage,
    game: &GameState,
) -> Result<(), ClientError> {
    let snapshot = jkr_protocol::decode_base_snapshot(&message.payload, message.sequence, game)?;
    while reader.bit_position() < snapshot.consumed_bits {
        reader.read_bits(1)?;
    }
    Ok(())
}

struct Exchange<'a, T> {
    connection: &'a mut T,
    sequence: &'a mut i32,
    pending: &'a mut VecDeque<(i32, Vec<u8>)>,
    server_id: i32,
    message_ack: i32,
    reliable_ack: i32,
}

impl<T: Transport> Exchange<'_, T> {
    fn queue(&mut self, command: Vec<u8>) -> Result<(), ClientError> {
        if self.pending.len() >= 64 {
            return Err(ClientError::ReliableCommandOverflow);
        }
        *self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(ClientError::ReliableCommandOverflow)?;
        self.pending.push_back((*self.sequence, command));
        Ok(())
    }

    fn acknowledge(&mut self, ack: i32) {
        self.pending
            .retain(|(sequence, _)| *sequence > ack.min(*self.sequence));
    }

    fn send(&mut self) -> Result<(), ClientError> {
        let commands: Vec<_> = self
            .pending
            .iter()
            .map(|(n, s)| (*n, s.as_slice()))
            .collect();
        self.connection.send_reliable_commands(
            self.server_id,
            self.message_ack,
            self.reliable_ack,
            &commands,
        )?;
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ServerMessage>, ClientError> {
        match self
            .connection
            .receive_server_message(Duration::from_millis(100))
        {
            Ok(message) => {
                self.message_ack = message.sequence;
                Ok(Some(message))
            }
            Err(NetworkError::TimedOut(_)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn server_command(&mut self, reader: &mut MessageReader<'_>) -> Result<(), ClientError> {
        let sequence = reader.read_i32()?;
        let text = reader.read_c_string(8192)?;
        self.connection.record_server_command(sequence, &text);
        self.reliable_ack = self.reliable_ack.max(sequence);
        if text.starts_with(b"disconnect") {
            return Err(ClientError::Download(
                String::from_utf8_lossy(&text).into_owned(),
            ));
        }
        if text.starts_with(b"print ") {
            eprintln!("download: {}", String::from_utf8_lossy(&text));
        }
        Ok(())
    }
}

/// Narrow transport seam for deterministic stock-format transcripts without sockets.
pub(crate) trait Transport {
    /// Retain reliable server text before acknowledging its sequence (packet XOR key).
    fn record_server_command(&mut self, sequence: i32, text: &[u8]);
    /// Send stock reliable commands without movement while CS_PRIMED.
    fn send_reliable_commands(
        &mut self,
        server: i32,
        message: i32,
        reliable: i32,
        commands: &[(i32, &[u8])],
    ) -> Result<(), NetworkError>;
    /// Receive a decoded netchan payload or a bounded timeout.
    fn receive_server_message(&mut self, timeout: Duration) -> Result<ServerMessage, NetworkError>;
    /// The server's address, for its `getinfo` (HTTP downloads); `None` without one.
    fn server_address(&self) -> Option<std::net::SocketAddr> {
        None
    }
}

impl Transport for LegacyConnection {
    fn server_address(&self) -> Option<std::net::SocketAddr> {
        Some(self.server())
    }

    fn record_server_command(&mut self, sequence: i32, text: &[u8]) {
        LegacyConnection::record_server_command(self, sequence, text);
    }

    fn send_reliable_commands(
        &mut self,
        server: i32,
        message: i32,
        reliable: i32,
        commands: &[(i32, &[u8])],
    ) -> Result<(), NetworkError> {
        LegacyConnection::send_reliable_commands(self, server, message, reliable, commands)
    }

    fn receive_server_message(&mut self, timeout: Duration) -> Result<ServerMessage, NetworkError> {
        LegacyConnection::receive_server_message(self, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// A channel that sends into nothing and hears nothing.
    struct Silent {
        sends: usize,
    }

    impl Transport for Silent {
        fn record_server_command(&mut self, _: i32, _: &[u8]) {}
        fn send_reliable_commands(
            &mut self,
            _: i32,
            _: i32,
            _: i32,
            _: &[(i32, &[u8])],
        ) -> Result<(), NetworkError> {
            self.sends += 1;
            Ok(())
        }
        fn receive_server_message(&mut self, _: Duration) -> Result<ServerMessage, NetworkError> {
            Err(NetworkError::TimedOut("test"))
        }
    }

    /// What a download put where.
    #[derive(Default)]
    struct Kept {
        size: Option<u64>,
        bytes: Vec<u8>,
        finished: bool,
    }

    impl DownloadStorage for Kept {
        fn feedback(
            &mut self,
            _: std::sync::mpsc::SyncSender<String>,
            _: std::sync::Arc<std::sync::atomic::AtomicBool>,
        ) {
        }
        fn missing(&self, _: &GameState) -> Result<Vec<PakRequest>, String> {
            Ok(Vec::new())
        }
        fn begin(&mut self, _: &PakRequest) -> Result<(), String> {
            Ok(())
        }
        fn size(&mut self, bytes: u64) -> Result<(), String> {
            self.size = Some(bytes);
            Ok(())
        }
        fn append(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }
        fn finish(&mut self) -> Result<(), String> {
            self.finished = true;
            Ok(())
        }
        fn abort(&mut self) {}
    }

    /// A pak served over HTTP after a pause long enough for the channel to be kept alive.
    #[test]
    fn a_pak_comes_over_http_with_the_channel_kept_alive() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap() == 1 {
                request.push(byte[0]);
            }
            assert!(request.starts_with(b"GET /base/x.pk3 HTTP/1.1\r\n"));
            std::thread::sleep(Duration::from_millis(1_300));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nPK\x03\x04")
                .unwrap();
        });
        let (mut connection, mut sequence, mut pending) = (Silent { sends: 0 }, 0, VecDeque::new());
        let mut exchange = Exchange {
            connection: &mut connection,
            sequence: &mut sequence,
            pending: &mut pending,
            server_id: 1,
            message_ack: 0,
            reliable_ack: 0,
        };
        let request = PakRequest {
            remote: "base/x.pk3".into(),
            filename: "x.pk3".into(),
            checksum: 0,
        };
        let mut storage = Kept::default();
        http_fetch(
            &mut exchange,
            &format!("http://127.0.0.1:{port}"),
            &request,
            &mut storage,
            &GameState::empty_local(0),
        )
        .unwrap();
        assert_eq!(
            (storage.size, storage.bytes.as_slice(), storage.finished),
            (Some(4), &b"PK\x03\x04"[..], true)
        );
        assert!(connection.sends >= 1, "the channel was kept alive");
    }
}
