//! jaPRO's match options on this server: `g_mercyRule` (a team game ends early on a big
//! lead, [`jkr_game_jka::match_end::Limits::mercy`], kept from the cvar hook),
//! `g_allowTeamSuicide` (`kill` in a team game; registered on so a stock game allows it,
//! while jaPRO's own default refuses it) and `g_showHealth` (every player's health in its
//! entity, `G_ScaleNetHealth`, for the health bars clients draw over what they aim at);
//! `g_corpseRemovalTime` (the body sink time, stock's 30 s, kept from the cvar hook);
//! `g_lagIcon` (the connection-trouble sprite) and `g_allowSamePlayerNames` (see
//! `bridge_userinfo`), both registered on so a stock game keeps stock's ways, where
//! jaPRO's defaults turn them off; `g_allowUseInDuel` (`bridge_multiples`);
//! `g_godChat` (no harm to or from a typing player in a free-for-all,
//! [`jkr_game_jka::god_chat`]); `g_blockDuelHealthSpec` (`bridge_follow`); `g_scoreNPCs`
//! (what a player's kill of an NPC counts for, `NpcHost::score_npcs`, 2 = stock);
//! `g_autoQuit` (an idle server quits after so many days, kept from the cvar hook); and
//! `g_fakeClients` (bots' scoreboard pings, `bridge_ranks`); `g_allowTargetLaser`
//! (`bridge_target_laser`).

/// `TEAM_SPECTATOR`.
const TEAM_SPECTATOR: i32 = 3;
/// `STAT_MAX_HEALTH`; `entityState_t`'s `health` and `maxhealth`.
const STAT_MAX_HEALTH: usize = 8;
const ES_HEALTH: usize = 69;
const ES_MAX_HEALTH: usize = 73;

use super::*;

impl NativeGame {
    /// The match options, archived and announced.
    pub(super) fn register_game_options(&mut self) {
        let options: [(&[u8], &[u8], &[u8]); 13] = [
            (
                b"g_mercyRule",
                b"0",
                b"jaPRO: past half the frag limit, a team leading by this share of it wins",
            ),
            (
                b"g_allowTeamSuicide",
                b"1",
                b"jaPRO: kill is allowed in a team game (jaPRO's default is 0)",
            ),
            (
                b"g_showHealth",
                b"0",
                b"jaPRO: every player's health goes out for the health bars",
            ),
            (
                b"g_corpseRemovalTime",
                b"30",
                b"jaPRO: seconds before a body sinks away",
            ),
            (
                b"g_lagIcon",
                b"1",
                b"jaPRO: a silent player shows connection trouble (jaPRO's default is 0)",
            ),
            (
                b"g_allowSamePlayerNames",
                b"1",
                b"jaPRO: two players may share a name (0 numbers the newcomer's)",
            ),
            (
                b"g_allowUseInDuel",
                b"1",
                b"jaPRO: duellists use triggers (0: no use buttons; 2: none at all)",
            ),
            (
                b"g_godChat",
                b"0",
                b"jaPRO: in FFA, a player typing for 3 s cannot be hurt, and deals no harm",
            ),
            (
                b"g_blockDuelHealthSpec",
                b"0",
                b"jaPRO: spectators following a duellist do not see its health",
            ),
            (
                b"g_scoreNPCs",
                b"2",
                b"jaPRO: NPC kills score (0 not at all, 1 without the victory taunt, 2 fully)",
            ),
            (
                b"g_autoQuit",
                b"0",
                b"jaPRO: with nobody playing, the server quits after this many days up",
            ),
            (
                b"g_fakeClients",
                b"0",
                b"jaPRO: bots show an ordinary ping on the scoreboard",
            ),
            (
                super::bridge_target_laser::ALLOW_TARGET_LASER,
                b"0",
                b"jaPRO: +button15 shows everyone a laser where the player aims",
            ),
        ];
        for (name, value, about) in options {
            self.cvars
                .get_announced(name, value, crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }

    /// jaPRO's `g_autoQuit`, once a frame: up more than that many days by the game's
    /// clock (`levelTime`) with nobody playing, the server quits — before its clock runs
    /// out. jaPRO multiplies the days in an `int`, which makes 25 or more quit at once;
    /// here they are counted in 64 bits.
    pub(super) fn auto_quit(&mut self, server_time: i32) {
        let days = i64::from(self.auto_quit_days);
        if days <= 0 || self.quitting || i64::from(server_time) <= days * 86_400_000 {
            return;
        }
        if self.voting_clients() != 0 {
            return;
        }
        self.quitting = true;
        println!(
            "Auto quitting server (up {} days)",
            server_time / 86_400_000
        );
        self.commands.add_text(b"quit\n", &mut |_| {});
    }

    /// jaPRO's god chat in `G_Damage`: player `number` is typing, so its blows deal nothing
    /// ([`jkr_game_jka::god_chat`]).
    pub(super) fn typing_player(&self, number: u16) -> bool {
        self.peer(usize::from(number))
            .is_some_and(|peer| peer.typing.typing())
    }

    /// [`Self::typing_player`] for a blow's attacker, which no NPC is.
    pub(super) fn typing_attacker(&self, attacker: Option<jkr_game_jka::damage::Attacker>) -> bool {
        attacker.is_some_and(|attacker| !attacker.npc && self.typing_player(attacker.client))
    }

    /// jaPRO's `g_showHealth`, once a frame while it is on: each playing player's health
    /// and maximum in its entity (`G_ScaleNetHealth`; the player's own maximum, where
    /// jaPRO shows every bar out of 100).
    pub(super) fn show_player_health(&mut self) {
        if self.show_health {
            self.write_player_health(true);
        }
    }

    /// `g_showHealth` changed (`CVU_ShowHealth`): on, the health goes out at once; off,
    /// every player's is taken back, as stock never sends it.
    pub(super) fn show_health_changed(&mut self, shown: bool) {
        self.show_health = shown;
        self.write_player_health(shown);
    }

    fn write_player_health(&mut self, shown: bool) {
        for client in 0..self.players.places() {
            if let Some(peer) = self.peer_mut(client) {
                let (max, health) = if shown && peer.playing() {
                    let max = peer.state.stats[STAT_MAX_HEALTH] as i32;
                    jkr_game_jka::damage::net_health(peer.health, max)
                } else {
                    (0, 0)
                };
                let state = peer.entity.state_mut();
                state.set_raw_field(ES_MAX_HEALTH, max);
                state.set_raw_field(ES_HEALTH, health);
            }
        }
    }

    /// `G_CheckClientTimeouts` (`g_active.c:3521-3541`), every frame: a player whose
    /// last command is older than `g_timeouttospec` seconds (70; 0 for never) goes to the
    /// spectators (`SetTeam(ent, "spectator")`).
    pub(super) fn check_client_timeouts(&mut self, level_time: i32) {
        let timeout = self.cvars.integer(b"g_timeouttospec");
        if timeout == 0 {
            return;
        }
        for client in 0..self.players.places() {
            let silent = self.peer(client).is_some_and(|peer| {
                peer.begun
                    && peer.session.team != TEAM_SPECTATOR
                    && level_time - peer.last_command.server_time > timeout.saturating_mul(1_000)
            });
            if silent {
                self.set_team_to(client, b"spectator", level_time);
            }
        }
    }
}
