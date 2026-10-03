//! Full local gameplay authority for a retained world, independent of the remote join.
use jkr_client::LocalSimulation;
use jkr_dedicated::bridge::{Identity, NativeGame};
use jkr_network::{LegacyGameHost, LegacyGameOutput, LocalSnapshotBuffer};
use jkr_protocol::{ConfigStringDirty, GameState, Snapshot, UserCommand};

pub(super) struct Prepared {
    game: NativeGame,
}
impl Prepared {
    pub(super) fn new(
        vfs: &jkr_vfs::VirtualFileSystem,
        map: &str,
        source: &GameState,
    ) -> Result<Self, String> {
        let name = map
            .strip_prefix("maps/")
            .unwrap_or(map)
            .trim_end_matches(".bsp");
        let loaded = jkr_dedicated::map::load_from(vfs.clone(), name).map_err(|e| e.to_string())?;
        let mut game = NativeGame::new(
            Identity {
                hostname: b"Local continuation".to_vec(),
                mapname: name.as_bytes().to_vec(),
            },
            Some(loaded),
            1,
            32,
        )
        .map_err(|e| e.to_string())?;
        let mut userinfo = jkr_network::LegacyUserInfo::with_name("local");
        published_appearance(source, source.client_num as usize, &mut userinfo);
        game.precache_local_player(source.client_num as usize, &local_userinfo(&userinfo)?)?;
        Ok(Self { game })
    }

    pub(super) fn activate(
        mut self,
        mut snapshot: Snapshot,
        userinfo: &jkr_network::LegacyUserInfo,
    ) -> Result<(GameState, Snapshot, Box<dyn LocalSimulation>), String> {
        let client = usize::from(snapshot.player.client_num());
        let info = local_userinfo(userinfo)?;
        self.game
            .client_connect(client, &info)
            .map_err(|e| String::from_utf8_lossy(&e).into_owned())?;
        self.game
            .enter_world(client, &UserCommand::default(), snapshot.server_time);
        self.game
            .client_command(client, b"team free", snapshot.server_time);
        self.game
            .resume_local_player(&snapshot.player, snapshot.server_time)?;
        self.game.resume_local_movers(&snapshot);
        self.game.take_output(&mut |_| {});
        let mut game = GameState::empty_local(client as i32);
        for (index, value) in self.game.config_strings() {
            game.replace_config_string(index, value.to_vec())
                .map_err(|e| e.to_string())?;
        }
        snapshot.entities.clear();
        snapshot
            .entities
            .reserve(jkr_network::LEGACY_SNAPSHOT_ENTITIES);
        snapshot.area_mask.reserve(32);
        snapshot.server_commands.clear();
        snapshot.delta_from = None;
        snapshot.flags = 0;
        let mut authority = Authority {
            game: self.game,
            client,
            time: snapshot.server_time,
            next_frame: snapshot.server_time.saturating_add(50),
            projection: LocalSnapshotBuffer::new(),
            pending: false,
        };
        authority
            .projection
            .capture(&authority.game, client, &mut snapshot);
        Ok((game, snapshot, Box::new(authority)))
    }
}

struct Authority {
    game: NativeGame,
    client: usize,
    time: i32,
    next_frame: i32,
    projection: LocalSnapshotBuffer,
    pending: bool,
}
impl LocalSimulation for Authority {
    fn command(&mut self, command: &UserCommand) {
        self.time = command.server_time.max(self.time);
        self.game.client_think(self.client, command, self.time);
        if self.time >= self.next_frame {
            self.game.run_frame(self.time);
            self.next_frame = self.time.saturating_add(50);
        }
        self.pending = true;
    }
    fn reliable(&mut self, command: &[u8]) {
        if let Some(info) = command
            .strip_prefix(b"userinfo \"")
            .and_then(|s| s.strip_suffix(b"\""))
        {
            self.game.client_userinfo_changed(self.client, info);
        } else {
            self.game.client_command(self.client, command, self.time);
        }
        self.pending = true;
    }
    fn receive(
        &mut self,
        game: &mut GameState,
        snapshot: &mut Snapshot,
        dirty: &mut ConfigStringDirty,
    ) -> bool {
        if !std::mem::take(&mut self.pending) {
            return false;
        }
        snapshot.server_commands.clear();
        self.game.take_output(&mut |out| {
            if let LegacyGameOutput::ServerCommand { client, text } = out
                && client.is_none_or(|client| client == self.client)
            {
                snapshot
                    .server_commands
                    .push(jkr_protocol::ReliableServerCommand {
                        sequence: 0,
                        command: text.to_vec(),
                    });
            }
            if let LegacyGameOutput::ConfigString { index, value, .. } = out {
                if game.config_string(index) != Some(value)
                    && game
                        .replace_config_string(index, value.to_vec())
                        .unwrap_or(false)
                {
                    dirty.mark(index);
                }
            }
        });
        snapshot.message_sequence = snapshot.message_sequence.wrapping_add(1);
        snapshot.server_time = self.time;
        self.projection.capture(&self.game, self.client, snapshot);
        true
    }
}

impl crate::GpuState {
    pub(super) fn start_local_game_continuation(&mut self) -> bool {
        let Some(remote) = self.resident.session.as_mut() else {
            return false;
        };
        let source = remote.take_retired_world().or_else(|| {
            self.resident.intermission.then(|| {
                (
                    remote.game_state().clone(),
                    remote.latest_snapshot().clone(),
                )
            })
        });
        let Some((game, mut snapshot)) = source else {
            return false;
        };
        if !crate::live_session::active_snapshot(&snapshot)
            && let Some(player) = &self.resident.last_playing
        {
            snapshot.player.copy_from(player);
        }
        let mut userinfo = match self.console.as_ref().and_then(|c| c.userinfo().ok()) {
            Some(info) => info,
            None => return false,
        };
        published_appearance(
            &game,
            usize::from(snapshot.player.client_num()),
            &mut userinfo,
        );
        userinfo.password = None;
        userinfo.guid = None;
        self.resident.scenery = Some((game, snapshot.clone()));
        let Some(prepared) = self.resident.prepared_game.take() else {
            return false;
        };

        if snapshot.player.movement_type() == jkr_client::PM_INTERMISSION {
            if let Some(player) = &self.resident.last_playing {
                snapshot.player.copy_from(player);
            }
        }
        if let Some(predicted) = self.local_prediction.predicted_state() {
            predicted.write_player_state(&mut snapshot.player);
        }
        // Keep the already displayed pose and the new authority on one timeline.
        // Starting from the last packet can rewind prediction by several frames.
        let now = std::time::Instant::now();
        snapshot.server_time = snapshot
            .server_time
            .max(snapshot.player.command_time())
            .max(
                self.presentation_clock
                    .sample(now)
                    .clamp(0, i64::from(i32::MAX)) as i32,
            );
        let result = prepared.activate(snapshot, &userinfo);
        let (game, snapshot, simulation) = match result {
            Ok(ready) => ready,
            Err(error) => {
                crate::log::progress(format_args!("local continuation failed: {error}"));
                return false;
            }
        };
        let local = remote.local_continuation(game, snapshot, simulation);
        let local_id =
            jkr_runtime::EntityId::new(u64::from(local.latest_snapshot().player.client_num()) + 1);
        let retired: Vec<_> = self
            .live_world
            .entities()
            .filter(|entity| entity.id != local_id)
            .map(|entity| entity.id)
            .collect();
        for id in retired {
            self.live_world.remove(id);
        }
        self.local_prediction = crate::LocalPrediction::new(
            Some(local.latest_snapshot()),
            self.actor_meshes.first().map(|m| m.preview.config.as_ref()),
            Some(local.game_state()),
            self.vfs.as_ref().expect("resident VFS"),
        );
        let mut adapter = jkr_client::LegacyWorldAdapter::new(self.live_world.id());
        adapter.apply_snapshot(
            local.latest_snapshot(),
            local.game_state(),
            &mut self.live_world,
        );
        self.legacy_world_adapter = Some(adapter);
        let now = std::time::Instant::now();
        self.network_command_due = now;
        self.server_clock = jkr_client::ServerClock::new(local.latest_snapshot().server_time, now);
        self.presentation_clock
            .follow_local(local.latest_snapshot().server_time, now);
        self.live_session = Some(local);
        self.live_map_installed = true;
        self.resident.local_game = true;
        self.resident.attached = true;
        crate::log::progress(format_args!("resident world: full local authority ready"));
        true
    }
}

pub(super) fn recover(session: &mut jkr_client::ClientSession) -> Option<Prepared> {
    let local: Box<dyn std::any::Any + Send> = session.take_local_authority()?;
    let authority = local.downcast::<Authority>().ok()?;
    let Authority {
        mut game,
        client,
        time,
        ..
    } = *authority;
    game.client_disconnect(client);
    game.reset_local_world(time);
    Some(Prepared { game })
}

fn local_userinfo(userinfo: &jkr_network::LegacyUserInfo) -> Result<Vec<u8>, String> {
    let mut info =
        jkr_network::legacy_userinfo_payload_with_extensions(userinfo, Default::default())
            .map_err(|e| e.to_string())?;
    info.push_str("\\ip\\127.0.0.1");
    Ok(info.into_bytes())
}

fn published_appearance(
    game: &GameState,
    client: usize,
    userinfo: &mut jkr_network::LegacyUserInfo,
) {
    if let Some(info) = game
        .config_string(1131 + client)
        .and_then(|raw| std::str::from_utf8(raw).ok())
        .and_then(|text| jkr_protocol::InfoString::parse(text).ok())
    {
        for (key, slot) in [
            ("model", &mut userinfo.model),
            ("st", &mut userinfo.saber1),
            ("st2", &mut userinfo.saber2),
        ] {
            if let Some(value) = info.get(key) {
                *slot = value.to_owned();
            }
        }
        if let Some(color) = info.get("c1").and_then(|v| v.parse().ok()) {
            userinfo.color1 = color;
        }
        if let Some(color) = info.get("c2").and_then(|v| v.parse().ok()) {
            userinfo.color2 = color;
        }
    }
}
