//! The bars stock draws under the crosshair (`CG_DrawCrosshair`, `cg_draw.c:8252-8281`): the
//! health of what is under it (`CG_DrawHealthBar`); in Siege, for a class that views stats,
//! a teammate's health and ammunition (`CG_DrawSiegeInfo`, from `sxd`); the health of the
//! vehicle a pilot under it sits on; and a hack's progress (`CG_DrawHaqrBar`). Each is 50 by
//! 5 units of 480 lines, stacked down from 8 units below the crosshair, and shows only
//! while the crosshair does. The hack bar carries the technician's icon above it.

use super::*;
use crate::crosshair_scan::Target;
use jkr_client::siege_extended::SiegeExtended;
use jkr_protocol::{EntityState, Snapshot};
use jkr_ui::{Color, DrawCommand, DrawList, Rect};

/// `HEALTH_WIDTH`, `HEALTH_HEIGHT`.
const WIDTH: f32 = 50.0;
const HEIGHT: f32 = 5.0;
/// The bar colours (`aColor`) and the rest's (`cColor`).
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 0.4];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 0.4];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 0.4];
const GREY: [f32; 4] = [0.5, 0.5, 0.5, 0.4];
const FAINT_GREY: [f32; 4] = [0.5, 0.5, 0.5, 0.1];
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
const ET_NPC: u8 = 13;
const CLASS_VEHICLE: u8 = 53;
/// `s.teamowner`, `s.bolt1`.
const ES_TEAM_OWNER: usize = 21;
const ES_BOLT1: usize = 91;
/// `ps.hackingTime`, `ps.hackingBaseTime`.
const PS_HACKING_TIME: usize = 91;
const PS_HACKING_BASE_TIME: usize = 124;
const CS_PLAYERS: usize = 1_131;

/// One bar: how far down, how full and in which colours.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bar {
    /// Units below the crosshair's bottom edge.
    offset: f32,
    /// The filled part, from 0 to 1.
    fill: f32,
    color: [f32; 4],
    rest: [f32; 4],
    /// The hack bar, under `hackerIconShader`.
    hack: bool,
}

/// This frame's bars.
#[derive(Default)]
pub(super) struct State {
    bars: [Option<Bar>; 4],
    /// `cg_crosshairSize`.
    size: f32,
    /// The classes that view stats (`CFL_STATVIEWER`), read for the level named first.
    stat_viewers: Option<(Vec<u8>, Vec<String>)>,
}

/// What one frame of the bars reads.
pub(crate) struct CrosshairBarInputs<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) game: &'a GameState,
    pub(crate) target: Option<Target>,
    /// `sxd`'s reports; none in a demo.
    pub(crate) extended: Option<&'a SiegeExtended>,
    /// `cg.time`.
    pub(crate) time: i32,
    /// `cg_drawPlayerNames` below 2, without which an entity's health bar is left out.
    pub(crate) entity_health: bool,
    /// `cg_crosshairSize`, in the crosshair's own units.
    pub(crate) size: f32,
    pub(crate) vfs: Option<&'a jkr_vfs::VirtualFileSystem>,
}

impl HudOverlay {
    /// Which bars show under the crosshair this frame.
    pub(crate) fn update_crosshair_bars(&mut self, input: CrosshairBarInputs<'_>) {
        let state = &mut self.crosshair_bars;
        state.size = input.size;
        let gametype = info(input.game, 0, "g_gametype").unwrap_or(0);
        let entities = &input.snapshot.entities;
        let entity = |number: u16| {
            entities
                .binary_search_by_key(&number, EntityState::number)
                .ok()
                .map(|index| &entities[index])
        };
        let player = &input.snapshot.player;
        let team = i32::from(player.team());
        let mut bars = [None; 4];
        let mut count = 0;
        let mut push = |bar: Bar| {
            if let Some(slot) = bars.get_mut(count) {
                *slot = Some(bar);
                count += 1;
            }
        };
        // How far `chY` has moved down, as each kind of bar moves it.
        let mut moved = 0.0;
        if let Some(target) = input.target
            && let Some(cross) = entity(target.entity)
        {
            if cross.max_health() != 0
                && cross.raw_field(ES_BOLT1) != Some(2)
                && input.entity_health
            {
                if let Some(bar) = health_bar(cross, team, gametype, moved) {
                    push(bar);
                }
                moved += HEIGHT * 2.0;
            } else if target.entity < 32 {
                if gametype == GT_SIEGE {
                    let viewer = state.stat_viewer(input.game, player.client_num(), input.vfs);
                    let teammate = info(input.game, CS_PLAYERS + usize::from(target.entity), "t")
                        == Some(team);
                    if let Some([health, ammo]) = input
                        .extended
                        .filter(|_| viewer && teammate)
                        .and_then(|data| {
                            data.bars(target.entity, cross.weapon(), cross.e_flags(), input.time)
                        })
                    {
                        push(Bar {
                            offset: moved + 8.0,
                            fill: health,
                            color: GREEN,
                            rest: GREY,
                            hack: false,
                        });
                        push(Bar {
                            offset: moved + HEIGHT + 10.0,
                            fill: ammo,
                            color: YELLOW,
                            rest: GREY,
                            hack: false,
                        });
                    }
                    moved += HEIGHT * 4.0;
                }
                if let Some(vehicle) = target.vehicle.and_then(entity).filter(|vehicle| {
                    vehicle.entity_type() == ET_NPC
                        && vehicle.npc_class() == CLASS_VEHICLE
                        && vehicle.max_health() != 0
                }) {
                    if let Some(bar) = health_bar(vehicle, team, gametype, moved) {
                        push(bar);
                    }
                    moved += HEIGHT * 2.0;
                }
            }
        }
        // `CG_DrawHaqrBar`: the hack's time left of its whole.
        let hacking = player.raw_field(PS_HACKING_TIME).unwrap_or(0) as i32;
        let whole = player.raw_field(PS_HACKING_BASE_TIME).unwrap_or(0) as i32;
        if hacking != 0 && whole != 0 {
            let fill = hacking.wrapping_sub(input.time) as f32 / whole as f32;
            if (1.0 / WIDTH..=1.0).contains(&fill) {
                push(Bar {
                    offset: moved + 8.0,
                    fill,
                    color: YELLOW,
                    rest: FAINT_GREY,
                    hack: true,
                });
            }
        }
        state.bars = bars;
    }
}

impl State {
    /// The bars under the crosshair as `crosshair` places it: its offset from the middle of
    /// the screen in screen fractions (y up) and the lines its size counts on.
    pub(super) fn emit(
        &self,
        list: &mut DrawList,
        viewport: [f32; 2],
        crosshair: [f32; 4],
        hacker: Option<jkr_ui::TextureId>,
    ) {
        let [x, y, _, dimensions] = crosshair;
        let (offset, size) = ([x, y], self.size);
        if size <= 0.0 || dimensions <= 0.0 {
            return;
        }
        let unit = viewport[1] / 480.0;
        let centre = [
            (0.5 + offset[0]) * viewport[0],
            (0.5 - offset[1]) * viewport[1],
        ];
        let bottom = centre[1] + size * 0.5 * viewport[1] / dimensions;
        let left = centre[0] - WIDTH * 0.5 * unit;
        let color = |[r, g, b, a]: [f32; 4]| Color::new(r, g, b, a);
        for bar in self.bars.iter().flatten() {
            let top = bottom + bar.offset * unit;
            let filled = bar.fill * WIDTH;
            // `CG_DrawRect` one unit wide, then the two `CG_FillRect`s inside it.
            let _ = list.push(DrawCommand::Border {
                rect: Rect::new(left, top, WIDTH * unit, HEIGHT * unit),
                radius: 0.0,
                width: unit,
                color: Color::new(0.0, 0.0, 0.0, 1.0),
            });
            let _ = list.push(DrawCommand::SolidRect {
                rect: Rect::new(
                    left + unit,
                    top + unit,
                    (filled - 1.0).max(0.0) * unit,
                    (HEIGHT - 1.0) * unit,
                ),
                color: color(bar.color),
            });
            let _ = list.push(DrawCommand::SolidRect {
                rect: Rect::new(
                    left + filled * unit,
                    top + unit,
                    (WIDTH - filled - 1.0).max(0.0) * unit,
                    (HEIGHT - 1.0) * unit,
                ),
                color: color(bar.rest),
            });
            if bar.hack
                && let Some(texture) = hacker
            {
                let _ = list.push(DrawCommand::TexturedQuad {
                    rect: Rect::new(left, top - WIDTH * unit, WIDTH * unit, WIDTH * unit),
                    texture,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                });
            }
        }
    }

    /// Whether the local player's `siegeclass` views stats, its class files read once per
    /// level (`BG_SiegeFindClassByName`).
    fn stat_viewer(
        &mut self,
        game: &GameState,
        client: u16,
        vfs: Option<&jkr_vfs::VirtualFileSystem>,
    ) -> bool {
        let map = game
            .config_string(0)
            .and_then(|info| jkr_client::LegacyClientInfo::new(info).bytes("mapname"))
            .unwrap_or_default();
        if self
            .stat_viewers
            .as_ref()
            .is_none_or(|(known, _)| known != map)
        {
            let names = vfs.map(stat_viewer_classes).unwrap_or_default();
            self.stat_viewers = Some((map.to_vec(), names));
        }
        let Some(class) = game
            .config_string(CS_PLAYERS + usize::from(client))
            .and_then(|info| jkr_client::LegacyClientInfo::new(info).bytes("siegeclass"))
            .and_then(|name| std::str::from_utf8(name).ok())
        else {
            return false;
        };
        self.stat_viewers
            .as_ref()
            .is_some_and(|(_, names)| names.iter().any(|name| name.eq_ignore_ascii_case(class)))
    }
}

/// `CG_DrawHealthBar` for `entity`: its health of its maximum, yellow when no team owns it
/// (or the game has no teams), green for the viewer's team, red for the other; nothing
/// once it has none.
fn health_bar(entity: &EntityState, team: i32, gametype: i32, moved: f32) -> Option<Bar> {
    let fill = entity.health() as f32 / entity.max_health() as f32;
    if fill <= 0.0 {
        return None;
    }
    let owner = entity.raw_field(ES_TEAM_OWNER).unwrap_or(0) as i32;
    let color = if owner == 0 || gametype < GT_TEAM {
        YELLOW
    } else if owner == team {
        GREEN
    } else {
        RED
    };
    Some(Bar {
        offset: moved + 8.0,
        fill: fill.min(1.0),
        color,
        rest: GREY,
        hack: false,
    })
}

/// The names of the classes whose `classflags` carry `CFL_STATVIEWER`.
fn stat_viewer_classes(vfs: &jkr_vfs::VirtualFileSystem) -> Vec<String> {
    let directory = "ext_data/Siege/Classes";
    vfs.list_files(directory, ".scl")
        .into_iter()
        .filter_map(|file| {
            let asset = vfs.read(&format!("{directory}/{file}")).ok().flatten()?;
            jkr_game_jka::siege_class::parse(&String::from_utf8_lossy(&asset.bytes)).ok()
        })
        .filter(|class| class.class_flags & (1 << jkr_game_jka::siege_class::CFL_STATVIEWER) != 0)
        .map(|class| class.name)
        .collect()
}

/// An integer key of configstring `index`.
fn info(game: &GameState, index: usize, key: &str) -> Option<i32> {
    game.config_string(index)
        .and_then(|info| jkr_client::LegacyClientInfo::new(info).integer(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(health: u32, max: u32, owner: u32) -> EntityState {
        let mut state = EntityState::zero(40, &jkr_protocol::LEGACY_ENTITY_FIELDS);
        state.set_raw_field(69, health);
        state.set_raw_field(73, max);
        state.set_raw_field(ES_TEAM_OWNER, owner);
        state
    }

    #[test]
    fn an_entity_bar_takes_its_owners_colour() {
        let bar = |owner, gametype| health_bar(&entity(30, 120, owner), 1, gametype, 0.0);
        assert_eq!(bar(0, GT_TEAM).map(|bar| bar.color), Some(YELLOW));
        assert_eq!(bar(1, GT_TEAM).map(|bar| bar.color), Some(GREEN));
        assert_eq!(bar(2, GT_TEAM).map(|bar| bar.color), Some(RED));
        assert_eq!(bar(2, 0).map(|bar| bar.color), Some(YELLOW));
        assert_eq!(
            bar(0, 0).map(|bar| (bar.fill, bar.offset)),
            Some((0.25, 8.0))
        );
        assert_eq!(health_bar(&entity(0, 120, 0), 1, 0, 0.0), None);
    }

    #[test]
    fn bars_sit_under_the_crosshair() {
        let mut state = State {
            size: 24.0,
            ..State::default()
        };
        state.bars[0] = Some(Bar {
            offset: 8.0,
            fill: 0.5,
            color: GREEN,
            rest: GREY,
            hack: false,
        });
        let mut list = DrawList::new(8);
        // A 24-unit crosshair in the middle of a 640 by 480 screen.
        let technician = Some(jkr_ui::TextureId(7));
        state.emit(&mut list, [640.0, 480.0], [0.0, 0.0, 640.0, 480.0], technician);
        match list.commands() {
            [
                DrawCommand::Border { rect: outline, .. },
                DrawCommand::SolidRect { rect: full, .. },
                DrawCommand::SolidRect { rect: rest, .. },
            ] => {
                assert_eq!(*outline, Rect::new(295.0, 260.0, 50.0, 5.0));
                assert_eq!(*full, Rect::new(296.0, 261.0, 24.0, 4.0));
                assert_eq!(*rest, Rect::new(320.0, 261.0, 24.0, 4.0));
            }
            commands => panic!("unexpected commands: {commands:?}"),
        }
        // The hack bar carries the technician a bar's width square above it.
        state.bars[0] = state.bars[0].map(|bar| Bar { hack: true, ..bar });
        let mut list = DrawList::new(8);
        state.emit(&mut list, [640.0, 480.0], [0.0, 0.0, 640.0, 480.0], technician);
        match list.commands().last() {
            Some(DrawCommand::TexturedQuad { rect, texture, .. }) => {
                assert_eq!(*rect, Rect::new(295.0, 210.0, 50.0, 50.0));
                assert_eq!(Some(*texture), technician);
            }
            commands => panic!("unexpected commands: {commands:?}"),
        }
    }
}
