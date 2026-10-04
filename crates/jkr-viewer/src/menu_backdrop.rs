//! Live-map backdrop behind the standalone client menus.
//!
//! The main menu sits over the real renderer instead of a flat gradient: the
//! camera parks at the map's `info_player_intermission` view (the vantage the
//! map author chose to show off the level) and drifts very slowly so the
//! screen never reads as a still image. Screens that ask for a different
//! shot (settings) fly there along a per-map authored route and back again.

mod flight;
mod passage;
mod routes;

use super::GpuState;
use crate::world_props::{self, GateCue, PropSpec};
use flight::Path;
use glam::Vec3;
use jkr_bsp::Bsp;
use jkr_entity::parse_entity_lump;
use jkr_ui::{Easing, Tween};
use passage::Passage;
pub(crate) use routes::{Stage, props_for};
use std::time::Instant;

/// Which backdrop shot a menu screen wants behind it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shot {
    /// The intermission vantage behind the main menu.
    Main,
    /// The map's server-browser vantage (the big gate at the end of the
    /// corridor beside the ship on mp/ffa3).
    Browser,
    /// The map's settings vantage (the balcony behind the main vantage on mp/ffa3).
    Settings,
    /// The map's player stage (the tower balcony on mp/ffa3).
    Player,
    /// Where the thrown saber floats (between the tower walls on mp/ffa3),
    /// reached from the player stage.
    Saber,
}

/// Fraction of the flight over which the destination screen fades in. It
/// starts at departure so settings are usable while the camera is still
/// moving; the flight is scenery, not a wait.
const REVEAL_SPAN: f32 = 0.18;
/// How long the gate takes to open once the camera is in front of it, and
/// to close again when a connection is abandoned.
const GATE_OPEN_MILLIS: u32 = 1_200;
const GATE_CLOSE_MILLIS: u32 = 1_200;
/// Browser-flight progress from which the gate is close enough to open.
const GATE_IN_VIEW: f32 = 0.5;

/// Camera anchor for the menu backdrop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Vantage {
    pub(crate) origin: Vec3,
    pub(crate) yaw: f32,
    pitch: f32,
}

/// One drifted camera sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    pub(crate) origin: Vec3,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

/// One authored flight: from the main vantage, or chained onto the end of
/// another flight (`parent`).
struct Flight {
    shot: Shot,
    parent: Option<usize>,
    path: Path,
    millis: u32,
    stage: Option<Stage>,
    focus: Option<Vec3>,
}

/// Camera state behind the standalone menus: the main vantage, the routes
/// to the other shots, and where along the active one the camera is.
pub(crate) struct Backdrop {
    main: Vantage,
    flights: Vec<Flight>,
    /// Flight the camera is currently on (or parked at an end of).
    active: Option<usize>,
    /// 0 = parked where the active flight departs from (the main vantage or
    /// its parent's shot), 1 = parked on the active flight's shot.
    progress: Tween,
    last_progress: f32,
    /// Whether the client wants the map's gate prop open (it is connecting).
    gate_wanted: bool,
    /// Whether the server world the glide leads into is built and waiting
    /// (true while nothing is pending, so a map without a join never waits).
    world_ready: bool,
    /// 0 = the gate prop at rest, 1 = fully open.
    gate: Tween,
    /// The glide through the open gate into the map being joined.
    passage: Passage,
    gate_prop: Option<&'static PropSpec>,
    /// Gate opening last frame, to notice the cues it passes.
    gate_last: f32,
    /// A cue the opening passed, until it is taken.
    gate_cue: Option<GateCue>,
}

impl Backdrop {
    pub(crate) fn from_bsp(bsp: &Bsp) -> Option<Self> {
        let main = Vantage::from_bsp(bsp)?;
        let message = worldspawn_message(bsp);
        let routes = [Shot::Browser, Shot::Settings, Shot::Player, Shot::Saber]
            .into_iter()
            .filter_map(|shot| routes::route_for(message.as_deref()?, shot))
            .collect::<Vec<_>>();
        Some(Self::new(main, &routes, gate_for(bsp)))
    }

    /// Routes chained onto another shot must follow that shot's route; a
    /// route whose parent is missing is dropped. `gate` is the map's gate
    /// prop, opened and flown through when a join completes.
    fn new(main: Vantage, routes: &[&routes::Route], gate: Option<&'static PropSpec>) -> Self {
        let mut flights: Vec<Flight> = Vec::with_capacity(routes.len());
        for route in routes {
            let parent = match route.from {
                Shot::Main => None,
                from => match flights.iter().position(|flight| flight.shot == from) {
                    Some(parent) => Some(parent),
                    None => continue,
                },
            };
            let start = parent.map_or(main, |parent| flights[parent].path.destination());
            flights.push(Flight {
                shot: route.shot,
                parent,
                path: Path::new(start, route),
                millis: route.millis,
                stage: route.stage,
                focus: route.focus.map(Vec3::from_array),
            });
        }
        Self {
            main,
            flights,
            active: None,
            progress: Tween::settled(0.0),
            last_progress: 0.0,
            gate_wanted: false,
            world_ready: true,
            gate: Tween::settled(0.0),
            passage: Passage::new(gate.map(PropSpec::doorway)),
            gate_prop: gate,
            gate_last: 0.0,
            gate_cue: None,
        }
    }

    fn flight_for(&self, shot: Shot) -> Option<usize> {
        self.flights.iter().position(|flight| flight.shot == shot)
    }

    /// Whether `ancestor` is `flight` itself or a flight it chains onto.
    fn descends_from(&self, flight: Option<usize>, ancestor: Option<usize>) -> bool {
        let mut current = flight;
        loop {
            if current == ancestor {
                return true;
            }
            match current {
                Some(index) => current = self.flights[index].parent,
                None => return false,
            }
        }
    }

    /// The flight chained directly onto `parent` on the way to `wanted`.
    fn child_toward(&self, parent: Option<usize>, wanted: Option<usize>) -> Option<usize> {
        let mut current = wanted?;
        while self.flights[current].parent != parent {
            current = self.flights[current].parent?;
        }
        Some(current)
    }

    /// Advance toward `shot` at backdrop time `millis` and return the camera.
    /// A destination off the current chain first flies back down the chain
    /// (to the main vantage at most), then departs along the new one; a
    /// chained shot is reached through its parent's shot.
    pub(crate) fn drive(&mut self, shot: Shot, millis: u64) -> Sample {
        let wanted = self.flight_for(shot);
        let progress = self.progress.sample(millis);
        let parked_start = self.progress.target() == 0.0 && progress <= 1e-4;
        let parked_end = self.progress.target() == 1.0 && progress >= 1.0 - 1e-4;
        // Parked at a flight's start: the camera is on the parent's shot.
        if parked_start && let Some(active) = self.active {
            self.active = self.flights[active].parent;
            self.progress = Tween::settled(1.0);
        }
        let target = if self.active.is_none() && wanted.is_none() {
            0.0
        } else if self.active == wanted {
            1.0
        } else if self.descends_from(wanted, self.active) {
            // On the way out along the chain: depart the next flight once
            // the camera has reached this one's end.
            let next = self.child_toward(self.active, wanted);
            if self.active.is_none() || parked_end {
                self.active = next;
                self.progress = Tween::settled(0.0);
                1.0
            } else {
                1.0
            }
        } else {
            0.0
        };
        let millis_full = self.active.map_or(0, |index| self.flights[index].millis);
        if self.progress.target() != target {
            let remaining = (target - self.progress.sample(millis)).abs();
            let duration = (millis_full as f32 * remaining) as u32;
            self.progress
                .retarget(target, millis, duration, Easing::SmoothStep);
        }
        let progress = self.progress.sample(millis);
        self.last_progress = progress;
        self.drive_gate(millis);
        let seconds = millis as f32 / 1_000.0;
        let Some(flight) = self.active.map(|index| &self.flights[index]) else {
            return self.main.sample(seconds);
        };
        let pose = flight.path.sample(progress);
        let start = flight
            .parent
            .map_or(self.main, |parent| self.flights[parent].path.destination());
        let departure = start.drift(seconds);
        let arrival = flight.path.destination().drift(seconds);
        let drift = departure.blend(arrival, progress);
        let parked = Sample {
            origin: pose.origin + drift.origin,
            yaw: pose.yaw + drift.yaw,
            pitch: pose.pitch + drift.pitch,
        };
        self.passage.pose(parked, millis)
    }

    /// Ask for the map's gate prop to open (the client is connecting) or
    /// close. It only opens once the camera is in front of it.
    pub(crate) fn set_gate(&mut self, wanted: bool) {
        self.gate_wanted = wanted;
    }

    /// Whether the server world behind the gate is built: the glide through
    /// the gate does not set off before it is.
    pub(crate) fn set_world_ready(&mut self, ready: bool) {
        self.world_ready = ready;
    }

    /// Shut the gate and end any glide at once: the menu world is coming
    /// back after a game, so there is nothing to walk back out of.
    pub(crate) fn reset_gate(&mut self) {
        self.gate_wanted = false;
        self.gate = Tween::settled(0.0);
        self.gate_last = 0.0;
        self.gate_cue = None;
        self.passage.reset();
    }

    fn drive_gate(&mut self, millis: u64) {
        let in_view = self.active.is_some()
            && self.active == self.flight_for(Shot::Browser)
            && self.last_progress >= GATE_IN_VIEW;
        // The gate stays open while the camera is anywhere in the doorway.
        let wanted = self.gate_wanted && in_view || self.passage.under_way(millis);
        let target = if wanted { 1.0 } else { 0.0 };
        if self.gate.target() != target {
            let duration = if target > 0.0 {
                GATE_OPEN_MILLIS
            } else {
                GATE_CLOSE_MILLIS
            };
            self.gate.retarget(target, millis, duration, Easing::Linear);
        }
        let open = self.gate.sample(millis);
        if let Some(cue) = world_props::gate_cue(self.gate_last, open) {
            self.gate_cue = Some(cue);
        }
        self.gate_last = open;
        let fully_open = self.gate.target() == 1.0 && open >= 1.0;
        self.passage
            .drive(self.gate_wanted, fully_open, self.world_ready, millis);
    }

    /// A cue the gate's opening passed since the last call, with where the
    /// dust it shakes loose falls from.
    pub(crate) fn take_gate_cue(&mut self) -> Option<(GateCue, [[f32; 3]; 2])> {
        let cue = self.gate_cue.take()?;
        Some((cue, self.gate_prop?.dust_points()))
    }

    /// How far the map's gate prop is open at backdrop time `millis`.
    pub(crate) fn gate_open(&self, millis: u64) -> f32 {
        self.gate.sample(millis)
    }

    /// Whether the camera has flown through the open gate: the join may cut
    /// to the server world.
    pub(crate) fn gate_crossed(&self, millis: u64) -> bool {
        self.passage.crossed(millis)
    }

    /// How far along the glide through the gate the camera is (0 parked,
    /// 1 beyond the doorway), for diagnostics.
    pub(crate) fn passage_progress(&self, millis: u64) -> f32 {
        self.passage.progress(millis)
    }

    /// Opacity for the screen that belongs to `shot`: fully visible while the
    /// camera is parked there or heading there, fading in as soon as the
    /// flight departs and gone while the camera flies the other way. A
    /// screen stays visible while a flight chained onto its shot returns to
    /// it (the saber flying back to the player screen).
    pub(crate) fn reveal(&self, shot: Shot) -> f32 {
        let wanted = self.flight_for(shot);
        let target = match wanted {
            Some(index) if self.active == Some(index) => 1.0,
            None if shot == Shot::Main => 0.0,
            // A shot without a route on this map sits over the main vantage.
            None if self.active.is_none() => return 1.0,
            _ => {
                let returning = self.progress.target() == 0.0
                    && self
                        .active
                        .is_some_and(|active| self.flights[active].parent == wanted);
                return if returning { 1.0 } else { 0.0 };
            }
        };
        if self.progress.target() != target {
            return 0.0;
        }
        if target == 0.0
            && self
                .active
                .is_some_and(|active| self.flights[active].parent.is_some())
        {
            // Still on a chained flight: the main menu waits for the last leg.
            return 0.0;
        }
        let toward = (1.0 - (target - self.last_progress).abs()).clamp(0.0, 1.0);
        if toward >= 1.0 - 1e-4 {
            return 1.0;
        }
        (toward / REVEAL_SPAN).clamp(0.0, 1.0)
    }

    /// Where the player's model stands on this map. The stage is part of the
    /// scenery from every shot (the ffa3 balcony is in view from the main
    /// vantage), not only while the camera is on the player route.
    pub(crate) fn stage(&self) -> Option<Stage> {
        self.flights.iter().find_map(|flight| flight.stage)
    }

    /// Where `shot` presents floating objects on this map, and the camera's
    /// right along which several of them line up.
    pub(crate) fn focus(&self, shot: Shot) -> Option<Focus> {
        let flight = self.flights.iter().find(|flight| flight.shot == shot)?;
        let yaw = flight.path.destination().yaw;
        Some(Focus {
            point: flight.focus?,
            right: Vec3::new(yaw.sin(), -yaw.cos(), 0.0),
        })
    }
}

/// A shot's presentation spot: the point it looks at and the horizontal
/// axis to the camera's right, so a row of objects lines up across the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Focus {
    pub(crate) point: Vec3,
    pub(crate) right: Vec3,
}

/// The map's worldspawn `message` (its display name), the key the authored
/// routes and props are filed under.
pub(crate) fn worldspawn_message(bsp: &Bsp) -> Option<String> {
    let entities = parse_entity_lump(bsp.entities()).ok()?;
    entities
        .iter()
        .find(|entity| entity.classname() == Some("worldspawn"))
        .and_then(|entity| entity.get("message"))
        .map(str::to_owned)
}

/// The map's gate prop, if it has one: the menu's doorway into the map
/// being joined.
pub(crate) fn gate_for(bsp: &Bsp) -> Option<&'static PropSpec> {
    let message = worldspawn_message(bsp)?;
    props_for(&message).next()
}

impl Vantage {
    /// The intermission view (looking at its `target` when it has one), else
    /// the same spawn the free camera starts from.
    pub(crate) fn from_bsp(bsp: &Bsp) -> Option<Self> {
        let entities = parse_entity_lump(bsp.entities()).ok()?;
        let intermission = entities
            .iter()
            .find(|entity| entity.classname() == Some("info_player_intermission"));
        if let Some(entity) = intermission
            && let Some(origin) = entity.vector("origin").ok().flatten()
        {
            let origin = Vec3::from_array(origin);
            let target = entity.get("target").and_then(|name| {
                entities
                    .iter()
                    .find(|candidate| candidate.get("targetname") == Some(name))
                    .and_then(|candidate| candidate.vector("origin").ok().flatten())
            });
            let (yaw, pitch) = match target {
                Some(target) => {
                    let direction = (Vec3::from_array(target) - origin).normalize_or(Vec3::X);
                    (direction.y.atan2(direction.x), direction.z.asin())
                }
                None => {
                    let angles = entity.vector("angles").ok().flatten().unwrap_or_default();
                    (angles[1].to_radians(), -angles[0].to_radians())
                }
            };
            return Some(Self { origin, yaw, pitch });
        }
        let (origin, yaw) = crate::assets::initial_camera(bsp).ok()?;
        Some(Self {
            origin: Vec3::from_array(origin),
            yaw,
            pitch: 0.0,
        })
    }

    /// Slow cinematic drift: a gentle yaw sweep, a slower pitch breath, and a
    /// few units of lateral float. Periods are long enough that no motion is
    /// perceptible frame to frame; only the parallax over seconds is.
    pub(crate) fn sample(self, seconds: f32) -> Sample {
        let drift = self.drift(seconds);
        Sample {
            origin: self.origin + drift.origin,
            yaw: self.yaw + drift.yaw,
            pitch: self.pitch + drift.pitch,
        }
    }

    /// The drift offset alone, so it can be blended across a flight.
    fn drift(self, seconds: f32) -> Sample {
        let right = Vec3::new(-self.yaw.sin(), self.yaw.cos(), 0.0);
        Sample {
            origin: right * 8.0 * (seconds * 0.09).sin()
                + Vec3::Z * 3.0 * (seconds * 0.13 + 0.7).sin(),
            yaw: 0.09 * (seconds * 0.11).sin(),
            pitch: 0.02 * (seconds * 0.07 + 1.3).sin(),
        }
    }
}

impl Sample {
    fn blend(self, other: Self, t: f32) -> Self {
        Self {
            origin: self.origin.lerp(other.origin, t),
            yaw: self.yaw + (other.yaw - self.yaw) * t,
            pitch: self.pitch + (other.pitch - self.pitch) * t,
        }
    }
}

/// True while a client menu is up outside any live or demo session, or
/// while a join is under way in the menu world, i.e. the world on screen is
/// only the menu backdrop and no HUD belongs over it.
pub(crate) fn standalone_menu_visible(gpu: &GpuState) -> bool {
    let sessions = gpu.live_session.is_some() || gpu.demo_session.is_some();
    gpu.client_menu
        .as_ref()
        .is_some_and(|menu| menu_holds_view(menu, sessions, gpu.is_menu_world))
}

/// No HUD belongs over the menu backdrop, nor under the loading card of a
/// map restart or map change on a server world.
pub(crate) fn hides_hud(gpu: &GpuState) -> bool {
    standalone_menu_visible(gpu)
        || gpu
            .client_menu
            .as_ref()
            .is_some_and(crate::menu::ClientMenu::is_connecting)
}

/// Whether `menu` owns the camera given that a live or demo session exists
/// (`sessions`) and whether the world on screen is the menu map
/// (`menu_world`). A join's session appears while the menu still reads
/// `Connecting` — the map load names the map a frame later — so the whole
/// connect phase counts, not only the map load: for that one frame the
/// live camera would otherwise render the joined player's position inside
/// the menu map. Only the menu world has the backdrop's camera routes: a
/// map restart or map change reloads behind the loading card on a server
/// world, whose game camera keeps the view — the backdrop's gate shot would
/// show that map from the menu's vantage until the reload lands.
pub(crate) fn menu_holds_view(
    menu: &crate::menu::ClientMenu,
    sessions: bool,
    menu_world: bool,
) -> bool {
    menu.is_visible() && (!sessions || (menu_world && menu.is_connecting()))
}

/// How far the map's gate prop is open this frame (0 outside a connect).
pub(crate) fn gate_open(gpu: &GpuState, now: Instant) -> f32 {
    let millis = now.duration_since(gpu.ui_epoch).as_millis() as u64;
    gpu.client_menu
        .as_ref()
        .map_or(0.0, |menu| menu.gate_open(millis))
}

/// Fly or park the free camera on the current screen's shot while a
/// standalone menu is up.
pub(crate) fn drive(gpu: &mut GpuState, now: Instant) {
    if !standalone_menu_visible(gpu) || gpu.resident.map_change_pending {
        return;
    }
    let millis = now.duration_since(gpu.ui_epoch).as_millis() as u64;
    let world_ready = gpu.world_settled();
    let Some(sample) = gpu.client_menu.as_mut().and_then(|menu| {
        menu.set_world_ready(world_ready);
        menu.drive_backdrop(millis)
    }) else {
        return;
    };
    gpu.camera_position = sample.origin;
    gpu.camera_yaw = sample.yaw;
    gpu.camera_pitch = sample.pitch;
    let cue = gpu
        .client_menu
        .as_mut()
        .and_then(crate::menu::ClientMenu::take_gate_cue);
    if let Some((cue, points)) = cue {
        shake_dust_loose(gpu, cue, points, now);
    }
}

/// Dust falls from the gate's seam at each cue of its opening.
fn shake_dust_loose(gpu: &mut GpuState, cue: GateCue, points: [[f32; 3]; 2], now: Instant) {
    let Some(vfs) = gpu.vfs.clone() else {
        return;
    };
    for (index, point) in points.into_iter().enumerate() {
        let seed = (cue as u32) << 8 | index as u32;
        crate::effect_runtime::spawn_effect(
            &mut gpu.particles,
            &mut gpu.effect_aux,
            &mut gpu.effects,
            &vfs,
            world_props::DUST_EFFECT,
            Vec3::from_array(point),
            now,
            seed,
            0,
            &mut None,
            crate::combat_effects::rotation_from_direction([0.0, 0.0, -1.0]),
        );
    }
}

/// Legacy `hud.wgsl` backdrop selector: 4 = console, 3 = a menu screen that
/// still relies on the shader-drawn backdrop, 0 = none.
pub(crate) fn shader_menu_state(gpu: &GpuState) -> f32 {
    if gpu
        .console
        .as_ref()
        .is_some_and(|console| console.is_open())
    {
        4.0
    } else if gpu
        .client_menu
        .as_ref()
        .is_some_and(|menu| menu.is_visible() && menu.draw_list().is_none())
    {
        3.0
    } else {
        0.0
    }
}

/// The doorway of the gate prop while it stands open at all: where the map
/// being joined is seen through.
pub(crate) fn gate_doorway(gpu: &GpuState, now: Instant) -> Option<crate::portal::Frame> {
    if gate_open(gpu, now) <= 0.0 {
        return None;
    }
    gpu.mover_catalog
        .gate()
        .map(crate::world_props::Leaf::doorway)
}

/// Whether the menu camera has flown through the gate (or there is no gate
/// to fly through), so a finished join may cut to the server world.
pub(crate) fn gate_crossed(gpu: &GpuState, now: Instant) -> bool {
    let millis = now.duration_since(gpu.ui_epoch).as_millis() as u64;
    gpu.client_menu
        .as_ref()
        .is_none_or(|menu| menu.gate_crossed(millis))
}
