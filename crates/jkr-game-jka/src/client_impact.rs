//! A player breaking glass and weak breakables by running into them, which only the
//! game's own move does (`PM_ClientImpact`, `bg_slidemove.c:604-641`;
//! `Client_CheckImpactBBrush`, `g_active.c:419-448`; `DoImpact`, `g_active.c:225-417`).
//!
//! A player moving at 100 or more that has been off the ground for more than a tenth of a
//! second, striking an easily broken brush — a pane (`func_glass`), a breakable of glass,
//! glass and metal or grating, a THIN breakable, or a breakable with ten health or less —
//! hurts it through `G_Damage` as `MOD_CRUSH` by its speed over 25: less as it strikes
//! off the brush's middle, a third in water, nothing under one. A brush the blow
//! breaks and that leaves the world at once (a pane, or a breakable without a `delay`) is
//! passed through: the move's later traces no longer meet it.
//!
//! The move judges each blow as it lands ([`Strikes::strike`]); the game deals them once
//! the move is over ([`Strike`]), in order. Both read the same health, so what the move
//! passed through is what the game breaks.

use crate::breakables::{Breakable, FL_BBRUSH, MAT_GLASS, may_hurt};
use crate::map_scenery::Glass;
use crate::means_of_death::MOD_CRUSH;
use crate::player_angle_math::normalize;

/// `MAT_GLASS_METAL` and `MAT_GRATE1` (`q_shared.h:503-523`).
const MAT_GLASS_METAL: u32 = 6;
const MAT_GRATE1: u32 = 12;
/// `func_breakable`'s THIN: "can be broken by impact damage, like glass".
const THIN: u32 = 8;
/// `CONTENTS_WATER` (`codemp/game/surfaceflags.h:36`).
pub const CONTENTS_WATER: u32 = 0x4;

/// A brush as a player's blow finds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brittle {
    /// `r.currentOrigin`: the world's origin for a brush drawn where it stands.
    pub origin: [f32; 3],
    /// `r.absmax`, where `DoImpact` asks whether the brush stands in water.
    pub top: [f32; 3],
    /// Its health before the move.
    pub health: i32,
    /// Easily broken whatever its health: a pane, glass, grating or THIN.
    pub always_brittle: bool,
    /// `FL_BBRUSH`: a breakable, which ten health or less makes easily broken too.
    pub breakable: bool,
    /// `takedamage`: whether the blow goes to `G_Damage` at all (`DoImpact` otherwise
    /// knocks the brush back, which moves no standing brush).
    pub takes_damage: bool,
    /// Whether `G_Damage` lets the blow hurt it: not saber-only, not before a Siege round.
    pub hurt: bool,
    /// Whether, broken, it leaves the world at once: a pane always, a breakable without a
    /// `delay`.
    pub leaves_at_once: bool,
}

impl Brittle {
    /// A `func_breakable` (`InitBBrush`'s `FL_BBRUSH`), with `round_waiting` for a Siege
    /// round that has not begun (`G_Damage` hurts nothing then).
    pub fn breakable(brush: &Breakable, round_waiting: bool) -> Self {
        Self {
            origin: [0.0; 3],
            top: brush.bounds.1.map(|value| value + 1.0),
            health: brush.health,
            always_brittle: matches!(brush.material, MAT_GLASS | MAT_GLASS_METAL | MAT_GRATE1)
                || brush.spawnflags & THIN != 0,
            breakable: brush.flags & FL_BBRUSH != 0,
            takes_damage: brush.takes_damage,
            hurt: !round_waiting && may_hurt(brush, MOD_CRUSH, false),
            leaves_at_once: brush.delay == 0,
        }
    }

    /// A `func_glass` (`SVF_GLASS_BRUSH`): `GlassDie` frees it at once.
    pub fn pane(pane: &Glass, round_waiting: bool) -> Self {
        Self {
            origin: [0.0; 3],
            top: pane.bounds.1.map(|value| value + 1.0),
            health: pane.health,
            always_brittle: true,
            breakable: false,
            takes_damage: pane.takes_damage && !pane.shattered,
            hurt: !round_waiting && pane.takes_damage && !pane.shattered,
            leaves_at_once: true,
        }
    }
}

/// One blow a player's move dealt, for the game to deal once the move is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub entity: u16,
    /// What `G_Damage` is asked for: the blow's force, truncated.
    pub damage: i32,
    /// The player's velocity at the blow, `G_Damage`'s direction (which it normalizes).
    pub velocity: [f32; 3],
    /// Whether `G_Damage` lets it hurt the brush at all ([`Brittle::hurt`]).
    pub hurt: bool,
    /// Whether the blow broke the brush (its health gone), and whether that took it out of
    /// the world.
    pub broke: bool,
    pub broke_out: bool,
}

/// `PM_ClientImpact`'s gate (`bg_slidemove.c:623-626`): a blow at all — 100 or more fast,
/// and off the ground for more than a tenth of a second at `level_time`.
pub fn strikes(velocity: [f32; 3], last_on_ground: i32, level_time: i32) -> bool {
    length(velocity) >= 100.0 && last_on_ground.wrapping_add(100) < level_time
}

/// `DoImpact(player, brush, qfalse)`: the damage the blow asks `G_Damage` for, if it lands.
/// A player has no `mass`, so ten; `easyBreakBrush` starts true and nothing clears it, so
/// the magnitude is doubled. The force is the share of the blow along the way to the
/// brush's middle (all of it for a brush drawn where it stands), a third of it in water;
/// under one (a brush is no client) nothing is dealt.
pub fn blow(
    velocity: [f32; 3],
    player_origin: [f32; 3],
    brush: &Brittle,
    in_water: impl FnOnce() -> bool,
) -> Option<i32> {
    // `VectorLength(velocity) * my_mass / 10`, in single precision.
    let magnitude = length(velocity) * 10.0 / 10.0 * 2.0;
    if magnitude < 100.0 {
        return None;
    }
    let mut along = velocity;
    normalize(&mut along);
    let toward = if brush.origin == [0.0; 3] {
        along
    } else {
        let mut toward: [f32; 3] =
            std::array::from_fn(|axis| brush.origin[axis] - player_origin[axis]);
        normalize(&mut toward);
        toward
    };
    let dot = along[0] * toward[0] + along[1] * toward[1] + along[2] * toward[2];
    // `dot >= 0.2` compares with a double.
    let mut force = if f64::from(dot) >= 0.2 { dot } else { 0.0 };
    force *= magnitude / 50.0;
    if in_water() {
        force /= 3.0;
    }
    (force >= 1.0).then_some(force as i32)
}

/// `VectorLength`: the sum in single precision, its square root rounded to a float.
fn length(vector: [f32; 3]) -> f32 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

/// A player's blows within one move, kept across its slides: a brush struck twice takes
/// both blows, and one a blow broke out of the world stays out of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Strikes {
    pub blows: Vec<Strike>,
}

impl Strikes {
    /// Forgets the last move's blows, keeping the room for the next.
    pub fn clear(&mut self) {
        self.blows.clear();
    }

    /// Whether a blow of this move broke `entity` out of the world.
    pub fn gone(&self, entity: u16) -> bool {
        self.blows
            .iter()
            .any(|blow| blow.entity == entity && blow.broke_out)
    }

    /// Whether any blow of this move broke anything out of the world.
    pub fn any_gone(&self) -> bool {
        self.blows.iter().any(|blow| blow.broke_out)
    }

    /// `Client_CheckImpactBBrush` and `DoImpact` for a blow on `entity`, struck at
    /// `velocity` by a player last linked at `player_origin`: whether the brush is out of
    /// the world now, so that the move goes on through it. A brush this move already broke
    /// takes no more blows (`funcBBrushDie` turns `takedamage` off), nor does one without
    /// `takedamage` (`G_ApplyKnockback` moves no standing brush); a blow `G_Damage` turns
    /// away (saber only) is kept, hurting nothing.
    pub fn strike(
        &mut self,
        entity: u16,
        brush: &Brittle,
        velocity: [f32; 3],
        player_origin: [f32; 3],
        in_water: impl FnOnce() -> bool,
    ) -> bool {
        let earlier = self.blows.iter().filter(|blow| blow.entity == entity);
        let health = brush.health
            - earlier
                .clone()
                .filter(|blow| blow.hurt)
                .map(|blow| blow.damage)
                .sum::<i32>();
        let brittle = brush.always_brittle || (brush.breakable && health <= 10);
        if !brittle {
            return false;
        }
        let Some(damage) = blow(velocity, player_origin, brush, in_water) else {
            return false;
        };
        if !brush.takes_damage || earlier.clone().any(|blow| blow.broke) {
            return false;
        }
        let broke = brush.hurt && health - damage <= 0;
        let broke_out = broke && brush.leaves_at_once;
        self.blows.push(Strike {
            entity,
            damage,
            velocity,
            hurt: brush.hurt,
            broke,
            broke_out,
        });
        broke_out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(health: i32) -> Brittle {
        Brittle {
            origin: [0.0; 3],
            top: [64.0, 64.0, 128.0],
            health,
            always_brittle: true,
            breakable: false,
            takes_damage: true,
            hurt: true,
            leaves_at_once: true,
        }
    }

    #[test]
    fn the_gate_wants_speed_and_a_tenth_of_a_second_off_the_ground() {
        assert!(strikes([100.0, 0.0, 0.0], 1_000, 1_101));
        assert!(!strikes([100.0, 0.0, 0.0], 1_000, 1_100));
        assert!(!strikes([99.9, 0.0, 0.0], 0, 5_000));
    }

    #[test]
    fn a_blow_is_the_speed_over_twenty_five() {
        let brush = pane(1);
        assert_eq!(
            blow([300.0, 0.0, 0.0], [0.0; 3], &brush, || false),
            Some(12)
        );
        assert_eq!(blow([300.0, 0.0, 0.0], [0.0; 3], &brush, || true), Some(4));
        assert_eq!(blow([40.0, 0.0, 0.0], [0.0; 3], &brush, || false), None);
        // A brush with an origin takes the share of the blow along the way to it.
        let mut offset = brush;
        offset.origin = [0.0, 100.0, 0.0];
        assert_eq!(blow([300.0, 0.0, 0.0], [0.0; 3], &offset, || false), None);
    }

    #[test]
    fn a_broken_brush_is_out_of_the_world_and_takes_no_more_blows() {
        let fast = [400.0, 0.0, 0.0];
        let mut strikes = Strikes::default();
        assert!(strikes.strike(9, &pane(1), fast, [0.0; 3], || false));
        assert!(strikes.gone(9) && strikes.any_gone());
        assert!(!strikes.strike(9, &pane(1), fast, [0.0; 3], || false));
        assert_eq!(strikes.blows.len(), 1);
        // A strong breakable is no brittle one.
        let mut strong = pane(30);
        (strong.always_brittle, strong.breakable) = (false, true);
        assert!(!strikes.strike(10, &strong, fast, [0.0; 3], || false));
        assert_eq!(strikes.blows.len(), 1);
        // A weak one with a delay breaks where it stands, and takes no second blow.
        let mut delayed = strong;
        (delayed.health, delayed.leaves_at_once) = (5, false);
        assert!(!strikes.strike(11, &delayed, fast, [0.0; 3], || false));
        assert!(!strikes.strike(11, &delayed, fast, [0.0; 3], || false));
        assert_eq!(strikes.blows.len(), 2);
        assert!(strikes.blows[1].broke && !strikes.blows[1].broke_out);
        // Two blows of 16 on a THIN brush of 30 add up.
        assert!(!strikes.strike(12, &pane(30), fast, [0.0; 3], || false));
        assert!(strikes.strike(12, &pane(30), fast, [0.0; 3], || false));
        assert_eq!(strikes.blows[3].damage, 16);
        // Saber only: struck, kept, never hurt; without `takedamage`, not struck at all.
        let mut sabers_only = pane(1);
        sabers_only.hurt = false;
        assert!(!strikes.strike(13, &sabers_only, fast, [0.0; 3], || false));
        assert!(!strikes.strike(13, &sabers_only, fast, [0.0; 3], || false));
        assert_eq!(strikes.blows.len(), 6);
        assert!(!strikes.blows[5].hurt && !strikes.blows[5].broke);
        let mut invincible = pane(1);
        invincible.takes_damage = false;
        assert!(!strikes.strike(14, &invincible, fast, [0.0; 3], || false));
        assert_eq!(strikes.blows.len(), 6);
    }
}
