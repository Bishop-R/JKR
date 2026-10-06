//! What an instant-hit shot — the disruptor's, the concussion rifle's beam — makes of the
//! entity it struck, as `WP_DisruptorMainFire`, `WP_DisruptorAltFire` and
//! `WP_FireConcussionAlt` read `g_entities[tr.entityNum]`.

/// The struck entity's part in the shot. The world, and nothing, are none of these.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShotTarget {
    /// `client`: a player or an NPC.
    pub client: bool,
    /// `takedamage`.
    pub takes_damage: bool,
    /// `SVF_GLASS_BRUSH` or `ET_MOVER`: glass, a breakable brush, a door, a platform. The
    /// scoped disruptor shot and the concussion beam stop there.
    pub glass_or_mover: bool,
}
