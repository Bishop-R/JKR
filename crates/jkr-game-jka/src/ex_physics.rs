//! `G_RunExPhys` (`codemp/game/g_exphysics.c:39-297`) without Ghoul2 bolts: the extra physics
//! that thrown limbs ([`crate::npc_dismember`]) and set-down E-Webs ([`crate::eweb`]) fall by.
//!
//! Gravity gathers while nothing is under the thing (at most 512 a call) and is spent at once
//! when it rests on something; its velocity is cut by its mass, and a tenth of it is the move
//! of one call. What it strikes bounces it back along the surface's normal, the bounce taking
//! some of its gravity off when it is thrown upward. A thing that has all but stopped on the
//! ground stops; one whose move starts in something solid does not move at all.
//!
//! The caller's trace is the thing's own box from a point to another, passing the thing itself
//! with its clip mask; the caller places it (`G_SetOrigin`) and links it as the [`Step`] says.
//! Left out: the touch a thing may have (no caller here has one).

use crate::pmove::MovementTrace;

/// `MAX_GRAVITY_PULL`.
const MAX_GRAVITY_PULL: f32 = 512.0;
/// `ENTITYNUM_NONE`: nothing under it.
const ENTITYNUM_NONE: u16 = 1_023;

/// What `G_RunExPhys` keeps on an entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    /// `epVelocity`.
    pub velocity: [f32; 3],
    /// `epGravFactor`.
    pub grav_factor: f32,
    /// `s.groundEntityNum`, as the last call with gravity found it.
    pub ground: u16,
}

impl Body {
    /// A thing at rest at `origin`, standing on `ground`.
    pub fn at(origin: [f32; 3], ground: u16) -> Self {
        Self {
            origin,
            velocity: [0.0; 3],
            grav_factor: 0.0,
            ground,
        }
    }
}

/// A thing's physics: the gravity a call adds, its mass (0.01 to 1) and its bounce.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Physics {
    pub gravity: f32,
    pub mass: f32,
    pub bounce: f32,
}

/// How a call ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// No velocity, even with gravity: nothing moved.
    Still,
    /// "we've pretty much stopped moving anyway": velocity and gravity spent, linked where it
    /// is.
    Stopped,
    /// "can't go anywhere from here": its move started in something solid.
    Stuck,
    /// Moved to the trace's end (`G_SetOrigin`) and linked; the trace, when it struck
    /// something on the way.
    Moved { struck: Option<MovementTrace> },
}

/// One call of `G_RunExPhys(ent, gravity, mass, bounce, autoKill, NULL, 0)` on `body`; `trace`
/// sweeps the thing's box from one point to another. An `autoKill` caller frees the thing
/// when it is [`Step::Stuck`].
pub fn run(
    body: &mut Body,
    physics: Physics,
    trace: &mut dyn FnMut([f32; 3], [f32; 3]) -> MovementTrace,
) -> Step {
    if physics.gravity != 0.0 {
        let mut below = body.origin;
        below[2] -= 0.1;
        let ground = trace(body.origin, below);
        body.ground = if ground.fraction == 1.0 {
            ENTITYNUM_NONE
        } else {
            ground.entity_number
        };
        if body.ground == ENTITYNUM_NONE {
            body.grav_factor = (body.grav_factor + physics.gravity).min(MAX_GRAVITY_PULL);
            body.velocity[2] -= body.grav_factor;
        } else {
            body.grav_factor = 0.0;
        }
    }
    if body.velocity == [0.0; 3] {
        return Step::Still;
    }
    let projected: [f32; 3] =
        std::array::from_fn(|axis| body.origin[axis] + 0.1 * body.velocity[axis]);
    let keep = 1.0 - physics.mass;
    body.velocity = body.velocity.map(|value| value * keep);
    // `VectorNormalize` of a copy: its length, in the game's float arithmetic.
    let velocity = body.velocity;
    let mut total =
        (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]).sqrt();
    if total < 1.0 && body.ground != ENTITYNUM_NONE {
        body.velocity = [0.0; 3];
        body.grav_factor = 0.0;
        return Step::Stopped;
    }
    let moved = trace(body.origin, projected);
    if moved.start_solid || moved.all_solid {
        return Step::Stuck;
    }
    body.origin = moved.end_position;
    if moved.fraction == 1.0 {
        return Step::Moved { struck: None };
    }
    if physics.bounce != 0.0 {
        total *= physics.bounce;
        let rebound = moved.plane_normal.map(|value| value * total);
        if rebound[2] > 0.0 {
            // "The lighter it is the more gravity will be reduced by bouncing vertically."
            body.grav_factor = (body.grav_factor - rebound[2] * keep).max(0.0);
        }
        for axis in 0..3 {
            body.velocity[axis] += rebound[axis];
        }
    } else {
        body.velocity[0] = 0.0;
        body.velocity[1] = 0.0;
        if physics.gravity == 0.0 {
            body.velocity[2] = 0.0;
        }
    }
    Step::Moved {
        struck: Some(moved),
    }
}
