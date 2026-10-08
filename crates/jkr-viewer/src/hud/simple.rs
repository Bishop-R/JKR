//! Stock's simple HUD (`cg_hudFiles` a number but 0; `CG_DrawHUD`'s first branch,
//! `cg_draw.c:1331-1352`): in place of the status widgets, numbers in the small font with
//! a shadow in the bottom corners of the 640 by 480 screen — health (16, 440) and armour
//! (32, 454) at the left, Force (576, 454) and ammunition (592, 440) at the right — or,
//! with a saber, the stance's name (`CG_DrawSimpleSaberStyle`). The ammunition is grey
//! while firing, yellow for 200 ms after it grew, red at none and "--" for a weapon
//! without (`CG_DrawSimpleAmmo`); the Force is red while it flashes
//! (`CG_DrawSimpleForcePower`). The left items keep to the left edge and the right ones
//! to the right, as EternalJK's wide-screen HUD keeps them. EternalJK's and jaPRO's other
//! HUDs (`cg_hudFiles` 2 to 4, from their own files) are not drawn: like stock, any number
//! but 0 draws this one.

use super::*;
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign, TextOverflow};

/// `colorTable` (`q_color.c`): `CT_HUD_RED`, `CT_HUD_GREEN`, `CT_ICON_BLUE`,
/// `CT_HUD_ORANGE`, `CT_YELLOW`, `CT_LTGREY`, `CT_RED`.
const HUD_RED: [f32; 4] = [0.835, 0.015, 0.015, 1.0];
const HUD_GREEN: [f32; 4] = [0.0, 0.613, 0.097, 1.0];
const ICON_BLUE: [f32; 4] = [0.567, 0.685, 1.0, 0.75];
const HUD_ORANGE: [f32; 4] = [1.0, 0.658, 0.062, 1.0];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
const LIGHT_GREY: [f32; 4] = [0.75, 0.75, 0.75, 1.0];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
/// `ocr_a`'s line (`UI_SMALLFONT`) and its shadow's offset (`RE_Font_DrawString`'s
/// `0.075` of the 18-point size), in 640 by 480 units.
const LINE: f32 = 21.0;
const SHADOW: f32 = 1.0;
/// How long grown ammunition shows yellow (`cg.oldAmmoTime`).
const GROWN_MS: u64 = 200;
/// The stance name's text id.
pub(super) const STYLE_TEXT: u32 = 336;

/// The right-hand readout and the ammunition's colour state.
#[derive(Default)]
pub(crate) struct SimpleHud {
    /// The stance's name, colour and distance from the right edge, with a saber.
    style: Option<(&'static str, [f32; 4], f32)>,
    ammo_color: [f32; 4],
    /// `cg.oldammo`.
    old_ammo: i32,
    grown_until: u64,
}

/// `CG_DrawSimpleSaberStyle`'s names and colours, by `saberDrawAnimLevel`; the fast
/// stance (and any unknown one) sits 16 units further right.
fn style(level: u8) -> (&'static str, [f32; 4], f32) {
    match level {
        2 => ("MEDIUM", YELLOW, 64.0),
        3 => ("STRONG", HUD_RED, 64.0),
        4 => ("DESANN", HUD_RED, 64.0),
        5 => ("TAVION", ICON_BLUE, 64.0),
        6 => ("AKIMBO", HUD_ORANGE, 64.0),
        7 => ("STAFF", HUD_ORANGE, 64.0),
        _ => ("FAST", ICON_BLUE, 48.0),
    }
}

impl SimpleHud {
    /// The readouts for `values`; `firing` when the predicted weapon fires
    /// (`WEAPON_FIRING` with more than 100 ms to go).
    pub(super) fn update(&mut self, values: &ClientHudData, firing: bool, time_ms: u64) {
        self.style = values.saber_style.map(style);
        self.ammo_color = match values.ammo {
            _ if self.style.is_some() => HUD_ORANGE,
            None => HUD_ORANGE,
            Some(ammo) => {
                if self.old_ammo < ammo {
                    self.grown_until = time_ms + GROWN_MS;
                }
                self.old_ammo = ammo;
                if firing {
                    LIGHT_GREY
                } else if ammo <= 0 {
                    RED
                } else if time_ms < self.grown_until {
                    YELLOW
                } else {
                    HUD_ORANGE
                }
            }
        };
    }

    /// The stance's name, for [`STYLE_TEXT`].
    pub(super) fn style_text(&self) -> &'static str {
        self.style.map_or("", |(name, _, _)| name)
    }

    /// Draw it, the Force red while `force_red`.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2], force_red: bool) {
        let [w, h] = viewport;
        let s = h / 480.0;
        let (right, right_color) = match self.style {
            Some((_, color, from_right)) => ((STYLE_TEXT, from_right), color),
            None => ((14, 48.0), self.ammo_color),
        };
        let force = if force_red { RED } else { ICON_BLUE };
        for (text, x, y, color) in [
            (6, 16.0 * s, 440.0, HUD_RED),
            (8, 32.0 * s, 454.0, HUD_GREEN),
            (10, w - 64.0 * s, 454.0, force),
            (right.0, w - right.1 * s, 440.0, right_color),
        ] {
            let [r, g, b, a] = color;
            for (offset, color) in [
                (SHADOW * s, Color::new(0.0, 0.0, 0.0, a)),
                (0.0, Color::new(r, g, b, a)),
            ] {
                let _ = list.push(DrawCommand::Text {
                    rect: Rect::new(x + offset, y * s + offset, 160.0 * s, LINE * s),
                    text: TextId(text),
                    size: LINE * s,
                    color,
                    align: TextAlign::Start,
                    overflow: TextOverflow::Clip,
                    weight: FontWeight::Regular,
                    letter_spacing: 0.0,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(ammo: Option<i32>, saber_style: Option<u8>) -> ClientHudData {
        ClientHudData {
            health: 100,
            armor: 0,
            force: 100,
            weapon: if saber_style.is_some() { 3 } else { 4 },
            ammo,
            saber_style,
        }
    }

    #[test]
    fn ammunition_and_stance_colours_follow_stock() {
        let mut hud = SimpleHud::default();
        // `cg.oldammo` starts at 0: the first ammunition seen shows yellow.
        hud.update(&values(Some(50), None), false, 1_000);
        assert_eq!(hud.ammo_color, YELLOW);
        hud.update(&values(Some(49), None), false, 1_200);
        assert_eq!(hud.ammo_color, HUD_ORANGE);
        hud.update(&values(Some(48), None), true, 1_300);
        assert_eq!(hud.ammo_color, LIGHT_GREY);
        hud.update(&values(Some(0), None), false, 1_400);
        assert_eq!(hud.ammo_color, RED);
        hud.update(&values(None, Some(1)), false, 1_500);
        assert_eq!(hud.style_text(), "FAST");
        hud.update(&values(None, Some(6)), false, 1_600);
        assert_eq!(hud.style, Some(("AKIMBO", HUD_ORANGE, 64.0)));
    }
}
