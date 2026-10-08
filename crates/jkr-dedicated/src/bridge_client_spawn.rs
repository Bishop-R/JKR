//! Apply `ClientSpawn` state and reset the per-life server state.

use super::*;

impl NativeGame {
    /// What follows `ClientSpawn`'s state in the bridge, for a begin and a respawn
    /// alike: the player's string, what it is told, the flash, the spawn's own think with
    /// `command`, the frame's end for it, and the ranks.
    pub(super) fn spawned(
        &mut self,
        client: usize,
        begun: Begun,
        place: SpawnPlace,
        command: UserCommand,
        server_time: i32,
    ) {
        let (gravity, player_speed) = (self.gravity(), self.player_speed());
        let protection = self.cvars.integer(b"g_spawnInvulnerability");
        let inactivity = self.cvars.integer(b"g_inactivity");
        let legacy_fixes = self.legacy_fixes;
        let (parms, rules) = (self.saber_parms(), self.userinfo_rules());
        // The class a siege player spawns as (`ClientUserinfoChanged`'s, kept in `siegeClass`).
        let siege_class = if self.siege.is_some() {
            self.siege_class_of(client)
        } else {
            None
        };
        let duel_fraglimit = self.limits.duel_fraglimit;
        let lone_health = (
            self.cvars.integer(b"g_powerDuelStartHealth"),
            self.cvars.integer(b"g_powerDuelEndHealth"),
        );
        let starting_items = self.starting_items();
        let lag_icon = self.lag_icon;
        let gun_rung = self.gun_game_rung(client);
        let Self {
            server,
            world,
            players,
            map,
            told,
            obstacles,
            body_legs,
            gametype,
            pool,
            sounds,
            last_frame_time,
            ..
        } = self;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        let playing = peer.playing();
        let mut class_speed = 1.0;
        let mut class_sabers: Option<(Option<String>, Option<String>)> = None;
        (peer.state, peer.in_space, peer.no_corpse) = (begun.state, 0, false);
        // `ClientSpawn` clears `ent->flags` (`g_client.c:3372`): god mode and notarget
        // last one life.
        peer.riding.flags = 0;
        peer.switch_class_time = 0;
        // jaPRO's held emote lives in the client, which `ClientSpawn` clears.
        peer.emote = Default::default();
        // `ClientSpawn`'s siege branch: what the class hands its player.
        if let Some(class) = siege_class.as_ref() {
            class_speed = class.speed;
            class_sabers = Some((class.saber1.clone(), class.saber2.clone()));
            bridge_siege_clients::apply_siege_kit(&mut peer.state, class, playing);
        } else {
            // jaPRO's `GiveClientItems`: `g_startingItems`' holdables (none, as stock).
            super::bridge_starting_items::give_starting_items(&mut peer.state, starting_items);
        }
        jkr_game_jka::power_duel::spawned(
            &mut peer.state,
            &peer.session,
            *gametype,
            (duel_fraglimit, lone_health),
        );
        // `ClientSpawn` wipes the client: no longer a loser of the round, nor in noclip.
        (peer.loser, peer.noclip) = (false, false);
        peer.health = peer.state.health();
        peer.corpse = None;
        peer.saber = SaberFrame::default();
        peer.throw_memory = Default::default();
        // `ClientSpawn`'s `WP_SaberInitBladeData`: the same saber entity, back in hand.
        init_saber_entity(peer, pool, server_time);
        let levels = peer
            .session
            .force
            .as_ref()
            .map_or([0; jkr_game_jka::force_powers::NUM_FORCE_POWERS], |force| {
                force.levels
            });
        peer.force.respawned(levels, server_time);
        // jaPRO's gun game: the rung's fire mode, and the saber throw's level on its rung.
        if let Some(rung) = gun_rung {
            peer.gun_fire = rung.fire;
            if rung.saber_throw {
                peer.force.levels[jkr_game_jka::force_powers::FP_SABER_THROW] = 3;
            }
        }
        // `ClientSpawn` clears `legsAnimExecute` and `torsoAnimExecute` with the rest of
        // the client, so the spawn's animations are installed again over the old pose.
        if let Some(skeleton) = peer.skeleton.as_mut() {
            skeleton.forget_animations();
        }
        // `ClientSpawn` clears the client but its persistent data; its air lasts 12 s.
        (peer.wounds, peer.breath.air_out_time) = (
            Wounds::default(),
            server_time + jkr_game_jka::world_effects::AIR_TIME,
        );
        peer.saber_cut = Default::default();
        peer.kick = Default::default();
        peer.grapple = Default::default();
        peer.service = Default::default();
        peer.gear = Default::default();
        peer.time_residual = 0;
        peer.block_time = 0;
        peer.entity.spawned(place.angles);
        peer.inactivity = jkr_game_jka::inactivity::Inactivity::spawned(server_time, inactivity);
        // `ClientSpawn`: `respawnTime` is the spawn's time until the next death sets it.
        peer.mortality.respawn_time = server_time;
        peer.invulnerable_until = if playing && protection != 0 {
            server_time + protection
        } else {
            0
        };
        // The player's string names its team: `ClientSpawn` rewrites it when it changed.
        // A class's own sabers, or the default one and none (`g_client.c:2235-2240`),
        // written into the userinfo so that the next spawn's check keeps them.
        if let Some((first, second)) = class_sabers {
            let mut host = bridge_sabers::SaberHost {
                sounds,
                told,
                rng: &mut self.deaths.rng,
            };
            for (hand, name) in [
                (0, first.as_deref().unwrap_or("Kyle")),
                (1, second.as_deref().unwrap_or("none")),
            ] {
                if let Err(error) = peer.sabers.set(&parms, hand, name.as_bytes(), &mut host) {
                    eprintln!("client {client}: {error}; the class's saber is refused");
                }
            }
            peer.userinfo = bridge_sabers::with_saber_names(&peer.userinfo, &peer.sabers);
        }
        if let Ok(accepted) = judge(
            &bridge_sabers::with_saber_names(&peer.userinfo, &peer.sabers),
            rules,
            &peer.session,
            *gametype,
            &parms,
            siege_class.as_ref(),
        ) && accepted.client_info != peer.client_info
        {
            let previous = std::mem::replace(&mut peer.client_info, accepted.client_info.clone());
            told.push(Told::PlayerString {
                client,
                previous,
                value: accepted.client_info,
            });
        }
        told.extend(
            begun
                .commands
                .into_iter()
                .map(|text| Told::One(client, text)),
        );
        told.extend(begun.entered.map(Told::Everyone));
        for event in &begun.events {
            let _ = pool.spawn_temporary(event.state(), server_time, None);
        }
        // The spawn's own think, with the speed and gravity `ClientThink_real` and
        // `SpectatorThink` set before every move.
        // `ClientThink_real` (`g_active.c:2356-2363`): `g_speed`, then a siege class's
        // own multiplier — which is why some classes run and others lumber, and which a
        // client predicts from the class file it read itself.
        let (speed, gravity) = if playing {
            (player_speed * class_speed, gravity)
        } else {
            (400.0, 0.0)
        };
        peer.state.set_speed(speed);
        peer.state.set_base_speed(speed as i32);
        peer.state.set_gravity(gravity as i32);
        if !playing {
            peer.state.set_movement_type(PM_SPECTATOR);
        }
        peer.movement = Predictor::from_state(
            MovementState::from_player_state(&peer.state),
            jkr_game_jka::pmove::MovementConfig {
                legacy_fixes,
                ..authoritative()
            },
        );
        if let Some(lengths) = map.as_ref().and_then(|map| map.animations.clone()) {
            peer.movement.set_animation_lengths(lengths);
        }
        peer.movement.copied_to_entity();
        // `ClientSpawn` ends with its think, the frame's end, and the snapped conversion.
        let others = crate::peer::Others {
            boxes: obstacles,
            legs: body_legs,
            gametype: *gametype,
            ghoul2_time: *last_frame_time,
            // A player spawns standing still: its first move strikes nothing, and locks
            // nothing it would not lock on a client's prediction.
            blows: None,
            lock_targets: &jkr_game_jka::pmove::players_by_number,
        };
        let _ = peer.think(
            command,
            map.as_ref(),
            others,
            pool,
            server_time,
            &mut self.deaths.rng,
        );
        let _ = peer.end_frame(pool, server_time, (lag_icon, 0));
        peer.entity.spawn_finished(&peer.state);
        // `G_KillBox`: whoever stands in the spawned player's box is telefragged.
        if playing {
            let origin = peer.state.origin();
            let (bottom, top) = peer.movement.box_bounds();
            let victims: Vec<u16> = obstacles
                .iter()
                .filter(|other| {
                    let (absmin, absmax) = other.absolute_box();
                    (0..3).all(|axis| {
                        origin[axis] + bottom[axis] <= absmax[axis]
                            && origin[axis] + top[axis] >= absmin[axis]
                    })
                })
                .map(|other| other.entity)
                .collect();
            let max_health = peer.state.max_health();
            let team = peer.session.team;
            for victim in victims {
                let attacker = Attacker {
                    npc: false,
                    client: client as u16,
                    max_health,
                    team,
                    saber_knockback: [0.0; 4],
                };
                let _ = self.hurt(
                    usize::from(victim),
                    DamageRequest {
                        level_time: server_time,
                        attacker: Some(attacker),
                        direction: None,
                        point: None,
                        damage: 100_000,
                        flags: DAMAGE_NO_PROTECTION,
                        means: MOD_TELEFRAG,
                    },
                );
            }
        }
        // `ClientSpawn`'s fresh sequencer, then the ranks `ClientBegin` and `ClientRespawn` recalculate.
        self.scripts_client_spawned(client);
        self.siege_client_spawned(client);
        self.calculate_ranks();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{EF_INVULNERABLE, Identity, NativeGame};
    use jkr_network::LegacyGameHost;
    use jkr_protocol::UserCommand;

    /// A game whose cvar `name` is `value`, with one player in it, respawned at server
    /// time 1000.
    fn respawned(name: &[u8], value: &[u8]) -> NativeGame {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let mut cvars = crate::cvars::Cvars::server();
        cvars.set(name, value);
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        let userinfo = b"\\name\\a\\rate\\25000\\snaps\\40\\model\\kyle/default\\forcepowers\\7-1-032330000000001333\\color1\\4\\color2\\4\\handicap\\100\\cg_predictItems\\1\\saber1\\single_1\\saber2\\none\\char_color_red\\255\\char_color_green\\255\\char_color_blue\\255\\ip\\127.0.0.1:1";
        game.client_connect(0, userinfo).unwrap();
        game.enter_world(0, &UserCommand::default(), 100);
        // In the game, not spectating.
        game.peer_mut(0).unwrap().session.team = 0;
        game.respawn(0, UserCommand::default(), 1_000, 1_000);
        game
    }

    /// `ClientSpawn`'s protection (`g_client.c:3772-3776`): `g_spawnInvulnerability`
    /// milliseconds of it, and none at all at 0.
    #[test]
    fn spawn_protection_lasts_g_spawn_invulnerability() {
        for (setting, until) in [(b"3000".as_slice(), 4_000), (b"1500", 2_500), (b"0", 0)] {
            let game = respawned(b"g_spawnInvulnerability", setting);
            let peer = game.peer(0).unwrap();
            assert_eq!(peer.invulnerable_until, until);
            let flagged = peer.state.entity_flags() & EF_INVULNERABLE != 0;
            assert_eq!(flagged, until != 0);
        }
    }

    /// `G_RunClient`'s forced update: a player silent past `g_forceClientUpdateRate`
    /// milliseconds is thought for with an empty command; 0 leaves it be.
    #[test]
    fn a_silent_player_is_thought_for_after_the_update_rate() {
        for (rate, thought) in [(b"250".as_slice(), true), (b"0", false)] {
            let mut game = respawned(b"g_forceClientUpdateRate", rate);
            game.peer_mut(0).unwrap().last_command_time = 1_000;
            game.forced_client_update(0, 1_200);
            assert_eq!(game.peer(0).unwrap().last_command_time, 1_000);
            game.forced_client_update(0, 1_300);
            let peer = game.peer(0).unwrap();
            assert_eq!(peer.last_command_time == 1_300, thought, "rate {rate:?}");
        }
    }

    /// `ClientThink_real`'s dead branch (`g_active.c:3458-3474`): `g_forceRespawn` seconds
    /// past the respawn time respawn a dead player without a press; 0 never does.
    #[test]
    fn the_dead_are_respawned_after_g_force_respawn_seconds() {
        for (setting, forced) in [(b"60".as_slice(), true), (b"0", false)] {
            let mut game = respawned(b"g_forceRespawn", setting);
            let peer = game.peer_mut(0).unwrap();
            (peer.health, peer.state.stats[0]) = (0, 0);
            peer.mortality.respawn_time = 2_000;
            game.last_frame_time = 62_100;
            let command = UserCommand {
                server_time: 62_100,
                ..UserCommand::default()
            };
            game.client_think(0, &command, 62_100);
            assert_eq!(game.peer(0).unwrap().health > 0, forced);
        }
    }
}
