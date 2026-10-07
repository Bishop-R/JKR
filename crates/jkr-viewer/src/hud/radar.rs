//! The radar in the HUD's upper-right stack ([`jkr_client::radar`]): stock's picture with
//! its marks, scaled from stock's 120-unit box on a 480-line screen to [`SIZE`] HUD units.
//!
//! Each picture draws as stock's renderer draws its shader: up to two stages, each blended
//! or added and turned by `tcMod rotate`. Marks take the colour cgame gives them, as
//! TaystJK shows them (teammates green), although the arrow and goal-item stages say
//! `rgbGen identity`, which by its source OpenJK's GL renderer would draw white. Stage
//! images live in the atlas's radar cells, read again for each map.

use super::*;
use crate::ui_renderer::{self, RADAR_CELL_COUNT, RADAR_CELLS};
use jkr_client::radar::{RADAR_RADIUS, Radar, RadarEntity, RadarIcon, RadarMark, RadarViewer};
use jkr_game_jka::vehicle_parms::VehicleFiles;
use jkr_protocol::Snapshot;
use jkr_shader::{ShaderStage, StageBlend};
use jkr_ui::{Color, DrawCommand, DrawList, Rect, TextureId};

/// The radar's edge in 1080p HUD units (stock's 120 of 480 lines would be 270).
pub(super) const SIZE: f32 = 200.0;
/// The space kept below it.
const GAP: f32 = 12.0;
/// Its right edge, as the block above it has.
const RIGHT: f32 = 40.0;
/// `cgs.media.radarShader`, `mAutomapPlayerIcon`, `siegeItemShader`, `mAutomapRocketIcon`,
/// and the rocket lock's wedge and lock picture ([`super::rocket_lock`]), read with them.
const STOCK: [&str; 6] = [
    "gfx/menus/radar/radar.png",
    "gfx/menus/radar/arrow_w",
    "gfx/menus/radar/goalitem",
    "gfx/menus/radar/rocket",
    jkr_client::rocket_lock::WEDGE,
    jkr_client::rocket_lock::LOCK,
];
const CS_MODELS: usize = 298;
const CS_ICONS: usize = 1_067;
/// `PMF_FOLLOW`.
const PMF_FOLLOW: u16 = 4_096;

/// One shader stage of a picture as stock's renderer draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layer {
    texture: TextureId,
    /// `blendFunc GL_ONE GL_ONE`: added to what is below.
    additive: bool,
    /// `tcMod rotate`: degrees a second.
    spin: f32,
}

impl Layer {
    /// How the renderer draws `stage`, its image in `texture`.
    fn of(stage: &ShaderStage, texture: TextureId) -> Self {
        Self {
            texture,
            additive: stage.blend == StageBlend::Add,
            spin: stage
                .texture_modifications
                .iter()
                .filter(|modification| modification.kind == "rotate")
                .filter_map(|modification| modification.arguments.first())
                .sum(),
        }
    }

    /// The stage blended over what is below by its alpha, whatever its shader's `blendFunc`.
    pub(super) fn blended(self) -> Self {
        Self {
            additive: false,
            ..self
        }
    }
}

/// A picture's stages; stock's radar shaders have at most two.
pub(super) type Picture = [Option<Layer>; 2];

/// The radar's marks and pictures.
#[derive(Default)]
pub(super) struct State {
    radar: Radar,
    /// The latest frame's marks with their pictures.
    marks: Vec<(RadarMark, Picture)>,
    shown: bool,
    /// `cg.time` of the latest frame, which turns spinning stages.
    time: i32,
    /// The map the pictures were read for, once they have been.
    map: Option<Vec<u8>>,
    stock: [Option<Picture>; 6],
    /// Pictures read by shader name (`None`: nothing of it can be shown).
    pictures: Vec<(String, Option<Picture>)>,
    /// The image in each radar cell, in cell order.
    images: Vec<String>,
    /// The vehicle definitions, read on the first vehicle the radar shows.
    vehicle_files: Option<VehicleFiles>,
    /// Vehicle names and the picture their `radarIcon` names.
    vehicles: Vec<(String, Option<String>)>,
}

/// What one frame of the radar reads.
pub(crate) struct RadarInputs<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) game: &'a GameState,
    /// The predicted player; without one (a demo) the snapshot's own player is the viewer.
    pub(crate) predicted: Option<&'a jkr_client::pmove::MovementState>,
    /// `cg.time`.
    pub(crate) time: i32,
    /// `cg_drawRadar`.
    pub(crate) enabled: bool,
    pub(crate) feedback: &'a jkr_client::event_feedback::Feedback,
}

/// What the radar reads its pictures with.
pub(crate) struct RadarAssets<'a> {
    pub(crate) vfs: &'a jkr_vfs::VirtualFileSystem,
    pub(crate) shaders: &'a jkr_shader::ShaderCatalog,
    pub(crate) renderer: &'a ui_renderer::ShapeRenderer,
    pub(crate) queue: &'a crate::frame_queue::FrameQueue,
}

impl HudOverlay {
    /// One frame of the radar: whether it shows, its marks and the pictures they need.
    pub(crate) fn update_radar(&mut self, input: RadarInputs<'_>, assets: RadarAssets<'_>) {
        let state = &mut self.radar;
        state.marks.clear();
        let player = &input.snapshot.player;
        let in_vehicle = player.vehicle_entity_num() != 0;
        let viewer = match input.predicted {
            Some(predicted) => RadarViewer {
                client: predicted.client_num,
                origin: predicted.origin,
                pitch: predicted.view_angles[0],
                yaw: predicted.view_angles[1],
                team: predicted.team,
                health: predicted.health,
                following: predicted.movement_flags & PMF_FOLLOW != 0,
                in_vehicle,
            },
            None => RadarViewer {
                client: player.client_num(),
                origin: player.origin(),
                pitch: player.view_angles()[0],
                yaw: player.view_angles()[1],
                team: player.team(),
                health: player.health(),
                following: player.movement_flags() & PMF_FOLLOW != 0,
                in_vehicle,
            },
        };
        // The pictures, the rocket lock's too, are read for every map, shown or not.
        state.new_map(input.game, &assets);
        state.shown = input.enabled && Radar::shown(&viewer, input.game);
        if !state.shown {
            return;
        }
        let time = input.time;
        state.time = time;
        let entities = input.snapshot.entities.iter().map(|entity| {
            let origin = jkr_client::legacy_evaluate_trajectory(
                entity.trajectory_base(),
                entity.trajectory_delta(),
                entity.trajectory_type(),
                entity.trajectory_time(),
                entity.trajectory_duration(),
                time,
            );
            RadarEntity::from_state(entity, origin, entity.angles()[1])
        });
        let State {
            radar,
            marks,
            stock,
            pictures,
            images,
            vehicle_files,
            vehicles,
            ..
        } = state;
        let built = radar.build(&viewer, input.game, time, entities, |client| {
            input.feedback.spoke_at(client)
        });
        for mark in built {
            let picture = match mark.icon {
                RadarIcon::Arrow => stock[1],
                RadarIcon::GoalItem => stock[2],
                RadarIcon::Rocket => stock[3],
                RadarIcon::Game(index) => usize::try_from(index)
                    .ok()
                    .and_then(|index| input.game.config_string(CS_ICONS + index))
                    .and_then(|name| std::str::from_utf8(name).ok())
                    .filter(|name| !name.is_empty())
                    .and_then(|name| picture(pictures, images, name, &assets)),
                RadarIcon::Vehicle(number) => {
                    vehicle_picture(vehicle_files, vehicles, &input, number, &assets)
                        .and_then(|name| picture(pictures, images, &name, &assets))
                }
            };
            if let Some(picture) = picture {
                marks.push((*mark, picture));
            }
        }
    }
}

impl State {
    /// The rocket lock's wedge and lock picture.
    pub(super) fn lock_pictures(&self) -> [Option<Picture>; 2] {
        [self.stock[4], self.stock[5]]
    }

    /// The HUD units the radar takes in the upper-right stack.
    pub(super) fn height(&self) -> f32 {
        if self.shown { SIZE + GAP } else { 0.0 }
    }

    /// The radar's box and marks from `top` (1080p HUD units) down.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2], top: f32) {
        if !self.shown {
            return;
        }
        let s = (viewport[1] / 1080.0).clamp(0.6, 2.5);
        let edge = SIZE * s;
        let origin = [viewport[0] - (RIGHT + SIZE) * s, top * s];
        let unit = edge / (2.0 * RADAR_RADIUS);
        let seconds = f64::from(self.time) * 0.001;
        if let Some(picture) = &self.stock[0] {
            // Stock asks for 60% alpha here, but its stages take no vertex colour in
            // either renderer: the box shows at the picture's own alpha.
            let rect = Rect::new(origin[0], origin[1], edge, edge);
            draw(list, picture, rect, 0.0, [1.0; 4], seconds);
        }
        for (mark, picture) in &self.marks {
            let size = mark.size * unit;
            let (x, y) = (origin[0] + mark.x * unit, origin[1] + mark.y * unit);
            // A turned mark is placed by its centre (`CG_DrawRotatePic2`), others by a corner.
            let rect = match mark.angle {
                Some(_) => Rect::new(x - size * 0.5, y - size * 0.5, size, size),
                None => Rect::new(x, y, size, size),
            };
            let degrees = mark.angle.unwrap_or(0.0);
            draw(list, picture, rect, degrees, mark.color, seconds);
        }
    }

    /// On a new map, forget the pictures read for the last one and read the stock four.
    fn new_map(&mut self, game: &GameState, assets: &RadarAssets<'_>) {
        let map = game
            .config_string(0)
            .and_then(|info| jkr_client::LegacyClientInfo::new(info).bytes("mapname"))
            .unwrap_or_default();
        if self.map.as_deref() == Some(map) {
            return;
        }
        self.map = Some(map.to_vec());
        self.pictures.clear();
        self.images.clear();
        self.vehicles.clear();
        self.vehicle_files = None;
        for (slot, name) in STOCK.iter().enumerate() {
            self.stock[slot] = read_picture(&mut self.images, name, assets);
        }
    }
}

/// Each stage of `picture` over `rect` in `color`, turned `degrees` and by the stage's own
/// spin.
pub(super) fn draw(
    list: &mut DrawList,
    picture: &Picture,
    rect: Rect,
    degrees: f32,
    color: [f32; 4],
    seconds: f64,
) {
    let [r, g, b, a] = color;
    let color = Color::new(r, g, b, a);
    for layer in picture.iter().flatten() {
        let degrees = degrees + (f64::from(layer.spin) * seconds).rem_euclid(360.0) as f32;
        let texture = layer.texture;
        let _ = list.push(if degrees == 0.0 && !layer.additive {
            DrawCommand::TexturedQuad {
                rect,
                texture,
                color,
            }
        } else {
            DrawCommand::RotatedQuad {
                rect,
                degrees,
                texture,
                color,
                additive: layer.additive,
            }
        });
    }
}

/// Shader `name`'s picture, read the first time it is asked for.
fn picture(
    pictures: &mut Vec<(String, Option<Picture>)>,
    images: &mut Vec<String>,
    name: &str,
    assets: &RadarAssets<'_>,
) -> Option<Picture> {
    if let Some((_, picture)) = pictures
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
    {
        return *picture;
    }
    let picture = read_picture(images, name, assets);
    pictures.push((name.to_owned(), picture));
    picture
}

/// Shader `name`'s first two stages with their images in radar cells. A name without a
/// shader is `R_RegisterShaderNoMip`'s implicit picture: one blended stage.
fn read_picture(images: &mut Vec<String>, name: &str, assets: &RadarAssets<'_>) -> Option<Picture> {
    let mut picture: Picture = [None; 2];
    let stages = assets
        .shaders
        .get(name)
        .map_or(&[][..], |shader| shader.stages.as_slice());
    if stages.is_empty() {
        let path = assets
            .shaders
            .resolve_image(assets.vfs, name)
            .ok()
            .flatten();
        picture[0] = path
            .and_then(|path| cell(images, path.as_str(), assets))
            .map(|texture| Layer {
                texture,
                additive: false,
                spin: 0.0,
            });
    }
    for (slot, stage) in picture.iter_mut().zip(stages) {
        let path = stage.images.first().and_then(|image| {
            assets
                .shaders
                .resolve_stage_image(assets.vfs, image)
                .ok()
                .flatten()
        });
        *slot = path
            .and_then(|path| cell(images, path.as_str(), assets))
            .map(|texture| Layer::of(stage, texture));
    }
    if picture.iter().all(Option::is_none) {
        crate::log::progress(format_args!("radar picture not shown: {name}"));
        return None;
    }
    Some(picture)
}

/// The radar cell holding image `path`, read into the next free one the first time.
fn cell(images: &mut Vec<String>, path: &str, assets: &RadarAssets<'_>) -> Option<TextureId> {
    let index = match images.iter().position(|known| known == path) {
        Some(index) => index,
        None => {
            if images.len() >= RADAR_CELL_COUNT as usize {
                return None;
            }
            let pixels = super::icons::assets::decode_path(assets.vfs, path)?;
            let texture = TextureId(RADAR_CELLS + images.len() as u32);
            assets
                .renderer
                .upload_icon(assets.queue, texture, pixels.as_raw());
            images.push(path.to_owned());
            images.len() - 1
        }
    };
    Some(TextureId(RADAR_CELLS + index as u32))
}

/// The `radarIcon` of the vehicle entity `number` is (`$<vehicle>` in `CS_MODELS`).
fn vehicle_picture(
    files: &mut Option<VehicleFiles>,
    vehicles: &mut Vec<(String, Option<String>)>,
    input: &RadarInputs<'_>,
    number: u16,
    assets: &RadarAssets<'_>,
) -> Option<String> {
    let entities = &input.snapshot.entities;
    let entity = entities
        .binary_search_by_key(&number, |entity| entity.number())
        .ok()
        .map(|index| &entities[index])?;
    let model = input
        .game
        .config_string(CS_MODELS + usize::try_from(entity.model_index()).ok()?)?;
    let name = jkr_client::legacy_vehicle_name(std::str::from_utf8(model).ok()?)?;
    if let Some((_, picture)) = vehicles
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
    {
        return picture.clone();
    }
    let files = files.get_or_insert_with(|| {
        VehicleFiles::from_listing(
            |directory, extension| assets.vfs.list_files(directory, extension),
            |path| {
                assets
                    .vfs
                    .read(path)
                    .ok()
                    .flatten()
                    .map(|asset| asset.bytes)
            },
        )
    });
    let picture = files.radar_icon(name.as_bytes());
    vehicles.push((name.to_owned(), picture.clone()));
    picture
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaders written the way the stock radar ones are.
    const SHADERS: &str = "
        test/spinning
        {
            { map test/item blendFunc GL_ONE GL_ONE rgbGen identity tcMod rotate -6 }
            { map test/item blendFunc GL_ONE GL_ONE rgbGen identity tcMod rotate 5 }
        }
        test/blended
        {
            { map test/icon blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA rgbGen vertex }
        }
    ";

    fn layers(name: &str) -> Vec<Layer> {
        let shaders = jkr_shader::parse_shader_script(SHADERS.as_bytes(), "test.shader")
            .expect("test shaders parse");
        let shader = shaders
            .iter()
            .find(|shader| shader.name == name)
            .expect("test shader");
        let stages = shader.stages.iter();
        stages
            .map(|stage| Layer::of(stage, TextureId(RADAR_CELLS)))
            .collect()
    }

    #[test]
    fn stages_keep_their_blend_and_spin() {
        let spinning = layers("test/spinning");
        assert_eq!(spinning.len(), 2);
        assert!(spinning.iter().all(|layer| layer.additive));
        assert_eq!([spinning[0].spin, spinning[1].spin], [-6.0, 5.0]);
        let blended = layers("test/blended");
        assert_eq!((blended[0].additive, blended[0].spin), (false, 0.0));
    }

    #[test]
    fn marks_draw_their_stages_in_their_colour() {
        let layer = |additive, spin| {
            Some(Layer {
                texture: TextureId(RADAR_CELLS),
                additive,
                spin,
            })
        };
        let mut list = DrawList::new(8);
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let green = [0.0, 1.0, 0.0, 0.5];
        // A teammate's `arrow_w`: added, turned, in cgame's green.
        draw(&mut list, &[layer(true, 0.0), None], rect, 30.0, green, 0.0);
        // A picture without a shader: blended and upright.
        draw(&mut list, &[layer(false, 0.0), None], rect, 0.0, green, 0.0);
        // Five degrees a second, two seconds in.
        draw(&mut list, &[layer(true, 5.0), None], rect, 0.0, green, 2.0);
        let green = Color::new(0.0, 1.0, 0.0, 0.5);
        match list.commands() {
            [
                DrawCommand::RotatedQuad {
                    degrees: arrow,
                    color: arrow_color,
                    additive: true,
                    ..
                },
                DrawCommand::TexturedQuad { color: icon, .. },
                DrawCommand::RotatedQuad { degrees: spun, .. },
            ] => {
                assert_eq!((*arrow, *arrow_color, *icon), (30.0, green, green));
                assert!((spun - 10.0).abs() < 1e-4);
            }
            commands => panic!("unexpected commands: {commands:?}"),
        }
    }
}
