//! What an instant-hit shot struck ([`ShotTarget`]) and the blow it deals: a pane of glass
//! or a breakable brush takes it through its own damage, anything else through `G_Damage`'s
//! usual routes ([`NativeGame::strike`]).

use super::*;
use jkr_game_jka::shot_target::ShotTarget;

/// `ENTITYNUM_WORLD`; the wire field `eType`.
const ENTITY_WORLD: u16 = 1_022;
const ES_TYPE: usize = 8;

impl NativeGame {
    /// `g_entities[number]` as the shot reads it: a player in the game or an NPC is a
    /// client; glass, breakable brushes and the other movers stop the shot; a body takes
    /// damage until it fades. Doors and buttons a map gave health take no damage here yet.
    pub(super) fn shot_target(&mut self, number: u16) -> ShotTarget {
        if number >= ENTITY_WORLD {
            return ShotTarget::default();
        }
        if let Some(peer) = self.peer(usize::from(number)) {
            return ShotTarget {
                client: true,
                takes_damage: peer.begun && peer.body_active(),
                glass_or_mover: false,
            };
        }
        if let Some(npc) = self.npcs.roster.begun().find(|npc| npc.number == number) {
            return ShotTarget {
                client: true,
                takes_damage: npc.takes_damage,
                glass_or_mover: false,
            };
        }
        if let Some(takes_damage) = self.stock.scenery.glass_takes_damage(number) {
            return ShotTarget {
                client: false,
                takes_damage,
                glass_or_mover: true,
            };
        }
        if let Some((_, brush)) = self
            .breakables
            .iter()
            .find(|(id, _)| id.legacy_number() == number)
        {
            return ShotTarget {
                client: false,
                takes_damage: brush.takes_damage,
                glass_or_mover: true,
            };
        }
        if let Some(slot) = self.corpse_slot(number) {
            return ShotTarget {
                client: false,
                takes_damage: self.corpses.takes_damage(slot, self.last_frame_time),
                glass_or_mover: false,
            };
        }
        // Turrets, emplaced guns and shields, as a blast finds them.
        self.gather_splash_targets();
        let takes_damage = self
            .splash_targets
            .iter()
            .any(|target| target.number == number && target.takes_damage);
        let mover = self
            .pool
            .legacy_id(number)
            .and_then(|id| self.pool.state(id))
            .is_some_and(|state| {
                state.raw_field(ES_TYPE) == Some(jkr_game_jka::breakables::ET_MOVER)
            });
        ShotTarget {
            client: false,
            takes_damage,
            glass_or_mover: mover,
        }
    }

    /// The shot's blow on `number` by `shooter`: a pane or a breakable brush through its own
    /// damage, anything else through [`Self::strike`], counted for the shooter's accuracy
    /// when `count`. Returns what the blow came to.
    pub(super) fn shot_hurt(
        &mut self,
        shooter: usize,
        number: u16,
        request: DamageRequest,
        count: bool,
    ) -> Damaged {
        if self.hurt_brush(
            number,
            request.damage,
            request.means,
            shooter as u16,
            request.level_time,
        ) {
            return Damaged::default();
        }
        self.strike(shooter, usize::from(number), request, count).0
    }
}
