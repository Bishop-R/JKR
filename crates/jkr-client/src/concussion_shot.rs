//! `EV_CONC_ALT_IMPACT` geometry from `CG_EntityEvent` (codemp).

use jkr_protocol::EntityState;

/// The server supplies the muzzle, displacement, ring axis and impact normal.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    /// Beam muzzle (`origin2`).
    pub start: [f32; 3],
    /// Stock's beam ends at the final 64-unit ring, not necessarily the wall.
    pub beam_end: [f32; 3],
    /// Impact position (`lerpOrigin` of the temporary entity).
    pub impact: [f32; 3],
    /// `ByteToDir(eventParm)`.
    pub normal: [f32; 3],
    /// Authored ring orientation (`angles2`).
    pub ring_direction: [f32; 3],
    direction: [f32; 3],
    rings: usize,
}

impl Shot {
    /// Decode a finite shot. A zero-length reference shot leaves its endpoint
    /// uninitialized; use its muzzle and emit no rings for that malformed case.
    pub fn decode(entity: &EntityState) -> Option<Self> {
        let delta = entity.angles();
        let distance = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
        let start = entity.origin2();
        let impact = entity.trajectory_base();
        let ring_direction =
            [82, 51, 84].map(|field| f32::from_bits(entity.raw_field(field).unwrap_or(0)));
        if !distance.is_finite()
            || !start
                .iter()
                .chain(&impact)
                .chain(&ring_direction)
                .all(|v| v.is_finite())
        {
            return None;
        }
        let direction = if distance > 0.0 {
            let inverse = 1.0 / distance;
            delta.map(|v| v * inverse)
        } else {
            [0.0; 3]
        };
        let rings = (distance / 64.0).ceil() as usize;
        let last = rings.saturating_sub(1) as f32 * 64.0;
        Some(Self {
            start,
            beam_end: std::array::from_fn(|i| start[i] + direction[i] * last),
            impact,
            normal: crate::legacy_byte_to_direction(entity.event_parameter()),
            ring_direction,
            direction,
            rings,
        })
    }

    /// Ordered ring positions. The caller bounds submissions by its effect capacity.
    pub fn rings(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        (0..self.rings).map(|index| {
            let distance = index as f32 * 64.0;
            std::array::from_fn(|i| self.start[i] + self.direction[i] * distance)
        })
    }
}
