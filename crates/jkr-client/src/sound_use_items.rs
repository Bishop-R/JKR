//! `CG_UseItem` (`cg_event.c:519-575`): the sound of a holdable put to use — the seeker's
//! hiss as it deploys, bacta for a medpac of either size — and, for the local player whose
//! weapon is ready, the electrobinoculars' zoom (`CG_ToggleBinoculars`, `cg_event.c:458-496`).
//!
//! The server sends the binoculars' event with 1 when they have just been put down and 2
//! when they have just been raised; cgame turns 1 into zoom mode 2 and 2 into 0 and then
//! plays `zoomStart` for mode 0 and `zoomEnd` for mode 2, so raising them starts the
//! zoom and lowering them ends it. Without a parameter (the use key's predicted event)
//! the mode at the time decides, the other way round. Stock also writes that mode into
//! the snapshot's player state for the frame; JKR does not, the server's mode stands.

/// `EV_USE_ITEM0` (`bg_public.h:868`).
const EV_USE_ITEM0: u16 = 45;
/// `HI_SEEKER`, `HI_MEDPAC`, `HI_MEDPAC_BIG` and `HI_BINOCULARS` (`bg_public.h:744-761`).
const SEEKER: u16 = 1;
const MEDPAC: u16 = 3;
const MEDPAC_BIG: u16 = 4;
const BINOCULARS: u16 = 5;

/// The registered sounds.
pub(super) struct UseItemSounds {
    seeker: Option<u16>,
    bacta: Option<u16>,
    /// `zoomStart` and `zoomEnd`.
    zoom: [Option<u16>; 2],
}

impl UseItemSounds {
    pub(super) fn register(intern: &mut impl FnMut(&str) -> u16) -> Self {
        Self {
            seeker: Some(intern("sound/chars/seeker/misc/hiss")),
            bacta: Some(intern("sound/items/use_bacta.wav")),
            zoom: [
                Some(intern("sound/interface/zoomstart.wav")),
                Some(intern("sound/interface/zoomend.wav")),
            ],
        }
    }

    /// The sound of item event `event`. `local`: the event is the local player's;
    /// `weapon_ready`: their weapon is ready (`WEAPON_READY`); `zoom`: their zoom mode.
    pub(super) fn sound(
        &self,
        event: u16,
        parameter: u8,
        local: bool,
        weapon_ready: bool,
        zoom: u8,
    ) -> Option<u16> {
        match event.checked_sub(EV_USE_ITEM0)? {
            SEEKER => self.seeker,
            MEDPAC | MEDPAC_BIG => self.bacta,
            BINOCULARS if local && weapon_ready => {
                let zoom = match parameter {
                    1 => 2,
                    2 => 0,
                    _ => zoom,
                };
                match zoom {
                    0 => self.zoom[0],
                    2 => self.zoom[1],
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sounds() -> UseItemSounds {
        UseItemSounds {
            seeker: Some(1),
            bacta: Some(2),
            zoom: [Some(3), Some(4)],
        }
    }

    #[test]
    fn items_sound_as_stock() {
        let sounds = sounds();
        assert_eq!(sounds.sound(46, 0, false, false, 0), Some(1));
        assert_eq!(sounds.sound(48, 0, false, false, 0), Some(2));
        assert_eq!(sounds.sound(49, 0, true, true, 0), Some(2));
        // Shield, sentry, jetpack and the rest are silent.
        assert_eq!(sounds.sound(47, 0, true, true, 0), None);
        assert_eq!(sounds.sound(52, 0, true, true, 0), None);
    }

    #[test]
    fn binoculars_start_when_raised_and_end_when_lowered() {
        let sounds = sounds();
        // The server's 2: just raised.
        assert_eq!(sounds.sound(50, 2, true, true, 2), Some(3));
        // Its 1: just put down.
        assert_eq!(sounds.sound(50, 1, true, true, 0), Some(4));
        // Without a parameter, the mode at the time.
        assert_eq!(sounds.sound(50, 0, true, true, 0), Some(3));
        assert_eq!(sounds.sound(50, 0, true, true, 1), None);
        // Only the local player's, and only with the weapon ready.
        assert_eq!(sounds.sound(50, 2, false, true, 2), None);
        assert_eq!(sounds.sound(50, 2, true, false, 2), None);
    }
}
