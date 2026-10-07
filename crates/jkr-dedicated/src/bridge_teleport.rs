//! `TeleportPlayer` on this server (`g_misc.c:197-256`), which a teleporter's touch and the
//! `setviewpos` cheat (`Cmd_SetViewpos_f`, `g_cmds.c:2502-2522`) share: the player put where
//! it goes, facing its angles, the flashes where it left and where it arrived, and
//! whatever stands there telefragged (`G_KillBox`).

use super::*;

impl NativeGame {
    /// `TeleportPlayer` for player `client`, standing at `from`, to `destination` facing
    /// `angles`: a spectator's jump is silent and kills nobody.
    pub(super) fn teleport_client(
        &mut self,
        client: usize,
        from: [f32; 3],
        destination: [f32; 3],
        angles: [f32; 3],
        spectating: bool,
        level_time: i32,
    ) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let command_angles = peer.last_command.angles;
        let teleported = jkr_game_jka::triggers::teleport_player(
            &mut peer.state,
            destination,
            angles,
            spectating,
        );
        jkr_game_jka::triggers::face(&mut peer.state, teleported.angles, command_angles);
        peer.movement = peer.movement.reseeded(&peer.state);
        self.after_teleport(client, (from, destination), teleported, level_time);
    }

    /// jaPRO's `AmTeleportPlayer` for its admins' teleports: as [`Self::teleport_client`],
    /// but at a standstill eight units up ([`jkr_game_jka::triggers::admin_teleport_player`]),
    /// a rolling player stood up first. (A rider's vehicle is not carried along.)
    pub(super) fn admin_teleport(
        &mut self,
        client: usize,
        destination: [f32; 3],
        angles: [f32; 3],
        level_time: i32,
    ) {
        use jkr_game_jka::pmove_anim::{
            SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART,
        };
        const TEAM_SPECTATOR: i32 = 3;
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if jkr_game_jka::pmove_roll::in_roll(peer.movement.state())
            && let Some(uncrouch) = jkr_game_jka::legacy_animation_index("BOTH_UNCROUCH1")
        {
            peer.movement.set_animation_parts(
                SETANIM_BOTH,
                uncrouch as u16,
                SETANIM_FLAG_RESTART | SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            peer.movement.write_player_state(&mut peer.state);
        }
        let from = peer.state.origin();
        let spectating = peer.session.team == TEAM_SPECTATOR;
        let command_angles = peer.last_command.angles;
        let teleported = jkr_game_jka::triggers::admin_teleport_player(
            &mut peer.state,
            destination,
            angles,
            spectating,
        );
        jkr_game_jka::triggers::face(&mut peer.state, teleported.angles, command_angles);
        peer.movement = peer.movement.reseeded(&peer.state);
        self.after_teleport(client, (from, destination), teleported, level_time);
    }

    /// A teleport's show and its kill: the flashes where the player left and where it
    /// arrived, and `G_KillBox` on whatever stands there.
    fn after_teleport(
        &mut self,
        client: usize,
        (from, destination): ([f32; 3], [f32; 3]),
        teleported: jkr_game_jka::triggers::Teleported,
        level_time: i32,
    ) {
        if teleported.flashes {
            let out = EventEntity::teleport_out(from, client as u16);
            let arrived = EventEntity::teleport_in(destination, client as u16);
            let _ = self.pool.spawn_temporary(out.state(), level_time, None);
            let _ = self.pool.spawn_temporary(arrived.state(), level_time, None);
        }
        // `G_KillBox`: anything standing where it arrives is telefragged.
        if teleported.kill_box {
            self.gather_obstacles(client);
            let obstacles = std::mem::take(&mut self.obstacles);
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let (bottom, top) = peer.movement.box_bounds();
            let arrived = teleported.origin;
            let (max_health, team) = (peer.state.max_health(), peer.session.team);
            let victims: Vec<u16> = obstacles
                .iter()
                .filter(|other| {
                    let (absmin, absmax) = other.absolute_box();
                    (0..3).all(|axis| {
                        arrived[axis] + bottom[axis] <= absmax[axis]
                            && arrived[axis] + top[axis] >= absmin[axis]
                    })
                })
                .map(|other| other.entity)
                .collect();
            self.obstacles = obstacles;
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
                        level_time,
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
    }

    /// `Cmd_SetViewpos_f` (`g_cmds.c:2502-2522`, `CMD_CHEAT`): `setviewpos x y z yaw` puts
    /// player `client` there, facing along the yaw.
    pub(super) fn set_view_position(
        &mut self,
        client: usize,
        arguments: &[&[u8]],
        level_time: i32,
    ) {
        if !self.settings.cheats {
            self.told
                .push(Told::One(client, b"print \"@@@NOCHEATS\n\"".to_vec()));
            return;
        }
        let Ok([x, y, z, yaw]) = <[&[u8]; 4]>::try_from(arguments) else {
            self.told.push(Told::One(
                client,
                b"print \"usage: setviewpos x y z yaw\n\"".to_vec(),
            ));
            return;
        };
        let value = |text: &[u8]| jkr_game_jka::text_parse::atof(text);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (from, spectating) = (peer.state.origin(), !peer.playing());
        self.teleport_client(
            client,
            from,
            [value(x), value(y), value(z)],
            [0.0, value(yaw), 0.0],
            spectating,
            level_time,
        );
    }
}
