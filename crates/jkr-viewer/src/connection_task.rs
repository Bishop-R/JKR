//! Worker lifecycle and bounded download-progress delivery.

use super::*;

#[path = "connection_download.rs"]
mod downloads;

/// One nonblocking worker result, including bounded download progress.
pub(crate) enum JoinPoll {
    Pending,
    Progress(String),
    Joined(JoinedSession),
    Failed(String),
}

/// Session plus the bootstrap timings that must survive into first render.
pub(crate) struct JoinedSession {
    /// The same netchan, including when resuming a map-change download.
    pub(crate) session: Box<ClientSession>,
    /// Connection timings retained until the first rendered snapshot.
    pub(crate) timeline: ConnectTimeline,
    /// `forcepowers` value legalized against the server before joining.
    pub(crate) forcepowers: String,
}

/// At-most-one background connection so DNS/UDP timeouts never stall frames.
pub(crate) struct JoinTask {
    receiver: Receiver<Result<JoinedSession, String>>,
    progress: Receiver<String>,
    cancelled: Arc<AtomicBool>,
}

impl JoinTask {
    /// Resolve, profile, and join an address entirely off the event thread.
    pub(crate) fn start_address(
        address: String,
        game_data: PathBuf,
        userinfo: LegacyUserInfo,
        allow_download: bool,
        guid: Option<crate::client_guid::Policy>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (progress_tx, progress) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        thread::spawn(move || {
            let result = (|| {
                let storage = crate::assets::downloads::Store::open(
                    &game_data,
                    crate::assets::downloads::home()?,
                    allow_download,
                    progress_tx,
                    worker_cancel,
                )?;
                join_with_storage(
                    &address,
                    &game_data,
                    &userinfo,
                    Some(Box::new(storage)),
                    guid,
                )
                .map_err(|e| e.to_string())
            })()
            .map_err(|error| error.to_string());
            if let Err(undelivered) = sender.send(result)
                && let Ok(mut joined) = undelivered.0
            {
                let _ = joined.session.disconnect();
            }
        });
        Self {
            receiver,
            progress,
            cancelled,
        }
    }

    /// Poll without blocking the rendering thread.
    pub(crate) fn poll(&self) -> JoinPoll {
        if let Ok(text) = self.progress.try_recv() {
            return JoinPoll::Progress(text);
        }
        match self.receiver.try_recv() {
            Ok(Ok(session)) => JoinPoll::Joined(session),
            Ok(Err(error)) => JoinPoll::Failed(error),
            Err(TryRecvError::Empty) => JoinPoll::Pending,
            Err(TryRecvError::Disconnected) => {
                JoinPoll::Failed("connection worker stopped unexpectedly".to_owned())
            }
        }
    }
}
