//! Per-map camera routes for the menu backdrop: where each menu screen
//! flies to and the waypoints that get the camera there without clipping
//! walls. Keyed by the worldspawn `message` so map packs can be added as
//! plain data rows.

use super::Shot;
use crate::world_props::PropSpec;

/// One authored camera pose along a route, angles in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Waypoint {
    pub(crate) origin: [f32; 3],
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

/// Where the player's model stands in a shot: the point it is dropped onto
/// the floor from and the direction it faces, in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stage {
    pub(crate) origin: [f32; 3],
    pub(crate) yaw: f32,
}

/// Flight to one screen's shot. The route starts where `from` parks — the
/// map's `info_player_intermission` for [`Shot::Main`] (not repeated here),
/// else the end of that shot's own route — and ends on the shot; everything
/// in between keeps the camera in open air.
#[derive(Debug, PartialEq)]
pub(crate) struct Route {
    /// Worldspawn `message` this route belongs to.
    pub(crate) map_message: &'static str,
    /// Screen shot the route arrives at.
    pub(crate) shot: Shot,
    /// Shot the route departs from; a route from anything but the main
    /// vantage chains onto that shot's route and must be listed after it.
    pub(crate) from: Shot,
    /// Wall-clock length of the full one-way flight.
    pub(crate) millis: u32,
    pub(crate) points: &'static [Waypoint],
    /// Where the player's model stands once the camera has arrived.
    pub(crate) stage: Option<Stage>,
    /// Where the shot presents a floating object (the thrown saber).
    pub(crate) focus: Option<[f32; 3]>,
}

const fn point(origin: [f32; 3], yaw: f32, pitch: f32) -> Waypoint {
    Waypoint { origin, yaw, pitch }
}

/// mp/ffa3 browser: from the courtyard over the parked transport's nose,
/// down into the walled corridor that leaves the landing pad to the south
/// and along it to the big gate (`door_lrg`, x 528..880 at y −1488, z
/// −144..144) at its end, parking at the mouth of its recessed portal so the
/// gate and the sunlit floor in front of it sit behind the server list.
const TATOOINE_FFA_BROWSER: Route = Route {
    map_message: "Tatooine FFA",
    shot: Shot::Browser,
    from: Shot::Main,
    millis: 2_600,
    points: &[
        point([1_400.0, 700.0, 140.0], 200.0, -8.0),
        point([1_050.0, 650.0, 230.0], 225.0, -15.0),
        point([820.0, 420.0, 180.0], 255.0, -20.0),
        point([740.0, 250.0, 60.0], 268.0, -6.0),
        point([708.0, -300.0, -20.0], 270.0, -3.0),
        point([704.0, -800.0, -50.0], 270.0, -1.0),
        point([704.0, -1_120.0, -60.0], 270.0, 3.0),
    ],
    stage: None,
    focus: None,
};

/// mp/ffa3 settings: the rock-walled balcony in the wall directly behind
/// the intermission vantage (its sill at z ≈ 250 above the crates, arch top
/// ≈ 420). The camera keeps facing the parked transport, climbs straight up
/// out of the courtyard and backs through the arch, so the settings form
/// sits in the alcove with the ship framed in the opening beside it.
const TATOOINE_FFA_SETTINGS: Route = Route {
    map_message: "Tatooine FFA",
    shot: Shot::Settings,
    from: Shot::Main,
    millis: 1_600,
    points: &[
        point([1_800.0, 810.0, 180.0], 192.0, 10.0),
        point([1_900.0, 835.0, 330.0], 192.0, 0.0),
        point([2_100.0, 900.0, 340.0], 190.0, -6.0),
        point([2_280.0, 985.0, 345.0], 186.0, -10.0),
    ],
    stage: None,
    focus: None,
};

/// mp/ffa3 player: rise out of the courtyard and glide up to the round
/// tower's balcony, where the model stands on the lip facing out over the
/// yard — close enough to the main vantage to be seen from the main menu.
const TATOOINE_FFA_PLAYER: Route = Route {
    map_message: "Tatooine FFA",
    shot: Shot::Player,
    from: Shot::Main,
    millis: 2_200,
    points: &[
        point([1_500.0, 780.0, 160.0], 190.0, 6.0),
        point([1_100.0, 700.0, 380.0], 200.0, 4.0),
        point([780.0, 600.0, 500.0], 210.0, 0.0),
        point([562.0, 508.0, 518.0], 219.0, -8.0),
    ],
    // The balcony is patch geometry (no brush collision), so the floor
    // height is authored: its drawn surface lies flat at z 472 (sampled from
    // the tessellated `concrete_wall` triangles under this point).
    stage: Some(Stage {
        origin: [452.0, 478.0, 472.0],
        yaw: 14.0,
    }),
    focus: None,
};

/// mp/ffa3 saber: from the balcony shot, swing round past the tower and
/// settle looking through the gap between the two curved walls at the far
/// building's gate, where the thrown saber floats.
const TATOOINE_FFA_SABER: Route = Route {
    map_message: "Tatooine FFA",
    shot: Shot::Saber,
    from: Shot::Player,
    millis: 1_400,
    points: &[
        point([600.0, 522.0, 512.0], 300.0, 2.0),
        point([665.0, 540.0, 506.0], 16.0, -1.0),
    ],
    stage: None,
    // 48 units ahead (was 70: the owner found the hilt too far away).
    focus: Some([709.0, 552.5, 492.0]),
};

/// mp/ffa3's big gate at the end of the browser corridor: one `door_lrg`
/// face whose texture already restarts at x 704 (two mirrored leaves), so
/// that seam is the split. Opening, the leaves push 24 units back into the
/// recess (void lies behind the face) and slide into the flanking walls
/// (x 520..552 and 856..888, 16 units in front of the door plane), which
/// hide them from anywhere in the corridor.
const TATOOINE_FFA_GATE: PropSpec = PropSpec {
    map_message: "Tatooine FFA",
    shader: "textures/desert/door_lrg",
    bounds: ([528.0, -1_488.0, -144.0], [880.0, -1_488.0, 144.0]),
    split_axis: 0,
    split_at: 704.0,
    unseal: [0.0, -24.0, 0.0],
    travel: 184.0,
    doorway_origin: [704.0, -1_488.0, -144.0],
    doorway_yaw: 270.0,
};

const PROPS: &[PropSpec] = &[TATOOINE_FFA_GATE];

const ROUTES: &[Route] = &[
    TATOOINE_FFA_BROWSER,
    TATOOINE_FFA_SETTINGS,
    TATOOINE_FFA_PLAYER,
    TATOOINE_FFA_SABER,
];

/// World props authored for the map whose worldspawn message is `message`.
pub(crate) fn props_for<'a>(message: &'a str) -> impl Iterator<Item = &'static PropSpec> + 'a {
    PROPS.iter().filter(move |prop| prop.map_message == message)
}

/// Route authored for `shot` on the map whose worldspawn message is
/// `message`.
pub(crate) fn route_for(message: &str, shot: Shot) -> Option<&'static Route> {
    ROUTES
        .iter()
        .find(|route| route.map_message == message && route.shot == shot)
}
