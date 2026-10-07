//! The meters stock draws at the right edge (`CG_Draw2D`, `cg_draw.c:11569-11586`): the
//! jetpack's fuel while it is not full (`CG_DrawJetpackFuel`), the cloak's
//! (`CG_DrawCloakFuel`), and the health of the E-Web the player mans (`CG_DrawEWebHealth`).
//! Each is a column 20 by 100 units of 480 lines, filled from the bottom, 8 units in from
//! the right edge; each meter shown moves the ones after it a column to the left.

use super::*;
use jkr_protocol::{EntityState, PlayerState};
use jkr_ui::{Color, DrawCommand, DrawList, Rect};

/// `JPFUELBAR_W`, `JPFUELBAR_H`, the right margin, and the step to the next column.
const WIDTH: f32 = 20.0;
const HEIGHT: f32 = 100.0;
const MARGIN: f32 = 8.0;
const STEP: f32 = WIDTH + 8.0;
/// `ps.jetpackFuel`, `ps.cloakFuel`.
const PS_JETPACK_FUEL: usize = 39;
const PS_CLOAK_FUEL: usize = 40;
/// The fuel meters' and the E-Web meter's tops (`JPFUELBAR_Y`, `EWEBHEALTH_Y`).
const FUEL_TOP: f32 = 260.0;
const EWEB_TOP: f32 = 290.0;
/// The bars' colours and their empty parts'.
const DARK_RED: [f32; 4] = [0.5, 0.0, 0.0, 0.8];
const GREY: [f32; 4] = [0.5, 0.5, 0.5, 0.1];
const BLUE: [f32; 4] = [0.0, 0.0, 0.6, 0.8];
const DARK_BLUE: [f32; 4] = [0.1, 0.1, 0.3, 0.1];

/// One meter: its column, top, how full and its colours.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Meter {
    /// Columns left of the rightmost.
    column: f32,
    top: f32,
    fill: f32,
    color: [f32; 4],
    rest: [f32; 4],
}

/// This frame's meters.
#[derive(Default)]
pub(super) struct State {
    meters: [Option<Meter>; 3],
}

impl HudOverlay {
    /// Which meters show for `player`, the E-Web's health read from `entities`.
    pub(crate) fn update_meters(&mut self, player: &PlayerState, entities: &[EntityState]) {
        let fuel = |index: usize| player.raw_field(index).unwrap_or(0) as i32;
        let (jetpack, cloak) = (fuel(PS_JETPACK_FUEL), fuel(PS_CLOAK_FUEL));
        let mut column = 0.0;
        let mut meter = |fill: f32, top: f32, color, rest| {
            let shown = fill <= 1.0;
            let meter = shown.then_some(Meter {
                column,
                top,
                fill: fill.max(0.001),
                color,
                rest,
            });
            if shown {
                column += 1.0;
            }
            meter
        };
        let jetpack = (jetpack < 100)
            .then(|| meter(jetpack as f32 / 100.0, FUEL_TOP, DARK_RED, GREY))
            .flatten();
        let cloak = (cloak < 100)
            .then(|| meter(cloak as f32 / 100.0, FUEL_TOP, BLUE, DARK_BLUE))
            .flatten();
        // An E-Web carries no weapon of its own; an emplaced gun does.
        let eweb = match player.emplaced_index() {
            0 => None,
            index => entities
                .binary_search_by_key(&index, EntityState::number)
                .ok()
                .map(|found| &entities[found])
                .filter(|eweb| eweb.weapon() == 0)
                .and_then(|eweb| {
                    let health = eweb.health() as f32 / eweb.max_health() as f32;
                    meter(health, EWEB_TOP, DARK_RED, GREY)
                }),
        };
        self.meters.meters = [jetpack, cloak, eweb];
    }
}

impl State {
    /// The meters in 480-line units from the right edge of `viewport`.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2]) {
        let unit = viewport[1] / 480.0;
        let color = |[r, g, b, a]: [f32; 4]| Color::new(r, g, b, a);
        for meter in self.meters.iter().flatten() {
            let left = viewport[0] - (MARGIN + WIDTH + meter.column * STEP) * unit;
            let top = meter.top * unit;
            let filled = (meter.fill * HEIGHT).max(0.1);
            // `CG_DrawRect` one unit wide, the full part at the bottom, the empty part above.
            let _ = list.push(DrawCommand::Border {
                rect: Rect::new(left, top, WIDTH * unit, HEIGHT * unit),
                radius: 0.0,
                width: unit,
                color: Color::new(0.0, 0.0, 0.0, 1.0),
            });
            let _ = list.push(DrawCommand::SolidRect {
                rect: Rect::new(
                    left + unit,
                    top + (1.0 + HEIGHT - filled) * unit,
                    (WIDTH - 1.0) * unit,
                    (filled - 1.0).max(0.0) * unit,
                ),
                color: color(meter.color),
            });
            let _ = list.push(DrawCommand::SolidRect {
                rect: Rect::new(
                    left + unit,
                    top + unit,
                    (WIDTH - 1.0) * unit,
                    (HEIGHT - filled) * unit,
                ),
                color: color(meter.rest),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meters_stack_leftward_from_the_right_edge() {
        let state = State {
            meters: [
                Some(Meter {
                    column: 0.0,
                    top: FUEL_TOP,
                    fill: 0.25,
                    color: DARK_RED,
                    rest: GREY,
                }),
                None,
                Some(Meter {
                    column: 1.0,
                    top: EWEB_TOP,
                    fill: 1.0,
                    color: DARK_RED,
                    rest: GREY,
                }),
            ],
        };
        let mut list = DrawList::new(8);
        state.emit(&mut list, [640.0, 480.0]);
        let rects: Vec<Rect> = list
            .commands()
            .iter()
            .map(|command| match command {
                DrawCommand::Border { rect, .. } | DrawCommand::SolidRect { rect, .. } => *rect,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(
            rects,
            [
                Rect::new(612.0, 260.0, 20.0, 100.0),
                Rect::new(613.0, 336.0, 19.0, 24.0),
                Rect::new(613.0, 261.0, 19.0, 75.0),
                Rect::new(584.0, 290.0, 20.0, 100.0),
                Rect::new(585.0, 291.0, 19.0, 99.0),
                Rect::new(585.0, 291.0, 19.0, 0.0),
            ]
        );
    }
}
