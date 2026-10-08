//! The local player's own weapon in the view ([`jkr_client::fire_feedback`]): shakes after
//! heavy and charged shots and while a shot charges with the gun drawn, and the concussion
//! rifle's kick. EternalJK's `cg_screenShake` (archived, 2) chooses which shakes show: 0
//! none of the effects' (`CG_FX_CameraShake`), the damage kick or the weapons', 1 all but
//! the weapons', 2 all of them, as stock. A server's own shake (`EV_SCREENSHAKE`) always
//! shows. On a jaPRO server the damage kick is the server's choice (`jcinfo` bit 14, its
//! `g_screenShake`): always with it, never without (`CG_OffsetFirstPersonView`).

use crate::GpuState;
use crate::console::ViewerConsole;
use jkr_client::FireFeedback;
use std::time::Instant;

/// `JAPRO_CINFO_SCREENSHAKE`.
const JAPRO_SCREEN_SHAKE: i32 = 1 << 14;

/// `cg_screenShake`: 0 none, 1 no weapon shakes, 2 all.
pub(crate) fn level(console: Option<&ViewerConsole>) -> i64 {
    console
        .and_then(|console| console.integer_cvar("cg_screenShake"))
        .unwrap_or(2)
}

/// Whether the damage kick shows: on a jaPRO server as the server says, elsewhere unless
/// `cg_screenShake` is 0.
pub(crate) fn damage_kick(state: &GpuState) -> bool {
    let game = state
        .live_session
        .as_ref()
        .map(|session| session.game_state())
        .or_else(|| {
            state
                .demo_session
                .as_ref()
                .map(|session| session.game_state())
        });
    let info = game
        .and_then(|game| game.config_string(0))
        .unwrap_or_default();
    let japro = jkr_protocol::info_value(info, b"gamename")
        .is_some_and(|name| name.len() >= 5 && name[..5].eq_ignore_ascii_case(b"japro"));
    if japro {
        let jcinfo =
            jkr_protocol::info_value(info, b"jcinfo").map_or(0, jkr_game_jka::userinfo::atoi);
        return jcinfo & JAPRO_SCREEN_SHAKE != 0;
    }
    level(state.console.as_ref()) != 0
}

/// The effects' camera shakes against the local view (`CG_FX_CameraShake`), unless
/// `cg_screenShake` is 0; a demo showing another player's view takes none.
pub(crate) fn resolve_shakes(state: &mut GpuState, now: Instant) {
    let local_view = state.demo_session.as_ref().is_none_or(|session| {
        !matches!(session.camera(), crate::demo_playback::Camera::Spectate(_))
    });
    let shakes = local_view && level(state.console.as_ref()) != 0;
    state
        .effect_aux
        .resolve_shakes(state.camera_position, now, shakes);
}

/// Once a frame, before the camera: the shot fired since the last frame and a charging
/// shot, as shakes and a kick.
pub(crate) fn frame(state: &mut GpuState, time: i32, now: Instant) {
    let fired = state.first_person_view.take_fired();
    if level(state.console.as_ref()) < 2 {
        return;
    }
    let snapshot = super::presented_snapshot(
        state.live_session.as_ref(),
        state.demo_session.as_ref(),
        time,
    );
    let Some(player) = snapshot.map(|snapshot| &snapshot.player) else {
        return;
    };
    if let Some((weapon, alternate, at)) = fired {
        // The shot's charge as `cg.predictedPlayerEntity` holds it.
        let charge_time = state
            .live_session
            .as_ref()
            .and_then(|_| state.local_prediction.predicted_state())
            .map_or(player.weapon_charge_time(), |predicted| {
                predicted.weapon_charge_time
            });
        let random = crate::effect_envelope::random_unit(at as u32);
        match jkr_client::fire_feedback(
            weapon,
            alternate,
            at - charge_time,
            !state.third_person,
            random,
        ) {
            Some(FireFeedback::Shake(shake)) => {
                state
                    .effect_aux
                    .screen_shake(shake.intensity, shake.duration, now, at as u32);
            }
            Some(FireFeedback::Kick(pitch)) => state.first_person_view.kick = Some((at, pitch)),
            None => {}
        }
    }
    // The charge as the newest snapshot holds it (`cg_entities[clientNum]`), while
    // `CG_AddViewWeapon` draws the gun.
    let drawn = state
        .console
        .as_ref()
        .and_then(|console| console.bool_cvar("cg_drawGun"))
        .unwrap_or(true)
        && snapshot.is_some_and(|snapshot| {
            jkr_client::legacy_view_weapon_visible_as(snapshot, state.third_person, player.weapon())
                .is_some()
        });
    if drawn
        && let Some(shake) = jkr_client::charge_shake(
            player.weapon(),
            player.weapon_state(),
            time - player.weapon_charge_time(),
        )
    {
        state
            .effect_aux
            .screen_shake(shake.intensity, shake.duration, now, time as u32);
    }
}
