//! jaPRO's emote commands on this server ([`jkr_game_jka::japro_emotes`]): the commands'
//! gates (`CMD_NOINTERMISSION | CMD_ALIVE`), the families `g_emotesDisable` turns off, the
//! pose itself, and the held poses' freeze in each think.

use super::bridge_cheats::{ALIVE, NO_INTERMISSION};
use super::bridge_generic::{play_saber_sounds, saber_sound_hands};
use super::*;
use jkr_game_jka::japro_emotes::{self, Refusal};

/// The option's name: a bit per family (`emote_type_t`) whose emotes are refused.
const EMOTES_DISABLE: &[u8] = b"g_emotesDisable";
/// `WP_SABER`; `fd.saberAnimLevel`; `G_Sound`'s `CHAN_AUTO`.
const WP_SABER: u8 = 3;
const PS_SABER_ANIM_LEVEL: usize = 23;
const CHAN_AUTO: u32 = 0;

impl NativeGame {
    /// `g_emotesDisable`, archived, none off by default (jaPRO's).
    pub(super) fn register_emotes(&mut self) {
        let about: &[u8] = b"jaPRO: emote families refused, a bit each";
        self.cvars.get_announced(
            EMOTES_DISABLE,
            b"0",
            crate::cvars::CVAR_ARCHIVE,
            Some(about),
        );
    }

    /// An emote command from `client`: whether `command` was one. `amflip` strikes its
    /// saber style's pose and then toggles the saber (`Cmd_EmoteSaberFlip_f`).
    pub(super) fn emote_command(&mut self, client: usize, command: &[u8], level_time: i32) -> bool {
        let flip = command.eq_ignore_ascii_case(b"amflip");
        let named = japro_emotes::find(command);
        if !flip && named.is_none() {
            return false;
        }
        if !self.passes_gates(client, command, NO_INTERMISSION | ALIVE) {
            return true;
        }
        let disabled = self.cvars.integer(EMOTES_DISABLE) as u32;
        let gametype = self.gametype;
        let Some(peer) = self.peer_mut(client) else {
            return true;
        };
        if flip && peer.state.weapon() != WP_SABER {
            return true;
        }
        let family = named.map_or(japro_emotes::E_SABERFLIP, |emote| emote.family);
        if disabled & (1 << family) != 0 {
            let line = format!("print \"{}\"", japro_emotes::DISABLED).into_bytes();
            self.told.push(Told::One(client, line));
            return true;
        }
        let emote = if flip {
            let style = peer.state.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0) as u8;
            japro_emotes::saber_flip(style, peer.state.saber_holstered() != 0)
        } else {
            named
        };
        let mut sounds = Vec::new();
        let mut told = None;
        if let Some(emote) = emote {
            let legs = (peer.state.leg_animation(), peer.movement.state().legs_timer);
            match japro_emotes::refusal(&peer.state, legs, gametype) {
                Some(Refusal::Told(text)) => told = Some(text),
                Some(Refusal::Quiet) => {}
                None => japro_emotes::begin(
                    emote,
                    &mut peer.state,
                    &mut peer.movement,
                    &mut peer.emote,
                    &mut sounds,
                ),
            }
        }
        if flip {
            // `Cmd_ToggleSaber_f`, whatever the pose came to.
            let _ = jkr_game_jka::generic_commands::toggle_saber(
                &mut peer.state,
                level_time,
                &mut sounds,
            );
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        let (hands, origin) = (saber_sound_hands(peer), peer.state.origin());
        if let Some(text) = told {
            let line = format!("print \"{text}\"").into_bytes();
            self.told.push(Told::One(client, line));
        }
        play_saber_sounds(
            &mut self.pool,
            hands,
            sounds,
            (CHAN_AUTO, origin),
            level_time,
        );
        true
    }
}

/// Whether `ClientThink_real`'s movement chain reaches jaPRO's holds (the admin's freeze,
/// the emote's) for this player: alive, playing, neither in noclip, disintegrating nor
/// gripped.
pub(super) fn reaches_holds(peer: &Peer) -> bool {
    peer.playing()
        && !peer.noclip
        && peer.health > 0
        && peer.state.raw_field(EFLAGS).unwrap_or(0) & EF_DISINTEGRATION == 0
        && peer.force.grip_movement_type == 0
}

/// The held pose's step in `ClientThink_real`'s movement chain, after the admin's freeze,
/// where the chain reaches it ([`reaches_holds`]).
pub(super) fn emote_hold(
    peer: &mut Peer,
    command: &UserCommand,
    pool: &mut EntityPool,
    level_time: i32,
) {
    if !reaches_holds(peer) {
        return;
    }
    let mut sounds = Vec::new();
    if let Some(kind) = japro_emotes::hold(
        &mut peer.state,
        &mut peer.movement,
        command,
        &mut peer.emote,
        &mut sounds,
    ) {
        peer.state.set_movement_type(kind);
        peer.movement = peer.movement.reseeded(&peer.state);
    }
    if !sounds.is_empty() {
        let (hands, origin) = (saber_sound_hands(peer), peer.state.origin());
        play_saber_sounds(pool, hands, sounds, (CHAN_AUTO, origin), level_time);
    }
}
