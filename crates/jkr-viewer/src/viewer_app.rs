//! The winit application: owns the window's [`GpuState`], swaps in freshly
//! installed server worlds and keeps the menu world parked meanwhile.

use super::*;

pub(super) struct ViewerApplication {
    pub(super) scene: Option<StaticWorld>,
    pub(super) bsp: Option<Bsp>,

    pub(super) vfs: Option<Arc<VirtualFileSystem>>,
    pub(super) shaders: Option<ShaderCatalog>,
    pub(super) world_minimums: [f32; 3],
    pub(super) world_maximums: [f32; 3],
    pub(super) camera_origin: [f32; 3],
    pub(super) camera_yaw: f32,
    pub(super) player_preview: Option<PlayerPreview>,
    pub(super) live_session: Option<ClientSession>,
    pub(super) demo_session: Option<demo_playback::Session>,
    pub(super) console: Option<console::ViewerConsole>,
    pub(super) client_menu: Option<menu::ClientMenu>,
    pub(super) game_data: PathBuf,
    pub(super) connect_timeline: Option<log::ConnectTimeline>,

    pub(super) completed_map_changes: u32,
    pub(super) game_audio: Option<GameAudio>,
    pub(super) initial_connect: Option<String>,
    pub(super) gpu: Option<GpuState>,
    pub(super) menu_world: menu_world::Parked,
}

impl ApplicationHandler for ViewerApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let title = if self
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.is_visible())
        {
            "JKR"
        } else if self.live_session.is_some() {
            "JKR live client"
        } else {
            "JKR world viewer"
        };
        let attributes = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(PhysicalSize::new(1280, 720));
        let result = (|| -> Result<GpuState, Box<dyn Error>> {
            let window = Arc::new(event_loop.create_window(attributes)?);
            let scene = self
                .scene
                .take()
                .ok_or("viewer scene was already consumed")?;
            let bsp = self.bsp.take().ok_or("viewer BSP was already consumed")?;
            let vfs = self.vfs.take().ok_or("viewer VFS was already consumed")?;
            let shaders = self
                .shaders
                .take()
                .ok_or("viewer shader catalog was already consumed")?;
            pollster::block_on(GpuState::new(
                window,
                GpuWorldInput {
                    scene,
                    bsp,

                    vfs,
                    shaders,
                    world_minimums: self.world_minimums,
                    world_maximums: self.world_maximums,
                    camera_origin: self.camera_origin,
                    camera_yaw: self.camera_yaw,
                    player_preview: self.player_preview.take(),
                    live_session: self.live_session.take(),
                    demo_session: self.demo_session.take(),
                    build_game_state: None,
                    build_snapshot: None,
                    console: self.console.take(),
                    client_menu: self.client_menu.take(),
                    game_data: self.game_data.clone(),
                    connect_timeline: self.connect_timeline.take(),

                    completed_map_changes: self.completed_map_changes,
                },
            ))
        })();
        match result {
            Ok(gpu) => {
                let mut gpu = gpu;
                gpu.is_menu_world = true;
                if self.game_audio.is_none()
                    && gpu
                        .console
                        .as_ref()
                        .and_then(|c| c.bool_cvar("s_initsound"))
                        != Some(false)
                {
                    self.game_audio = GameAudio::start();
                }
                gpu.configure_audio(&mut self.game_audio);
                if let Some(address) = self.initial_connect.take() {
                    gpu.begin_address_join(address);
                }
                self.gpu = Some(gpu);
            }
            Err(error) => {
                eprintln!("jkr-viewer: failed to initialize graphics: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(gpu) = self.gpu.as_mut().filter(|gpu| {
            gpu.window
                .as_ref()
                .is_some_and(|window| window.id() == window_id)
        }) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gpu.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Err(error) = gpu.rebuild_modern_text(scale_factor) {
                    eprintln!("failed to rebuild DPI text atlas: {error}");
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                if let Some(console) = gpu.console.as_mut() {
                    console.set_shift(modifiers.state().shift_key());
                    console.set_control(modifiers.state().control_key());
                    console.window_alt(modifiers.state().alt_key());
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if !gpu
                    .console
                    .as_mut()
                    .is_some_and(|console| console.window_key(&event))
                {
                    gpu.keyboard(event);
                }
            }
            WindowEvent::CursorMoved { position, .. } => gpu.pointer_moved(position),
            WindowEvent::CursorLeft { .. } => gpu.pointer_left(),
            WindowEvent::MouseInput { state, button, .. } => gpu.pointer_button(button, state),
            WindowEvent::MouseWheel { delta, .. } => gpu.pointer_wheel(delta),
            WindowEvent::Focused(focused) => {
                if let Some(console) = &mut gpu.console {
                    console.window_state(
                        Some(focused),
                        gpu.window.as_ref().and_then(|w| w.is_minimized()),
                    );
                }
                gpu.pointer_focus(focused);
            }
            WindowEvent::Occluded(_) => {
                if let Some(console) = &mut gpu.console {
                    console.window_state(None, gpu.window.as_ref().and_then(|w| w.is_minimized()));
                }
            }
            WindowEvent::RedrawRequested => {
                let frame_status = gpu.render(&mut self.game_audio);
                if frame_status == FrameStatus::Reconfigure {
                    gpu.resize(gpu.size);
                }
                match gpu.poll_world_install() {
                    Ok(Some(reloaded)) => {
                        let mut reloaded = gpu.adopt_world(reloaded);
                        log::progress(format_args!(
                            "session transition complete: map change {}; audio underruns={}",
                            reloaded.completed_map_changes,
                            self.game_audio
                                .as_ref()
                                .map_or(0, GameAudio::underrun_count)
                        ));

                        if reloaded.pointer_captured {
                            reloaded.capture_pointer();
                        }
                        if let Some(menu) = &mut reloaded.client_menu {
                            menu.joined();
                        }
                        reloaded.configure_audio(&mut self.game_audio);
                        reloaded.present_latest_live_snapshot(&mut self.game_audio);
                        self.menu_world.install(gpu, reloaded);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        log::progress(format_args!("jkr-viewer: map transition failed: {error}"));
                        gpu.fail_world_install(error.to_string());
                    }
                }
            }
            _ => {}
        }
        if self.menu_world.restore_if_idle(gpu) {
            gpu.configure_audio(&mut self.game_audio);
        }
        gpu.sync_cursor_policy();
        if gpu.quit_requested {
            event_loop.exit();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let (Some(gpu), DeviceEvent::MouseMotion { delta }) = (&mut self.gpu, event) {
            gpu.pointer_motion(delta);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(gpu) = &mut self.gpu {
            let now = Instant::now();
            let maximum_fps = gpu.maximum_fps();
            if maximum_fps == 0 || now >= gpu.frame_pacer.deadline() {
                event_loop.set_control_flow(ControlFlow::Poll);
                gpu.frame_pacer.begin_frame(now);
                gpu.window
                    .as_ref()
                    .expect("windowed renderer has a window")
                    .request_redraw();
            } else {
                event_loop.set_control_flow(crate::console::wait_control(
                    gpu.console.as_ref(),
                    gpu.frame_pacer.deadline(),
                ));
            }
        }
    }
}
