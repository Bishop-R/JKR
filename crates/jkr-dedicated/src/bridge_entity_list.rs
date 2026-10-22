//! The developer's `entitylist` (`Svcmd_EntityList_f`): every client in the game and every
//! entity in use, by number, each with its `eType` and the classname the reference gives
//! it. This server keeps no entity for what is never sent (spawn points, triggers,
//! targets), so those are not listed, and kinds it does not name here print no classname.

use super::*;
use jkr_game_jka::means_of_death::*;

/// `ET_PLAYER`, `ET_ITEM`, `ET_TEAM` and `ET_EVENTS`.
const ET_PLAYER: u32 = 1;
const ET_ITEM: u32 = 2;
const ET_TEAM: u32 = 14;
const ET_EVENTS: u32 = 18;
/// `WP_BLASTER` and `WP_TURRET`, as missiles carry them in `s.weapon`.
const WP_BLASTER: u8 = 5;
const WP_TURRET: u8 = 18;
/// `MAX_CLIENTS`: a missile owned below this was fired by a client.
const MAX_CLIENTS: u16 = 32;

impl NativeGame {
    /// `Svcmd_EntityList_f`, one line per entity: its number, its type's name and its
    /// classname.
    pub(super) fn entity_list(&self, print: &mut dyn FnMut(&[u8])) {
        let mut rows: Vec<(u16, u32, Option<EntityId>)> = (0..self.players.places())
            .filter(|&client| self.peer(client).is_some())
            .map(|client| (client as u16, ET_PLAYER, None))
            .collect();
        rows.extend(
            self.pool
                .entities()
                .filter(|(_, in_use, _, _)| *in_use)
                .map(|(id, _, state, _)| {
                    let kind = u32::from(state.entity_type());
                    (id.legacy_number(), kind, Some(id))
                }),
        );
        rows.sort_by_key(|(number, _, _)| *number);
        for (number, kind, id) in rows {
            let classname = match id {
                Some(id) => self.classname_of(id, kind),
                None => "player",
            };
            print(format!("{number:3}:{}{classname}\n", entity_type_name(kind)).as_bytes());
        }
    }

    /// The classname of pool entity `id`, of `eType` `kind`, where this server tracks it.
    fn classname_of(&self, id: EntityId, kind: u32) -> &'static str {
        use jkr_game_jka::mines::Kind as Charge;
        use jkr_game_jka::movers::MoverKind;
        use jkr_game_jka::usables::Kind as Usable;
        let number = id.legacy_number();
        if kind >= ET_EVENTS {
            return "tempEntity";
        }
        if self.pool.in_body_queue(id) {
            return "bodyque";
        }
        if self.npcs.roster.is_npc(number) {
            return "NPC";
        }
        if kind == ET_ITEM {
            let state = self.pool.state(id);
            let item = state.and_then(|state| usize::try_from(state.model_index()).ok());
            return item
                .and_then(|item| jkr_game_jka::items::ITEMS.get(item))
                .map_or("", |item| item.classname);
        }
        let mut peers = (0..self.players.places()).filter_map(|client| self.peer(client));
        if peers.any(|peer| peer.state.saber_entity_num() == number) {
            return "lightsaber";
        }
        if let Some(index) = position_of(&self.missiles, id) {
            return missile_classname(&self.missiles[index].1);
        }
        if let Some(index) = position_of(&self.charges, id) {
            return match self.charges[index].1.kind {
                Charge::TripMine | Charge::ProximityMine => "laserTrap",
                Charge::DetPack => "detpack",
            };
        }
        if position_of(&self.spheres, id).is_some() {
            return "demp2_alt_proj";
        }
        if let Some(index) = position_of(&self.doors, id) {
            return match self.doors[index].1.kind {
                MoverKind::Door => "func_door",
                MoverKind::Plat => "func_plat",
                MoverKind::Button => "func_button",
            };
        }
        if position_of(&self.breakables, id).is_some() {
            return "func_breakable";
        }
        match self.stock.usables.get(number).map(|usable| usable.kind) {
            Some(Usable::Usable) => "func_usable",
            Some(Usable::Wall) => "func_wall",
            None => "",
        }
    }
}

/// The classname the reference's firing function gives a missile, told from what the
/// missile carries: the bowcaster's main bolt is `bowcaster_alt_proj` and its bouncing
/// alternate `bowcaster_proj`, as `g_weapon.c` names them; the E-Web's shot, fired by a
/// client, is a `generic_proj` beside the turbo laser's `turbo_proj`.
fn missile_classname(missile: &Missile) -> &'static str {
    let weapon = missile.state.weapon();
    match missile.method_of_death {
        MOD_BRYAR_PISTOL | MOD_BRYAR_PISTOL_ALT => "bryar_proj",
        MOD_BLASTER if weapon == WP_BLASTER => "blaster_proj",
        MOD_TURBLAST if missile.owner >= MAX_CLIENTS => "turbo_proj",
        MOD_BOWCASTER if missile.bounces => "bowcaster_proj",
        MOD_BOWCASTER => "bowcaster_alt_proj",
        MOD_REPEATER => "repeater_proj",
        MOD_REPEATER_ALT => "repeater_alt_proj",
        MOD_DEMP2 => "demp2_proj",
        MOD_FLECHETTE => "flech_proj",
        MOD_FLECHETTE_ALT_SPLASH => "flech_alt",
        MOD_ROCKET | MOD_ROCKET_HOMING => "rocket_proj",
        MOD_THERMAL => "thermal_detonator",
        MOD_CONC => "conc_proj",
        MOD_VEHICLE if weapon == WP_TURRET => "emplaced_gun_proj",
        MOD_VEHICLE => "vehicle_proj",
        MOD_TARGET_LASER => "turret_proj",
        _ => "generic_proj",
    }
}

/// Where entity `id` is in `list`.
fn position_of<T>(list: &[(EntityId, T)], id: EntityId) -> Option<usize> {
    list.iter().position(|(entity, _)| *entity == id)
}

/// `Svcmd_EntityList_f`'s name for an `eType`, padded as it pads it; `ET_TEAM`, an event
/// and anything unknown print as their number.
fn entity_type_name(kind: u32) -> String {
    const NAMES: [&str; 18] = [
        "ET_GENERAL",
        "ET_PLAYER",
        "ET_ITEM",
        "ET_MISSILE",
        "ET_SPECIAL",
        "ET_HOLOCRON",
        "ET_MOVER",
        "ET_BEAM",
        "ET_PORTAL",
        "ET_SPEAKER",
        "ET_PUSH_TRIGGER",
        "ET_TELEPORT_TRIGGER",
        "ET_INVISIBLE",
        "ET_NPC",
        "ET_TEAM",
        "ET_BODY",
        "ET_TERRAIN",
        "ET_FX",
    ];
    match NAMES.get(kind as usize) {
        Some(name) if kind != ET_TEAM => format!("{name:<20}"),
        _ => format!("{kind:<3}                "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Named types pad to twenty columns; `ET_TEAM` and events print their number in three
    /// and sixteen spaces, as `"%-3i                "` does.
    #[test]
    fn entity_types_print_as_the_reference_prints_them() {
        assert_eq!(entity_type_name(0), "ET_GENERAL          ");
        assert_eq!(entity_type_name(11), "ET_TELEPORT_TRIGGER ");
        assert_eq!(entity_type_name(17), "ET_FX               ");
        assert_eq!(entity_type_name(14), "14                 ");
        assert_eq!(entity_type_name(124), "124                ");
        assert_eq!(entity_type_name(14).len(), 19);
    }
}
