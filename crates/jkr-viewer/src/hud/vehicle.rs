//! The vehicle HUD (`CG_DrawVehicleHud`, `cg_draw.c:2740-2803`), from the `swoopvehiclehud`
//! menu of `ui/hud.menu` (its items, 640 by 480, from 0, 400): two frames; armour as 12
//! ticks along the top (`CG_DrawVehicleArmor`, the vehicle's health over its `armor`);
//! below, speed (`CG_DrawVehicleSpeed`, flashing light red while the turbo runs), shields
//! (`CG_DrawVehicleShields`) and ammunition (`CG_DrawVehicleAmmo`, or upper and lower for a
//! second weapon, of which stock lights only four) as 5 ticks each, the last one faded by
//! how much of it is left; a turbo bar that refills red and stands green when ready, and a
//! cyan mark while the weapons are linked. A vehicle that hides its rider (the AT-ST)
//! takes the player's status HUD's place; on others both show. It shows for the vehicle
//! the player pilots, from the prediction. Not drawn: the damage HUDs of a rider-hiding
//! vehicle and its target, the empty-ammunition flash and the link sound.

use super::*;
use jkr_game_jka::vehicle_fields::VehicleInfo;
use jkr_protocol::PlayerState;
use jkr_ui::{Color, DrawCommand, DrawList, Rect};

/// The textures, in the order of [`icon_assets::VEHICLE`]'s slots.
pub(super) const TEXTURES: [&str; 7] = [
    "gfx/hud/vehicle_frame",
    "gfx/hud/vehicle_grid",
    "gfx/hud/vehicle_grid2",
    "gfx/hud/vehicle_health_tick",
    "gfx/hud/vehicle_turbo_tick",
    "gfx/hud/vehicle_armor_tick",
    "gfx/hud/vehicle_ammo_tick",
];
const FRAME: usize = 0;
const GRID: usize = 1;
const GRID2: usize = 2;
const HEALTH_TICK: usize = 3;
const TURBO_TICK: usize = 4;
const ARMOR_TICK: usize = 5;
const AMMO_TICK: usize = 6;
/// The menu's origin on the 640 by 480 screen.
const MENU_Y: f32 = 400.0;
/// `STAT_ARMOR`.
const STAT_ARMOR: usize = 5;

/// One row of ticks: its background, the first tick, the step between ticks, how many
/// parts the maximum is cut into, how many ticks are drawn, and the tick texture.
struct Row {
    background: [f32; 4],
    first: [f32; 4],
    step: f32,
    parts: usize,
    count: usize,
    tick: usize,
}

const ARMOR: Row = Row {
    background: [190.0, 32.0, 256.0, 16.0],
    first: [203.0, 32.0, 31.0, 16.0],
    step: 18.0,
    parts: 12,
    count: 12,
    tick: HEALTH_TICK,
};
const SPEED: Row = Row {
    background: [215.0, 55.0, 115.0, 16.0],
    first: [216.0, 55.0, 14.0, 16.0],
    step: 12.0,
    parts: 5,
    count: 5,
    tick: TURBO_TICK,
};
const SHIELDS: Row = Row {
    background: [286.0, 55.0, 115.0, 16.0],
    first: [287.0, 55.0, 14.0, 16.0],
    step: 12.0,
    parts: 5,
    count: 5,
    tick: ARMOR_TICK,
};
const AMMO: Row = Row {
    background: [357.0, 55.0, 115.0, 16.0],
    first: [358.0, 55.0, 14.0, 16.0],
    step: 12.0,
    parts: 5,
    count: 5,
    tick: AMMO_TICK,
};
/// Stock's loops for two weapons stop a tick short (`i < MAX_VHUD_AMMO_TICS`).
const AMMO_UPPER: Row = Row {
    background: [357.0, 55.0, 115.0, 10.0],
    first: [358.0, 55.0, 14.0, 10.0],
    step: 12.0,
    parts: 5,
    count: 4,
    tick: AMMO_TICK,
};
const AMMO_LOWER: Row = Row {
    background: [357.0, 67.0, 115.0, 10.0],
    first: [358.0, 67.0, 14.0, 10.0],
    step: 12.0,
    parts: 5,
    count: 4,
    tick: AMMO_TICK,
};

/// One row's fill: its value over its maximum, and the ticks' colour.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fill {
    value: f32,
    maximum: f32,
    color: [f32; 4],
}

/// This frame's panel, or none outside a piloted vehicle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Panel {
    shown: bool,
    hides_rider: bool,
    armor: Option<Fill>,
    speed: Option<Fill>,
    shields: Option<Fill>,
    ammo: [Option<Fill>; 2],
    two_weapons: bool,
    /// The turbo bar's fill and whether it stands ready.
    turbo: Option<(f32, bool)>,
    linked: bool,
}

/// `colorTable[CT_LTRED1]`.
const LIGHT_RED: [f32; 4] = [1.0, 0.2, 0.2, 1.0];
const WHITE: [f32; 4] = [1.0; 4];

impl Panel {
    /// The panel for the piloted vehicle (`info`, its predicted `state`, its turbo time),
    /// its shields and links from the snapshot's vehicle state `vehicle`, at `time`.
    pub(crate) fn update(
        &mut self,
        piloted: Option<(&VehicleInfo, &jkr_client::pmove::MovementState, i32)>,
        vehicle: Option<&PlayerState>,
        time: i32,
    ) {
        *self = Self::default();
        let Some((info, state, turbo_time)) = piloted else {
            return;
        };
        self.shown = true;
        self.hides_rider = info.hide_rider;
        let fill = |value: f32, maximum: f32, color| {
            (maximum > 0.0).then_some(Fill {
                value,
                maximum,
                color,
            })
        };
        self.armor = fill(state.health as f32, info.armor as f32, WHITE);
        // `VHUDFlashTime`: the speed ticks blink light red every 200 ms in turbo.
        let speed_color = if time < turbo_time && (time / 200) % 2 == 0 {
            LIGHT_RED
        } else {
            WHITE
        };
        self.speed = fill(state.speed, info.speed_max, speed_color);
        let shields = vehicle.map_or(0, |vehicle| vehicle.stats[STAT_ARMOR] as i32);
        self.shields = fill(shields as f32, info.shields as f32, WHITE);
        let [first, second] = [&info.weapons[0], &info.weapons[1]];
        self.two_weapons = first.id != 0 && second.id != 0;
        if first.id != 0 {
            self.ammo[0] = fill(state.ammo[0] as f32, first.ammo_max as f32, WHITE);
        }
        if self.two_weapons {
            self.ammo[1] = fill(state.ammo[1] as f32, second.ammo_max as f32, WHITE);
        }
        if info.turbo_recharge > 0 {
            let since = time.saturating_sub(turbo_time);
            self.turbo = Some(if since > info.turbo_recharge {
                (1.0, true)
            } else {
                ((since as f32 / info.turbo_recharge as f32).max(0.0), false)
            });
        }
        self.linked = first.linkable == 2
            || second.linkable == 2
            || vehicle.is_some_and(|vehicle| vehicle.vehicle_fields().weapons_linked);
    }

    /// Whether it takes the player's status HUD's place.
    pub(crate) fn hides_status(&self) -> bool {
        self.shown && self.hides_rider
    }

    /// Draw it, the 640 by 480 screen kept to the viewport's height and centred.
    pub(super) fn emit(&self, icons: &icons::Icons, list: &mut DrawList, viewport: [f32; 2]) {
        if !self.shown {
            return;
        }
        let scale = viewport[1] / 480.0;
        let left = (viewport[0] - 640.0 * scale) * 0.5;
        let rect = |[x, y, w, h]: [f32; 4]| {
            Rect::new(left + x * scale, (MENU_Y + y) * scale, w * scale, h * scale)
        };
        let color = |[r, g, b, a]: [f32; 4]| Color::new(r, g, b, a);
        icons.vehicle(
            list,
            FRAME,
            rect([200.0, 30.0, 16.0, 64.0]),
            color(WHITE),
            false,
        );
        icons.vehicle(
            list,
            FRAME,
            rect([420.0, 30.0, 16.0, 64.0]),
            color(WHITE),
            true,
        );
        let mut row = |row: &Row, fill: Option<Fill>| {
            let background = if row.tick == HEALTH_TICK { GRID } else { GRID2 };
            icons.vehicle(list, background, rect(row.background), color(WHITE), false);
            let Some(fill) = fill else {
                return;
            };
            let step = fill.maximum / row.parts as f32;
            let mut value = fill.value;
            for index in 0..row.count {
                if value <= 0.0 {
                    break;
                }
                let mut tint = fill.color;
                if value < step {
                    tint[3] *= value / step;
                }
                let [x, y, w, h] = row.first;
                let tick = [x + index as f32 * row.step, y, w, h];
                icons.vehicle(list, row.tick, rect(tick), color(tint), false);
                value -= step;
            }
        };
        row(&ARMOR, self.armor);
        row(&SPEED, self.speed);
        row(&SHIELDS, self.shields);
        if self.two_weapons {
            row(&AMMO_UPPER, self.ammo[0]);
            row(&AMMO_LOWER, self.ammo[1]);
        } else if self.ammo[0].is_some() {
            row(&AMMO, self.ammo[0]);
        }
        // `turborecharge` (207, 70, 3, -14) grows up from its foot.
        if let Some((fill, ready)) = self.turbo {
            let height = 14.0 * fill;
            let tint = if ready {
                [0.0, 1.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            };
            let _ = list.push(DrawCommand::SolidRect {
                rect: rect([207.0, 70.0 - height, 3.0, height]),
                color: color(tint),
            });
        }
        if self.linked {
            let _ = list.push(DrawCommand::SolidRect {
                rect: rect([426.0, 56.0, 3.0, 14.0]),
                color: color([0.0, 1.0, 1.0, 1.0]),
            });
        }
    }
}
