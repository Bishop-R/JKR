//! What a player sends: `ClientCommand` (`g_cmds.c`) and `ClientThink` (`g_active.c`), for
//! the player at a place in the profile's order. The host boundary (`bridge_host.rs`) calls
//! them for legacy clients; the bots' frame calls them for bots.

use super::*;

impl NativeGame {
    /// `ClientCommand`: a reliable command from the player at `client`'s place, at the
    /// server's clock `server_time`.
    pub(super) fn player_command(&mut self, client: usize, text: &[u8], server_time: i32) {
        let mut words = text
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty());
        let Some(command) = words.next() else { return };
        if self.command_refused(client, command) {
            return;
        }
        if self.emote_command(client, command, server_time) || self.admin_command(client, text) {
            return;
        }
        // jaPRO's `ammotd` (`CMD_NOINTERMISSION`): the message of the day again.
        if command.eq_ignore_ascii_case(b"ammotd") {
            if !self.refused_at_intermission(client, command) {
                self.show_motd(client, true);
            }
            return;
        }
        if command.eq_ignore_ascii_case(b"addbot") {
            // `Cmd_AddBot_f`: the menus offer it, but only the server adds bots.
            self.told.push(Told::One(
                client,
                b"print \"@@@ONLY_ADD_BOTS_AS_SERVER.\n\"".to_vec(),
            ));
            return;
        }
        if command.eq_ignore_ascii_case(b"score") {
            // `Cmd_Score_f`: the scoreboard, as this client asked for it.
            let message = self.scoreboard(server_time);
            self.told.push(Told::One(client, message));
            return;
        }
        if command.eq_ignore_ascii_case(b"duelteam") {
            let word = words.next();
            self.duel_team_command(client, word, server_time);
            return;
        }
        if [&b"follow"[..], b"follownext", b"followprev"]
            .iter()
            .any(|name| command.eq_ignore_ascii_case(name))
        {
            // `CMD_NOINTERMISSION` (`g_cmds.c:3398-3400`).
            if !self.refused_at_intermission(client, command) {
                let words: Vec<&[u8]> = text
                    .split(|byte| byte.is_ascii_whitespace())
                    .filter(|word| !word.is_empty())
                    .collect();
                self.follow_command(client, &words, server_time);
            }
            return;
        }
        if command.eq_ignore_ascii_case(b"setviewpos") {
            let arguments: Vec<&[u8]> = words.collect();
            self.set_view_position(client, &arguments, server_time);
            return;
        }
        if command.eq_ignore_ascii_case(b"siegeclass") {
            // `Cmd_SiegeClass_f`: only a siege game has classes.
            self.siege_class_command(client, text, server_time);
            return;
        }
        if command.eq_ignore_ascii_case(b"forcechanged") {
            let arguments: Vec<&[u8]> = words.collect();
            self.force_changed(client, &arguments, server_time);
            return;
        }
        if self.information_command(client, text, server_time)
            || self.chat_command(client, text, server_time)
            || self.vote_command(client, text, server_time)
            || self.npc_client_command(client, text)
            || self.cheat_command(client, text, server_time)
        {
            return;
        }
        if command.eq_ignore_ascii_case(b"team") {
            let arguments: Vec<&[u8]> = words.collect();
            self.team_request(client, &arguments, server_time);
            return;
        }
        // Not in the reference's table (`g_cmds.c:3446-3450`).
        let reply = [b"print \"Unknown command ".as_slice(), command, b"\n\""].concat();
        self.told.push(Told::One(client, reply));
    }

    /// `Cmd_Team_f` with `arguments`, the words after `team`.
    fn team_request(&mut self, client: usize, arguments: &[&[u8]], server_time: i32) {
        let gametype = self.gametype;
        // `SetTeam` on a siege server, once `Cmd_Team_f`'s own checks have passed.
        if self.siege.is_some()
            && let [word] = arguments
            && self
                .peer(client)
                .is_some_and(|peer| peer.session.switch_team_time <= server_time)
        {
            self.siege_team_command(client, word, server_time);
            return;
        }
        let sides = self.sides(client);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (before, name) = (peer.session.team, peer.name.clone());
        // The game this server is running, not a hard-coded free-for-all: `SetTeam`
        // only knows red and blue from `GT_TEAM` up, which is what a siege class's own
        // side change depends on.
        match team_command(
            &mut peer.session,
            &name,
            arguments,
            gametype,
            sides,
            server_time,
        ) {
            TeamCommand::Told(text) => self.told.push(Told::One(client, text)),
            TeamCommand::Unchanged | TeamCommand::NotSupported => {}
            TeamCommand::Changed {
                announcement,
                queued,
            } => self.team_changed(client, before, announcement, queued, server_time),
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.session.team_command_done(before, server_time);
        }
    }

    /// `Cmd_ForceChanged_f` (`g_cmds.c:1276-1311`): a spectator's Force is read again at
    /// once; a player is told and gets it at the next spawn. Outside duels an argument
    /// is the menu's combined team choice.
    fn force_changed(&mut self, client: usize, arguments: &[&[u8]], server_time: i32) {
        let settings = self.settings;
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if peer.session.team == i32::from(TEAM_SPECTATOR) {
            let mut force = jkr_game_jka::client_begin::init_force_powers(
                &mut peer.session,
                &peer.userinfo,
                settings,
            );
            let commands = std::mem::take(&mut force.commands);
            peer.session.force = Some(force);
            for command in commands {
                self.told.push(Told::One(client, command));
            }
        } else {
            peer.session.force_do_init = true;
            self.told.push(Told::One(
                client,
                b"print \"^2@@@FORCEPOWERCHANGED\n\"".to_vec(),
            ));
        }
        const GT_DUEL: i32 = 3;
        const GT_POWERDUEL: i32 = 4;
        if matches!(self.gametype, GT_DUEL | GT_POWERDUEL) {
            return;
        }
        if let Some(word) = arguments.first().filter(|word| !word.is_empty()) {
            self.team_request(client, &[word], server_time);
        }
    }

    /// `ClientThink`: one movement command from the player at `client`'s place.
    pub(super) fn player_think(&mut self, client: usize, command: &UserCommand, server_time: i32) {
        // `ClientThink_real` (g_active.c:2006-2012) keeps a command's time within a
        // second behind and a fifth of a second ahead of the server's.
        let mut command = *command;

        command.server_time = command.server_time.clamp(
            server_time.wrapping_sub(1000),
            server_time.wrapping_add(200),
        );

        // A pilot's command goes to its vehicle first (`g_active.c:1889-1910`).
        self.riding_opening(client, &mut command, self.last_frame_time);
        // `ClientThink_real`'s stance upkeep (`g_active.c:1914-1993`): the style the
        // sabers held allow.
        if let Some(peer) = self.peer_mut(client) {
            jkr_game_jka::saber_stance::upkeep_player(
                &mut peer.state,
                &mut peer.movement,
                &peer.sabers.hands,
            );
            // The powers the sabers forbid (`WP_ForcePowerUsable`), as they stand now.
            peer.force.saber_restrictions = peer.sabers.force_restrictions();
        }
        // `G_HeldByMonster` (`g_active.c:2000-2003`): a monster's victim goes where it is held.
        self.held_by_monster(client, &mut command);
        // `ClientThink_real` (`g_active.c:2356-2363`) sets the speed before EVERY move,
        // not once at the spawn: `g_speed`, times a siege class's own multiplier. Doing
        // it only at the spawn leaves a client that picked a class predicting one speed
        // while the server moves it at another, which shows up as prediction misses a
        // snapshot's worth of movement wide.
        if let Some(siege) = self.siege.as_ref() {
            let registry = std::sync::Arc::clone(&siege.registry);
            if let Some(peer) = self.peer_mut(client)
                && peer.playing()
            {
                let class_speed = peer
                    .siege_class_index
                    .map_or(1.0, |index| registry.classes[index].speed);
                peer.state.set_speed(PLAYER_SPEED * class_speed);
                peer.state
                    .set_base_speed((PLAYER_SPEED * class_speed) as i32);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        // `ClientThink_real` (`g_active.c:2046-2054`): at an intermission a player does
        // nothing at all but press the button that says it is ready to go.
        if self.match_end.intermission_time != 0 {
            self.intermission_think(client, &command, server_time);
            return;
        }
        // `SpectatorThink` (`g_active.c:697-760`): a follower does not move itself; the
        // buttons pick whom to follow.
        if let Some(following) = self
            .peer(client)
            .filter(|peer| !peer.playing())
            .map(|peer| {
                peer.session.spectator_state == jkr_game_jka::client_begin::SPECTATOR_FOLLOW
            })
        {
            if following {
                if let Some(peer) = self.peer_mut(client) {
                    peer.last_command_time = server_time;
                    peer.last_command = command;
                }
            } else {
                self.think_in_world(client, command, server_time);
            }
            self.spectator_buttons(client, &command, server_time);
            return;
        }
        self.think_in_world(client, command, server_time);
    }
}
