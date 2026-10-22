//! Stock's corpse fade. Thirty seconds after a body was copied the server raises
//! `EV_BODYFADE` on it (`BodySink`); from the frame the client sees it, for a minute
//! (`bodyFadeTime`, `cg_event.c:2778-2792`), `CG_General` draws the body only through
//! the fade's passes (`cg_ents.c:1500-1631`):
//!
//! * the body blended by its own alpha (`RF_FORCE_ENT_ALPHA`), from 254 down by 0.08 a
//!   millisecond, nothing once a tenth of the time passes 254 (about 2.5 s);
//! * a glow tinted grey by the same count rising (`RF_RGB_TINT`), the shield's for a
//!   light-sided owner and the red saber glow for a dark-sided one (`ircg`'s side); past
//!   200 it holds there and burns away from the body's origin (`RF_DISINTEGRATE2`), with
//!   the Force sight sound (light) or lightning's (dark) as it starts;
//! * for the first 3.4 s a dark-sided body's electric shell, tinted red, one of two
//!   shaders at random each frame, crackling a tenth of the frames; a light-sided one
//!   hums its saber for the first 2.2 s.
//!
//! The server takes the body away 18 s after the fade began.

use jkr_runtime::EntityId;

/// `EV_BODYFADE`, and the two bits `G_AddEvent` steps (`EV_EVENT_BITS`).
const EV_BODYFADE: u16 = 100;
const EV_EVENT_BITS: u16 = 0x300;
/// How long the fade's passes stand in for the body (`bodyFadeTime = cg.time + 60000`),
/// and with jaPRO's `cg_stylePlayer` no-bodies bit (`cg.time + 5000`, after which the body
/// is not drawn at all).
const FADE_MS: i64 = 60_000;
const NO_BODIES_MS: i64 = 5_000;
/// Until when a dark-sided body crackles and a light-sided one hums.
const ELECTRIC_MS: i64 = 3_400;
const HUM_MS: i64 = 2_200;
/// Stock rolls its per-frame sounds once a frame (about every 16 ms at its own rates);
/// `S_StartSound` restarts the same sound on the same entity at most every 50 ms.
const ROLL_MS: i64 = 16;
const RESTART_MS: i64 = 50;

/// The sounds a fade plays (`cg_ents.c:1541-1546`, `1582-1608`; `cgs.media.crackleSound`).
pub(crate) const SOUNDS: [&str; 4] = [
    "sound/weapons/force/see.wav",
    "sound/weapons/force/lightning",
    "sound/weapons/saber/saberhum1.wav",
    "sound/effects/energy_crackle.wav",
];

/// One corpse's fade as this client saw it begin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CorpseFade {
    entity: Option<EntityId>,
    /// The event field last seen: one event starts the fade once.
    last_event: u16,
    /// When the fade began (`bodyFadeTime - 60000`), and when its glow began to burn
    /// (`dustTrailTime`).
    since: Option<i64>,
    burning_since: Option<i64>,
    /// The last time the per-frame sounds were rolled for and a hum started.
    rolled: i64,
    hummed: i64,
    /// This frame's passes, worked out once by [`Self::update`].
    current: Option<Passes>,
    /// jaPRO's no-bodies style was asked for when the fade was last observed.
    no_bodies: bool,
}

/// What a fading corpse draws and plays this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Passes {
    /// The body's own alpha (`RF_FORCE_ENT_ALPHA`), when it is drawn at all.
    pub(crate) body_alpha: Option<u8>,
    /// The glow's grey tint and alpha (`RF_RGB_TINT`); 0, which a fade never draws, for none.
    pub(crate) glow: u8,
    /// When the glow began to burn away from the body's origin (`RF_DISINTEGRATE2`).
    pub(crate) burning_since: Option<i64>,
    /// A dark-sided body's electric shell, and which of its two shaders.
    pub(crate) electric: Option<bool>,
    /// Sounds starting this frame, by [`SOUNDS`] index.
    pub(crate) sounds: [bool; 4],
}

impl CorpseFade {
    /// Each frame: the corpse's entity and its event field now. A new `EV_BODYFADE` begins
    /// the fade; another entity in the slot starts afresh (`CG_BodyQueueCopy`), and one
    /// first seen with the event already raised fades from then (`CG_ResetEntity` clears
    /// `previousEvent`, so `CG_CheckEvents` takes it).
    ///
    /// With `no_bodies` (jaPRO's `cg_stylePlayer` 8192) the fade begins as the body is first
    /// seen and the server's `EV_BODYFADE` is left alone (EternalJK `cg_ents.c:943-944`,
    /// `cg_event.c:3178-3181`).
    pub(crate) fn observe(
        &mut self,
        entity: Option<EntityId>,
        event: u16,
        now: i64,
        no_bodies: bool,
    ) {
        if self.entity != entity {
            *self = Self {
                entity,
                ..Self::default()
            };
        }
        self.no_bodies = no_bodies;
        if no_bodies {
            if self.since.is_none() {
                self.since = Some(now);
                self.rolled = now - ROLL_MS;
            }
            return;
        }
        if event != self.last_event {
            self.last_event = event;
            if event & !EV_EVENT_BITS == EV_BODYFADE {
                *self = Self {
                    entity,
                    last_event: event,
                    since: Some(now),
                    rolled: now - ROLL_MS,
                    ..Self::default()
                };
            }
        }
    }

    /// This frame's passes, once a frame before the drawing reads them ([`Self::current`]);
    /// `no_effects` (jaPRO's `cg_stylePlayer` 16384) leaves out the crackle, hum, burning
    /// sounds and electric shell (EternalJK `cg_ents.c:1676`).
    pub(crate) fn update(
        &mut self,
        now: i64,
        (light_side, no_effects): (bool, bool),
        random: impl FnMut() -> f32,
    ) {
        self.current = self.passes(now, light_side, random).map(|passes| {
            if no_effects {
                Passes {
                    electric: None,
                    sounds: [false; 4],
                    ..passes
                }
            } else {
                passes
            }
        });
    }

    /// The passes [`Self::update`] worked out this frame.
    pub(crate) fn current(&self) -> Option<Passes> {
        self.current
    }

    /// The fade's passes at `now`, for an owner of the light side or not; `None` when the
    /// body is drawn as ever. `random` draws the per-frame choices.
    fn passes(
        &mut self,
        now: i64,
        light_side: bool,
        mut random: impl FnMut() -> f32,
    ) -> Option<Passes> {
        let since = self.since?;
        let elapsed = now - since;
        if self.no_bodies && elapsed >= NO_BODIES_MS {
            // Faded already: nothing of the body is drawn.
            return Some(Passes {
                body_alpha: None,
                glow: 0,
                burning_since: None,
                electric: None,
                sounds: [false; 4],
            });
        }
        if !(0..FADE_MS).contains(&elapsed) {
            return None;
        }
        // `int tMult = curTimeDif * 0.08`, `curTimeDif * 0.1 > 254`.
        let multiplier = (elapsed as f64 * 0.08) as i32;
        let body = if elapsed as f64 * 0.1 > 254.0 {
            0
        } else {
            (254 - multiplier) as u8
        };
        let mut sounds = [false; 4];
        let glow = if multiplier > 200 {
            if self.burning_since.is_none() {
                self.burning_since = Some(now);
                sounds[usize::from(!light_side)] = true;
            }
            200
        } else {
            multiplier.max(1) as u8
        };
        let electric = (!light_side && elapsed < ELECTRIC_MS).then(|| random() > 0.5);
        let mut crackle = false;
        while self.rolled + ROLL_MS <= now {
            self.rolled += ROLL_MS;
            crackle |= !light_side && self.rolled - since < ELECTRIC_MS && random() > 0.9;
        }
        sounds[3] = crackle;
        if light_side && elapsed < HUM_MS && now - self.hummed >= RESTART_MS {
            self.hummed = now;
            sounds[2] = true;
        }
        Some(Passes {
            body_alpha: (body >= 1).then_some(body),
            glow,
            burning_since: self.burning_since,
            electric,
            sounds,
        })
    }
}

/// The event field of the entity `entity` draws, from the snapshot.
pub(crate) fn entity_event(
    entity: Option<EntityId>,
    snapshot: Option<&jkr_protocol::Snapshot>,
) -> u16 {
    let (Some(id), Some(snapshot)) = (entity, snapshot) else {
        return 0;
    };
    let Some(number) = id
        .get()
        .checked_sub(1)
        .and_then(|number| u16::try_from(number).ok())
    else {
        return 0;
    };
    snapshot
        .entities
        .binary_search_by_key(&number, |state| state.number())
        .map_or(0, |index| snapshot.entities[index].event())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fading(light_side: bool) -> CorpseFade {
        let mut fade = CorpseFade::default();
        let body = Some(EntityId::new(40));
        fade.observe(body, 0, 0, false);
        fade.observe(body, EV_BODYFADE | 0x100, 1_000, false);
        fade.observe(body, EV_BODYFADE | 0x100, 1_200, false);
        assert!(fade.passes(999, light_side, || 0.0).is_none());
        fade
    }

    #[test]
    fn the_body_fades_and_the_glow_rises_then_burns() {
        let mut fade = fading(true);
        let start = fade.passes(1_000, true, || 0.0).unwrap();
        assert_eq!((start.body_alpha, start.glow), (Some(254), 1));
        assert_eq!(start.sounds, [false, false, true, false]);
        let one_second = fade.passes(2_000, true, || 0.0).unwrap();
        assert_eq!((one_second.body_alpha, one_second.glow), (Some(174), 80));
        // Past 200 the glow holds and starts to burn, with its sound once.
        let burning = fade.passes(3_520, true, || 0.0).unwrap();
        assert_eq!((burning.glow, burning.burning_since), (200, Some(3_520)));
        assert!(burning.sounds[0]);
        assert!(!fade.passes(3_521, true, || 0.0).unwrap().sounds[0]);
        // A tenth of the time past 254: the body is gone; the glow goes on.
        let gone = fade.passes(3_541, true, || 0.0).unwrap();
        assert_eq!((gone.body_alpha, gone.glow), (None, 200));
        // A minute on, the body would be drawn as ever (the server has taken it by then).
        assert!(fade.passes(61_000, true, || 0.0).is_none());
    }

    #[test]
    fn a_dark_body_crackles_in_its_electric_shell_for_a_while() {
        let mut fade = fading(false);
        let start = fade.passes(1_032, false, || 1.0).unwrap();
        assert_eq!(start.electric, Some(true));
        assert!(start.sounds[3] && !start.sounds[2]);
        assert_eq!(fade.passes(4_401, false, || 1.0).unwrap().electric, None);
        // Another body in the slot starts afresh; one first seen fading fades from then.
        fade.observe(Some(EntityId::new(41)), 0, 5_000, false);
        assert!(fade.passes(5_000, false, || 1.0).is_none());
        fade.observe(Some(EntityId::new(42)), EV_BODYFADE, 6_000, false);
        assert_eq!(
            fade.passes(6_000, false, || 1.0).unwrap().body_alpha,
            Some(254)
        );
    }

    #[test]
    fn no_bodies_fade_at_once_and_go_and_no_effects_stay_quiet() {
        let mut fade = CorpseFade::default();
        let body = Some(EntityId::new(40));
        fade.observe(body, 0, 2_000, true);
        // The server's later fade event changes nothing.
        fade.observe(body, EV_BODYFADE | 0x100, 2_500, true);
        let start = fade.passes(2_000, false, || 1.0).unwrap();
        assert_eq!(start.body_alpha, Some(254));
        let gone = fade.passes(7_000, false, || 1.0).unwrap();
        assert_eq!((gone.body_alpha, gone.glow), (None, 0));
        fade.update(2_032, (false, true), || 1.0);
        let quiet = fade.current().unwrap();
        assert_eq!((quiet.electric, quiet.sounds), (None, [false; 4]));
    }
}
