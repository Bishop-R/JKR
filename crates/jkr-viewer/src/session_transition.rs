//! Viewer reaction to typed client-session lifecycle events.
//!
//! Protocol parsing remains in `jkr-client`; this module owns the expensive,
//! one-shot world teardown/reload and has no steady-state frame cost.

use super::{GpuState, GpuWorldInput, assets};
use jkr_client::{ServerClock, SessionTransition, SessionTransitionKind};
use jkr_protocol::InfoString;
use std::error::Error;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Instant;

/// Result of applying a client lifecycle event to the viewer shell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Reaction {
    ReloadWorld,
    ReturnToMenu,
}

pub(crate) fn reaction(transition: &SessionTransition) -> Reaction {
    match transition.kind {
        SessionTransitionKind::NewMap
        | SessionTransitionKind::SameMapGamestate
        | SessionTransitionKind::MapRestart => Reaction::ReloadWorld,
        SessionTransitionKind::Disconnected => Reaction::ReturnToMenu,
    }
}

/// Aggregate the normally-empty transition queue without allocating.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Reactions {
    /// A map transition needs the existing world reload path.
    pub(crate) reload_world: bool,
    /// A disconnection needs the connection-failed screen with this reason.
    pub(crate) disconnect_reason: Option<String>,
}

/// Preserve lifecycle reasons while leaving map transition handling unchanged.
pub(crate) fn consume<'a>(transitions: impl Iterator<Item = SessionTransition> + 'a) -> Reactions {
    let mut result = Reactions::default();
    for transition in transitions {
        crate::log::progress(format_args!(
            concat!(
                "session transition: {:?} {:?} -> {:?}, serverId={}, ",
                "gamestate={:?}, firstSnapshot={:?}, reason={:?}"
            ),
            transition.kind,
            transition.old_map,
            transition.new_map,
            transition.server_id,
            transition.gamestate_message_sequence,
            transition.first_snapshot_message_sequence,
            transition.reason,
        ));
        match reaction(&transition) {
            Reaction::ReloadWorld => result.reload_world = true,
            Reaction::ReturnToMenu => {
                result.disconnect_reason = Some(disconnect_reason(transition.reason));
            }
        }
    }
    result
}

fn disconnect_reason(reason: Option<String>) -> String {
    reason.unwrap_or_else(|| "Server closed the connection".to_owned())
}

/// Surface a server or netchan failure in stderr, scrollback, and the hero screen.
pub(crate) fn show_disconnect(
    reason: String,
    console: Option<&mut crate::console::ViewerConsole>,
    menu: Option<&mut crate::menu::ClientMenu>,
) {
    crate::log::progress(format_args!("Disconnected: {reason}"));
    if let Some(console) = console {
        console.push_log(format!("^1Disconnected: {reason}"));
    }
    if let Some(menu) = menu {
        menu.join_failed(reason);
    }
}

#[path = "world_load.rs"]
mod load;
pub(crate) use load::{WorldLoadPoll, WorldLoadTask};

/// One complete replacement world built against the retained GPU context.
pub(crate) struct WorldInstallTask {
    receiver: Receiver<Result<GpuState, String>>,
    /// The worker's answer once it has arrived, until `poll` hands it out.
    result: Option<Result<Box<GpuState>, String>>,
    pub(crate) map_path: String,
    pub(crate) started: Instant,
}

pub(crate) enum WorldInstallPoll {
    Pending,
    Ready(GpuState),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoadStage {
    Idle,
    Preparing(u64),
    Installing(u64),
    Failed(u64),
}

/// Ordered, cancellable state for CPU preparation followed by background GPU install.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LoadStateMachine {
    generation: u64,
    stage: LoadStage,
}

impl LoadStateMachine {
    pub(crate) const fn new() -> Self {
        Self {
            generation: 0,
            stage: LoadStage::Idle,
        }
    }

    fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.stage = LoadStage::Preparing(self.generation);
        self.generation
    }

    fn prepared(&mut self) {
        debug_assert_eq!(self.stage, LoadStage::Preparing(self.generation));
        self.stage = LoadStage::Installing(self.generation);
    }

    fn installed(&mut self) {
        debug_assert_eq!(self.stage, LoadStage::Installing(self.generation));
        self.stage = LoadStage::Idle;
    }

    fn failed(&mut self) -> Reaction {
        self.stage = LoadStage::Failed(self.generation);
        Reaction::ReturnToMenu
    }
}

/// Stack reserved for the world-install worker. See the comment at its spawn.
pub(crate) const WORLD_INSTALL_STACK_BYTES: usize = 64 * 1024 * 1024;

impl WorldInstallTask {
    pub(crate) fn start(
        context: Arc<super::gpu_context::Context>,
        size: [u32; 2],
        input: GpuWorldInput,
        map_path: String,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let started = Instant::now();
        thread::Builder::new()
            .name("jkr-world-install".into())
            // `new_with_context` is async, so `block_on` materialises the whole
            // world-construction future — every intermediate it holds across an
            // await — as a single frame on this thread. That outgrew the 2 MiB
            // a spawned thread gets by default and aborted the process on join
            // ("thread 'jkr-world-install' has overflowed its stack"). The main
            // thread never hit it because it starts with 8 MiB. Reserve room
            // explicitly rather than depending on how much the future happens
            // to need this month; untouched pages cost nothing.
            .stack_size(WORLD_INSTALL_STACK_BYTES)
            .spawn(move || {
                let result = pollster::block_on(GpuState::new_with_context(context, size, input))
                    .map_err(|error| error.to_string());
                let _ = sender.send(result);
            })
            .expect("world install thread creation failed");
        Self {
            receiver,
            result: None,
            map_path,
            started,
        }
    }

    /// Whether the worker has answered (with a world or an error), without
    /// taking the answer.
    pub(crate) fn settled(&mut self) -> bool {
        if self.result.is_none() {
            self.result = match self.receiver.try_recv() {
                Ok(result) => Some(result.map(Box::new)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("world installer stopped unexpectedly".into()))
                }
            };
        }
        self.result.is_some()
    }

    pub(crate) fn poll(&mut self) -> WorldInstallPoll {
        if !self.settled() {
            return WorldInstallPoll::Pending;
        }
        match self.result.take() {
            Some(Ok(world)) => WorldInstallPoll::Ready(*world),
            Some(Err(error)) => WorldInstallPoll::Failed(error),
            None => WorldInstallPoll::Pending,
        }
    }
}

impl GpuState {
    /// Whether nothing of a join is still in flight: no connection, parse or
    /// GPU install pending, and any install's answer already in hand. The
    /// gate glide waits for this so the cut can follow the crossing at once.
    pub(crate) fn world_settled(&mut self) -> bool {
        self.join_task.is_none()
            && !self.pending_map_reload
            && self.world_load_task.is_none()
            && self
                .world_install_task
                .as_mut()
                .is_none_or(WorldInstallTask::settled)
    }

    pub(crate) fn poll_world_install(&mut self) -> Result<Option<GpuState>, Box<dyn Error>> {
        if self.pending_map_reload {
            self.world_load_state.begin();
            self.load_event_gap.start(Instant::now());
            self.world_load_task = None;
            self.world_install_task = None;
            let map_path = self.pending_map_path()?;
            self.world_load_started = Some(Instant::now());
            self.world_load_map.clear();
            self.world_load_map.push_str(&map_path);
            crate::log::progress(format_args!(
                "session transition: loading {map_path} on worker"
            ));
            if let Some(menu) = &mut self.client_menu {
                menu.loading_map(
                    map_path
                        .trim_start_matches("maps/")
                        .trim_end_matches(".bsp"),
                );
            }
            let game = self
                .live_session
                .as_ref()
                .map(jkr_client::ClientSession::game_state)
                .or_else(|| self.demo_session.as_ref().map(|s| s.game_state()))
                .ok_or("map transition lost its session")?;
            let selection = assets::session_content::Selection::from_game(game)?;
            self.world_load_task = Some(WorldLoadTask::start_session(
                self.game_data.clone(),
                map_path,
                selection,
            ));
            self.pending_map_reload = false;
        }
        if self.world_install_task.is_some() {
            if self.holds_world_install(Instant::now()) {
                return Ok(None);
            }
            let task = self.world_install_task.as_mut().expect("checked above");
            return match task.poll() {
                WorldInstallPoll::Pending => Ok(None),
                WorldInstallPoll::Failed(error) => Err(error.into()),
                WorldInstallPoll::Ready(world) => {
                    self.world_load_state.installed();
                    crate::log::progress(format_args!(
                        "session transition: installed {} in {:.1} ms",
                        task.map_path,
                        task.started.elapsed().as_secs_f64() * 1_000.0
                    ));
                    self.world_install_task = None;
                    Ok(Some(world))
                }
            };
        }
        let Some(task) = &self.world_load_task else {
            return Ok(None);
        };
        let loaded = match task.poll() {
            WorldLoadPoll::Pending => return Ok(None),
            WorldLoadPoll::Failed(error) => return Err(error.into()),
            WorldLoadPoll::Ready(loaded) => loaded,
        };
        self.world_load_task = None;
        self.world_load_state.prepared();
        if let Some(timeline) = &mut self.connect_timeline {
            timeline.mark(crate::log::TimelinePhase::MapLoaded);
        }
        crate::log::progress(format_args!(
            "session transition: parsed {} in {:.1} ms",
            loaded.map_path,
            loaded.elapsed.as_secs_f64() * 1_000.0
        ));
        let game_state = self
            .live_session
            .as_ref()
            .map(jkr_client::ClientSession::game_state)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(|session| session.game_state())
            })
            .ok_or("map transition lost its session")?;
        let snapshot = self
            .live_session
            .as_ref()
            .map(jkr_client::ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(|session| session.latest_snapshot())
            })
            .ok_or("map transition lost its snapshot")?;
        let player = &snapshot.player;
        let intermission = jkr_client::IntermissionView::from_player_state(player);
        let mut camera_origin = intermission.map_or_else(|| player.origin(), |view| view.origin);
        if intermission.is_none() {
            camera_origin[2] += player.view_height() as f32;
        }
        let camera_yaw = player.view_angles()[1].to_radians();
        let world_bounds = loaded.bsp.render().models()[0].clone();
        let input = GpuWorldInput {
            scene: loaded.scene,
            bsp: loaded.bsp,

            vfs: loaded.vfs,
            shaders: loaded.shaders,
            world_minimums: world_bounds.minimums,
            world_maximums: world_bounds.maximums,
            camera_origin,
            camera_yaw,
            player_preview: None,
            build_game_state: Some(game_state.clone()),
            build_snapshot: Some(snapshot.clone()),
            live_session: None,
            demo_session: None,
            console: None,
            client_menu: None,
            game_data: self.game_data.clone(),
            connect_timeline: self.connect_timeline.clone(),

            completed_map_changes: self.completed_map_changes,
        };
        self.world_install_task = Some(WorldInstallTask::start(
            Arc::clone(&self.context),
            [self.size.width, self.size.height],
            input,
            loaded.map_path,
        ));
        Ok(None)
    }

    /// Move everything that belongs to the player's session with the client
    /// — sessions, console, menu, pointer state — from this world into `to`.
    pub(crate) fn hand_shell_to(&mut self, to: &mut GpuState) {
        debug_assert_eq!(self.context.id, to.context.id);
        let now = Instant::now();
        to.live_session = self.live_session.take();
        to.demo_session = self.demo_session.take();
        to.console = self.console.take();
        to.net_timing = std::mem::take(&mut self.net_timing);
        to.client_menu = self.client_menu.take();
        to.join_task = self.join_task.take();
        to.last_connect_address = self.last_connect_address.take();
        to.pointer_captured = self.pointer_captured;
        to.cursor_policy = std::mem::replace(
            &mut self.cursor_policy,
            super::pointer_input::CursorPolicy::new(),
        );
        to.cursor_position = self.cursor_position;
        to.applied_display = self.applied_display;
        to.applied_resolution = self.applied_resolution;
        // The clock belongs to the connection, not to the world: a fresh one
        // per map would forget the highest stamp already sent and could
        // re-anchor behind it (`ServerClock`). It arrives restarted, so the
        // new world's first snapshot anchors it.
        to.server_clock = std::mem::replace(&mut self.server_clock, ServerClock::unanchored(now));
        to.server_clock.activate();
        to.load_event_gap = self.load_event_gap.clone();
        to.completed_map_changes = self.completed_map_changes;
        to.gameplay_input.clear();
    }

    pub(crate) fn adopt_world(&mut self, mut loaded: GpuState) -> GpuState {
        self.hand_shell_to(&mut loaded);
        loaded.transition_report_pending = self.connect_timeline.is_none();
        loaded.completed_map_changes = self
            .completed_map_changes
            .saturating_add(u32::from(self.live_map_installed));
        loaded.live_map_installed = true;
        loaded.world_load_state = self.world_load_state;
        loaded.world_load_started = self.world_load_started;
        loaded.world_load_map.clear();
        loaded.world_load_map.push_str(&self.world_load_map);
        loaded
    }

    pub(crate) fn fail_world_install(&mut self, error: String) {
        let reaction = self.world_load_state.failed();
        debug_assert_eq!(reaction, Reaction::ReturnToMenu);
        self.world_load_task = None;
        self.world_install_task = None;
        self.live_session = None;
        self.demo_session = None;
        self.gameplay_input.clear();
        if let Some(menu) = &mut self.client_menu {
            menu.join_failed(error);
        }
    }

    fn pending_map_path(&self) -> Result<String, Box<dyn Error>> {
        let game_state = self
            .live_session
            .as_ref()
            .map(jkr_client::ClientSession::game_state)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(|session| session.game_state())
            })
            .ok_or("map transition lost its session")?;
        let server_info = game_state
            .config_string(0)
            .ok_or("new gamestate has no CS_SERVERINFO")?;
        let server_info = InfoString::parse(std::str::from_utf8(server_info)?)?;
        let map_name = server_info
            .get("mapname")
            .ok_or("new CS_SERVERINFO has no mapname")?;
        Ok(format!("maps/{map_name}.bsp"))
    }
}
