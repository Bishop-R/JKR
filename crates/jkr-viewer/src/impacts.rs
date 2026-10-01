//! Codemp impact-event to retail-effect presentation mapping.
//!
//! Missile dispatch mirrors `CG_EntityEvent` in `codemp/cgame/cg_event.c`
//! lines 2888-2956 and `CG_MissileHitWall`/`CG_MissileHitPlayer` in
//! `codemp/cgame/cg_weapons.c` lines 1980-2200. Saber selection mirrors
//! `cg_event.c` lines 2147-2379. Disruptor trace shots mirror `cg_event.c`
//! lines 2442-2530 and the code-built lines of `fx_disruptor.c`. The shared
//! client adapter supplies exact event-toggle de-duplication and `ByteToDir`
//! normals.

use jkr_client::{LegacyImpactEvent, LegacyImpactKind, LegacyImpactTracker};
use jkr_protocol::Snapshot;

const MAX_SNAPSHOT_IMPACTS: usize = 1_024;
const MAX_VISUALS_PER_IMPACT: usize = 6;

/// One visual primitive selected by codemp for an impact event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Visual {
    Effect {
        name: &'static str,
        direction: [f32; 3],
    },
    /// `FX_AddLine` with linear size and alpha (`fx_disruptor.c`).
    Line(Line),
}

/// One `FX_AddLine` call: a beam of constant colour whose half-width and
/// alpha fade linearly over `lifetime_millis`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Line {
    pub(crate) start: [f32; 3],
    pub(crate) end: [f32; 3],
    pub(crate) size: [f32; 2],
    pub(crate) alpha: [f32; 2],
    pub(crate) color: [f32; 3],
    pub(crate) lifetime_millis: u32,
    pub(crate) shader: &'static str,
}

const WHITE: [f32; 3] = [1.0; 3];
/// `YELLER` in `fx_disruptor.c:73`.
const YELLER: [f32; 3] = [0.8, 0.7, 0.0];

/// Fixed-size result; effect selection itself never allocates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plan {
    visuals: [Option<Visual>; MAX_VISUALS_PER_IMPACT],
    len: usize,
}

impl Plan {
    fn new() -> Self {
        Self {
            visuals: [None; MAX_VISUALS_PER_IMPACT],
            len: 0,
        }
    }

    fn effect(mut self, name: &'static str, direction: [f32; 3]) -> Self {
        if self.len < self.visuals.len() {
            self.visuals[self.len] = Some(Visual::Effect { name, direction });
            self.len += 1;
        }
        self
    }

    fn repeated(self, name: &'static str, direction: [f32; 3], count: usize) -> Self {
        (0..count).fold(self, |plan, _| plan.effect(name, direction))
    }

    fn line(mut self, line: Line) -> Self {
        if self.len < self.visuals.len() {
            self.visuals[self.len] = Some(Visual::Line(line));
            self.len += 1;
        }
        self
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Visual> + '_ {
        self.visuals[..self.len].iter().flatten().copied()
    }
}

/// Fixed-capacity decoded impact pool reused for every received snapshot.
pub(crate) struct Pool {
    tracker: LegacyImpactTracker,
    events: Vec<LegacyImpactEvent>,
}

impl Pool {
    pub(crate) fn new() -> Self {
        Self {
            tracker: LegacyImpactTracker::new(),
            events: Vec::with_capacity(MAX_SNAPSHOT_IMPACTS),
        }
    }

    pub(crate) fn observe(&mut self, snapshot: &Snapshot) -> usize {
        self.tracker.observe(snapshot, &mut self.events);
        self.events.len()
    }

    pub(crate) fn event(&self, index: usize) -> Option<LegacyImpactEvent> {
        self.events.get(index).copied()
    }
}

/// Select the exact stock EFX graph(s) used by codemp. A zero-length plan is
/// intentional where codemp has no wall visual (Flechette alt-fire).
pub(crate) fn plan(event: LegacyImpactEvent) -> Plan {
    let direction = event.direction;
    match event.kind {
        LegacyImpactKind::SaberHit => match event.event_parameter {
            0 => Plan::new().effect("saber/saber_cut", direction),
            2 => Plan::new().effect("saber/blood_sparks_50_mp", direction),
            3 => Plan::new().effect("saber/blood_sparks_25_mp", direction),
            16 => Plan::new().repeated("saber/blood_sparks_mp", direction, 6),
            _ => Plan::new().repeated("saber/blood_sparks_mp", direction, 3),
        },
        LegacyImpactKind::SaberBlock => Plan::new().effect(
            if event.event_parameter == 0 {
                "blaster/deflect"
            } else {
                "saber/saber_block"
            },
            direction,
        ),
        // This is a screen-space latch, not an EFX primitive. The exact
        // cg_draw.c envelope and BSP visibility trace live in
        // jkr-client::LegacySaberClashFlare.
        LegacyImpactKind::SaberClashFlare => Plan::new(),
        LegacyImpactKind::DisruptorMainShot => disruptor_main_shot(event),
        LegacyImpactKind::DisruptorSniperShot => disruptor_sniper_shot(event),
        // `FX_DisruptorAltMiss` also lays a 4 s bezier smoke trail
        // (`fx_disruptor.c:89-139`); bezier primitives are not rendered yet.
        LegacyImpactKind::DisruptorSniperMiss => Plan::new().effect(
            if event.weapon != 0 {
                "disruptor/wall_impact"
            } else {
                "disruptor/alt_miss"
            },
            direction,
        ),
        LegacyImpactKind::DisruptorHit => Plan::new().effect(
            if event.weapon != 0 {
                "disruptor/flesh_impact"
            } else {
                "disruptor/wall_impact"
            },
            direction,
        ),
        LegacyImpactKind::MissileHitPlayer => missile_player(event, direction),
        LegacyImpactKind::MissileHitWall | LegacyImpactKind::MissileHitMetal => {
            missile_wall(event, direction)
        }
    }
}

/// `FX_DisruptorMainShot` (`fx_disruptor.c:35-44`).
fn disruptor_main_shot(event: LegacyImpactEvent) -> Plan {
    Plan::new().line(Line {
        start: event.start,
        end: event.origin,
        size: [0.1, 6.0],
        alpha: [1.0, 0.0],
        color: WHITE,
        lifetime_millis: 150,
        shader: "gfx/effects/redLine",
    })
}

/// `FX_DisruptorAltShot` (`fx_disruptor.c:63-81`): the sniper beam plus a
/// yellow core when the shot was fully charged.
fn disruptor_sniper_shot(event: LegacyImpactEvent) -> Plan {
    let plan = Plan::new().line(Line {
        start: event.start,
        end: event.origin,
        size: [0.1, 10.0],
        alpha: [1.0, 0.0],
        color: WHITE,
        lifetime_millis: 175,
        shader: "gfx/effects/redLine",
    });
    if !event.full_charge {
        return plan;
    }
    plan.line(Line {
        start: event.start,
        end: event.origin,
        size: [0.1, 7.0],
        alpha: [1.0, 0.0],
        color: YELLER,
        lifetime_millis: 150,
        shader: "gfx/misc/whiteline2",
    })
}

fn missile_player(event: LegacyImpactEvent, direction: [f32; 3]) -> Plan {
    let name = match (event.weapon, event.alternate) {
        (4 | 16 | 18, _) => "bryar/flesh_impact",
        (5 | 17, _) => "blaster/flesh_impact",
        (6, _) => "disruptor/alt_hit",
        (7, _) => "bowcaster/explosion",
        (8, true) => "repeater/concussion",
        (8, false) => "repeater/flesh_impact",
        (9, true) => "demp2/altdetonate",
        (9, false) => "demp2/flesh_impact",
        (10, _) => "flechette/flesh_impact",
        (11, _) => "rocket/explosion",
        (15, _) => "concussion/explosion",
        (12, _) => {
            return Plan::new()
                .effect("thermal/explosion", direction)
                .effect("thermal/shockwave", [0.0, 0.0, 1.0]);
        }
        _ => return Plan::new(),
    };
    Plan::new().effect(name, direction)
}

fn missile_wall(event: LegacyImpactEvent, direction: [f32; 3]) -> Plan {
    let name = match (event.weapon, event.alternate) {
        (4 | 16, true) if event.charge >= 4 => "bryar/wall_impact3",
        (4 | 16, true) if event.charge >= 2 => "bryar/wall_impact2",
        (4 | 16 | 18, _) => "bryar/wall_impact",
        (5 | 17, _) => "blaster/wall_impact",
        (6, _) => "disruptor/alt_miss",
        (7, _) => "bowcaster/explosion",
        (8, true) => "repeater/concussion",
        (8, false) => "repeater/wall_impact",
        (9, true) => "demp2/altdetonate",
        (9, false) => "demp2/wall_impact",
        (10, true) => return Plan::new(),
        (10, false) => "flechette/wall_impact",
        (11, _) => "rocket/explosion",
        (15, _) => "concussion/explosion",
        (12, _) => {
            return Plan::new()
                .effect("thermal/explosion", direction)
                .effect("thermal/shockwave", [0.0, 0.0, 1.0]);
        }
        _ => return Plan::new(),
    };
    Plan::new().effect(name, direction)
}
