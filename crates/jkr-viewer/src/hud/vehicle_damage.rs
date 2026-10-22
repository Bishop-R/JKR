//! A vehicle's damage HUDs (`CG_DrawVehicleDamageHUD`, `CG_DrawVehicleDamage`,
//! `CG_CheckTargetVehicle`, `cg_draw.c:2507-2665`, `2760-2768`): piloting a vehicle that
//! hides its rider, its own (the `vehicledamagehud` menu, at 5, 378 of the 640 by 480
//! screen) and the targeted fighter's (`enemyvehicledamagehud`, at 535, 378) — the
//! vehicle's background picture, red while its shields take a hit and back to its colour
//! over two seconds, its frame, its shields in green as strong as they are, and its front,
//! back, left and right in green, yellow when lightly damaged, red when heavily and dark
//! grey when both (`brokenLimbs`, `SHIPSURF_DAMAGE_*`). The target is the rocket lock's,
//! else the vehicle under the crosshair in the last three seconds, else the vehicle of the
//! player under it, if it is a fighter; once lost it fades over three seconds. Each
//! vehicle names its own pictures (`dmgIndic*`, `icon_*`): the fighters do, the AT-ST and
//! the speeders do not and show nothing.

use jkr_ui::{Color, DrawCommand, DrawList, Rect, TextureId};

/// Each panel's seven atlas cells: past `icon_assets::COUNT` and the duel portraits.
const FIRST_CELL: [u32; 2] = [127, 134];
/// The panels' places on the 640 by 480 screen: left edge for the own, right for the
/// target's.
const OWN_X: f32 = 5.0;
const ENEMY_FROM_RIGHT: f32 = 105.0;
const TOP: f32 = 378.0;
const SIZE: f32 = 100.0;
/// How long a lost target fades, and a crosshair vehicle counts (`cg_targVehLastTime`).
const FADE_MS: i32 = 3_000;
/// `MIN_SHIELD_TIME`.
const SHIELD_MS: f32 = 2_000.0;
/// Pictures, in `VehicleInfo::damage_pictures` order.
const FRAME: usize = 0;
const SHIELD: usize = 1;
const BACKGROUND: usize = 2;
/// The front, back, left and right pictures and their light and heavy damage bits.
const PARTS: [(usize, u8, u8); 4] = [(3, 0, 4), (4, 1, 5), (5, 3, 7), (6, 2, 6)];

/// One panel this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Panel {
    /// The vehicle's definition template, whose pictures it draws.
    pub(crate) template: usize,
    /// `brokenLimbs`.
    pub(crate) broken: u8,
    /// The shields' strength, 0 to 1.
    pub(crate) shields: f32,
    /// How far the background is back from red after a shield hit (`damageTime`), 0 to 1.
    pub(crate) recovered: f32,
    pub(crate) alpha: f32,
}

/// The panels and the pictures loaded for them.
#[derive(Default)]
pub(crate) struct DamageHud {
    panels: [Option<Panel>; 2],
    loaded: [Option<usize>; 2],
    present: [[bool; 7]; 2],
    /// `cg_targVeh` and `cg_targVehLastTime`.
    target: Option<(u16, i32)>,
    /// `cg.crosshairVehNum` and `cg.crosshairVehTime`.
    crosshair_vehicle: Option<(u16, i32)>,
}

/// The red flash's recovery at `time` for a vehicle hit until `until` (`damageTime`).
pub(crate) fn recovered(until: i32, time: i32) -> f32 {
    if until > time {
        (1.0 - (until - time) as f32 / SHIELD_MS).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

impl DamageHud {
    /// The vehicle a hiding pilot targets (`CG_CheckTargetVehicle`), from the rocket lock
    /// (`lock`), the vehicle under the crosshair this frame, and the vehicle of the player
    /// under it (`player_vehicle`); `is_fighter` says whether an entity is a fighter.
    /// Returns it with its alpha.
    pub(crate) fn target(
        &mut self,
        time: i32,
        lock: Option<u16>,
        crosshair_vehicle: Option<u16>,
        player_vehicle: Option<u16>,
        is_fighter: &mut dyn FnMut(u16) -> bool,
    ) -> Option<(u16, f32)> {
        if let Some(vehicle) = crosshair_vehicle {
            self.crosshair_vehicle = Some((vehicle, time));
        }
        let recent = self
            .crosshair_vehicle
            .filter(|(_, at)| time - at < FADE_MS)
            .map(|(vehicle, _)| vehicle);
        let candidate = lock.or(recent).or(player_vehicle);
        if let Some(vehicle) = candidate.filter(|vehicle| (32..1_022).contains(vehicle))
            && is_fighter(vehicle)
        {
            self.target = Some((vehicle, time));
            return Some((vehicle, 1.0));
        }
        let (vehicle, last) = self.target?;
        (time - last < FADE_MS).then(|| (vehicle, 1.0 - (time - last) as f32 / FADE_MS as f32))
    }

    /// This frame's own panel and target panel.
    pub(crate) fn set(&mut self, own: Option<Panel>, enemy: Option<Panel>) {
        self.panels = [own, enemy];
    }

    /// The panels whose template's pictures are not loaded yet.
    pub(crate) fn wanted(&self) -> [Option<usize>; 2] {
        std::array::from_fn(|panel| {
            self.panels[panel]
                .map(|panel| panel.template)
                .filter(|template| self.loaded[panel] != Some(*template))
        })
    }

    /// Load `pictures` (`VehicleInfo::damage_pictures`) for `panel` as template `template`.
    pub(crate) fn load(
        &mut self,
        panel: usize,
        template: usize,
        pictures: &[Option<Vec<u8>>; 7],
        vfs: &jkr_vfs::VirtualFileSystem,
        shaders: &jkr_shader::ShaderCatalog,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        self.loaded[panel] = Some(template);
        for (slot, name) in pictures.iter().enumerate() {
            let texture =
                TextureId(crate::ui_renderer::ICON_CELLS + FIRST_CELL[panel] + slot as u32);
            self.present[panel][slot] = name
                .as_deref()
                .and_then(|name| std::str::from_utf8(name).ok())
                .and_then(|name| super::icons::assets::decode(vfs, shaders, name))
                .map(|pixels| renderer.upload_icon(queue, texture, pixels.as_raw()))
                .is_some();
        }
    }

    /// Draw them, the own panel from the left edge and the target's from the right.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2]) {
        let s = viewport[1] / 480.0;
        for (index, panel) in self.panels.iter().enumerate() {
            let Some(panel) = panel else {
                continue;
            };
            if self.loaded[index] != Some(panel.template) {
                continue;
            }
            let x = if index == 0 {
                OWN_X * s
            } else {
                viewport[0] - ENEMY_FROM_RIGHT * s
            };
            let rect = Rect::new(x, TOP * s, SIZE * s, SIZE * s);
            let mut picture = |slot: usize, [r, g, b, a]: [f32; 4]| {
                if self.present[index][slot] {
                    let _ = list.push(DrawCommand::TexturedQuad {
                        rect,
                        texture: TextureId(
                            crate::ui_renderer::ICON_CELLS + FIRST_CELL[index] + slot as u32,
                        ),
                        color: Color::new(r, g, b, a),
                    });
                }
            };
            // The menu items' colours are white; the flash keeps red and fades the rest in.
            let recovered = panel.recovered;
            picture(BACKGROUND, [1.0, recovered, recovered, 1.0]);
            picture(FRAME, [1.0, 1.0, 1.0, 1.0]);
            // `CT_HUD_GREEN` as strong as the shields.
            picture(SHIELD, [0.0, 0.613, 0.097, panel.shields.clamp(0.0, 1.0)]);
            for (slot, light, heavy) in PARTS {
                let broken = |bit: u8| panel.broken & (1 << bit) != 0;
                let [r, g, b] = match (broken(heavy), broken(light)) {
                    (true, true) => [0.25, 0.25, 0.25],
                    (true, false) => [1.0, 0.0, 0.0],
                    (false, true) => [1.0, 1.0, 0.0],
                    (false, false) => [0.0, 1.0, 0.0],
                };
                picture(slot, [r, g, b, panel.alpha]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_holds_three_seconds_and_fades() {
        let mut hud = DamageHud::default();
        let mut fighters = |vehicle: u16| vehicle == 40;
        assert_eq!(hud.target(0, None, None, None, &mut fighters), None);
        assert_eq!(
            hud.target(1_000, Some(40), None, None, &mut fighters),
            Some((40, 1.0))
        );
        // A crosshair vehicle counts for three seconds.
        assert_eq!(
            hud.target(2_000, None, Some(40), None, &mut fighters),
            Some((40, 1.0))
        );
        assert_eq!(
            hud.target(4_900, None, None, None, &mut fighters),
            Some((40, 1.0))
        );
        // Then the lost target fades.
        let (_, alpha) = hud.target(6_400, None, None, None, &mut fighters).unwrap();
        assert!((alpha - 0.5).abs() < 0.05);
        assert_eq!(hud.target(8_000, None, None, None, &mut fighters), None);
        // Not a fighter: nothing new.
        assert_eq!(hud.target(9_000, Some(41), None, None, &mut fighters), None);
        assert!((recovered(10_000, 9_000) - 0.5).abs() < 1e-6);
    }
}
