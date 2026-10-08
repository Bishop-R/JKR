//! What picking an item up shows (`CG_ItemPickup`, `cg_event.c:597-600`): the item's
//! icon at the right for three seconds, fading over the last 200 ms (`CG_DrawPickupItem`,
//! `CG_FadeColor`), and the crosshair swelling to twice its size over 200 ms before it
//! snaps back (`ITEM_BLOB_TIME`, `CG_DrawCrosshair`).

const SHOW_MS: u64 = 3_000;
const FADE_MS: u64 = 200;
const BLOB_MS: u64 = 200;

/// The last item picked up and when the HUD first saw it.
#[derive(Default)]
pub(crate) struct Pickup {
    item: u16,
    pending: bool,
    at: Option<u64>,
}

impl Pickup {
    /// The local player picked up `item` (its `bg_itemlist` index).
    pub(crate) fn picked_up(&mut self, item: u16) {
        self.item = item;
        self.pending = true;
    }

    /// Stamp a new pickup with the HUD's clock.
    pub(super) fn update(&mut self, time_ms: u64) {
        if std::mem::take(&mut self.pending) {
            self.at = Some(time_ms);
        }
    }

    /// The item whose icon shows at `time_ms`, and its opacity.
    pub(super) fn icon(&self, time_ms: u64) -> Option<(u16, f32)> {
        let age = time_ms.checked_sub(self.at?)?;
        (age < SHOW_MS).then(|| {
            (
                self.item,
                ((SHOW_MS - age) as f32 / FADE_MS as f32).min(1.0),
            )
        })
    }

    /// The crosshair's size factor at `time_ms`.
    pub(super) fn crosshair_scale(&self, time_ms: u64) -> f32 {
        self.at
            .and_then(|at| time_ms.checked_sub(at))
            .filter(|age| (1..BLOB_MS).contains(age))
            .map_or(1.0, |age| 1.0 + age as f32 / BLOB_MS as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pickup_swells_the_crosshair_and_shows_its_icon() {
        let mut pickup = Pickup::default();
        assert_eq!(pickup.icon(0), None);
        pickup.picked_up(31);
        pickup.update(1_000);
        assert_eq!(pickup.crosshair_scale(1_000), 1.0);
        assert_eq!(pickup.crosshair_scale(1_100), 1.5);
        assert_eq!(pickup.crosshair_scale(1_200), 1.0);
        assert_eq!(pickup.icon(1_100), Some((31, 1.0)));
        assert_eq!(pickup.icon(3_900), Some((31, 0.5)));
        assert_eq!(pickup.icon(4_000), None);
    }
}
