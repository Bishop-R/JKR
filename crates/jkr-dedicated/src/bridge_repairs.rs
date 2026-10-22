//! Siege's repairs on this server (`TryHeal`, OpenJK `codemp/game/g_utils.c:1522-1578`,
//! [`jkr_game_jka::try_heal`]): what the use key reaches that a map marks with a
//! `healingclass` is mended by a player of that class — the emplaced guns
//! (`bridge_emplaced`), the breakable objectives (`bridge_breakables`), and here the
//! `misc_turret`s and the siege items. Vehicles, which stock repairs only from a zoomed
//! view (the use key boards them otherwise), are not.

use super::*;
use jkr_game_jka::try_heal::Heal;

/// `CHAN_AUTO`'s `EV_GENERAL_SOUND`.
const EV_GENERAL_SOUND: u32 = 76;
/// `SETANIM_TORSO`, `SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD`.
const SETANIM_TORSO: u8 = 1;
const OVERRIDE_HOLD: u8 = 1 | 2;

impl NativeGame {
    /// `bgSiegeClasses[client->siegeClass].name`: the Siege class `client` plays, empty
    /// for none.
    pub(in crate::bridge) fn healer_class(&mut self, client: usize) -> String {
        self.siege_class_of(client)
            .map(|class| class.name)
            .unwrap_or_default()
    }

    /// `TryHeal`'s `G_Sound`: a `healingsound` at `origin`.
    pub(in crate::bridge) fn healing_sound(&mut self, sound: &[u8], origin: [f32; 3], time: i32) {
        let index = self.sound_index_for(sound);
        let event = jkr_game_jka::event_entity::EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(index),
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        let _ = self.pool.spawn_temporary(event.state(), time, None);
    }

    /// `TryHeal` on what the use key reached, entity `number`, by `client`: a
    /// `misc_turret`'s base or a siege item. Whether it was that class's to mend.
    pub(super) fn repair_by_key(&mut self, client: usize, number: u16, time: i32) -> bool {
        self.repair_turret(client, number, time) || self.repair_siege_item(client, number, time)
    }

    /// `TryHeal` on the `misc_turret` whose base is entity `number`: the repair, the
    /// base's sound, the player held in the use pose.
    fn repair_turret(&mut self, client: usize, number: u16, time: i32) -> bool {
        let Some(index) = self
            .map_turrets
            .misc
            .iter()
            .position(|(base, _, _)| base.legacy_number() == number)
        else {
            return false;
        };
        let class = self.healer_class(client);
        let gametype = self.gametype;
        let turret = &mut self.map_turrets.misc[index].2;
        let healer = (!class.is_empty()).then_some(class.as_bytes());
        let Heal::Yes { repaired } =
            jkr_game_jka::map_turret::repair(turret, healer, gametype, time)
        else {
            return false;
        };
        let sound = turret.healing.sound.clone().filter(|_| repaired);
        let origin = turret.base.origin;
        self.publish_turret(number);
        self.repaired(client, sound, origin, time);
        true
    }

    /// `TryHeal` on siege item entity `number`: ten points every `healingrate`, its bar
    /// told, its sound on it, the player held in the use pose.
    fn repair_siege_item(&mut self, client: usize, number: u16, time: i32) -> bool {
        let class = self.healer_class(client);
        let gametype = self.gametype;
        let Some((index, placed)) = self.siege.as_mut().and_then(|siege| {
            siege
                .items
                .iter_mut()
                .enumerate()
                .find(|(_, placed)| placed.id.legacy_number() == number && !placed.item.freed)
        }) else {
            return false;
        };
        let item = &mut placed.item;
        let healer = (!class.is_empty()).then_some(class.as_bytes());
        let healed = jkr_game_jka::try_heal::try_heal(
            &mut item.healing,
            &mut item.health,
            item.max_health,
            healer,
            gametype,
            time,
        );
        let Heal::Yes { repaired } = healed else {
            return false;
        };
        let sound = item.healing.sound.clone().filter(|_| repaired);
        let origin = item.origin;
        self.publish_siege_item(index);
        self.repaired(client, sound, origin, time);
        true
    }

    /// A repair's end: its sound where it is heard, and the player held in the use pose.
    fn repaired(&mut self, client: usize, sound: Option<Vec<u8>>, at: [f32; 3], time: i32) {
        if let Some(sound) = sound {
            self.healing_sound(&sound, at, time);
        }
        if let Some(peer) = self.peer_mut(client) {
            hold_healer_pose(peer);
        }
    }
}

/// The pose a repair keeps its player in (`TryHeal`'s end): the use pose begun, or one
/// already held kept 500 ms more.
pub(in crate::bridge) fn hold_healer_pose(peer: &mut crate::peer::Peer) {
    match jkr_game_jka::try_heal::healer_pose(peer.state.torso_animation()) {
        Some(pose) => {
            peer.movement
                .set_animation_parts(SETANIM_TORSO, pose, OVERRIDE_HOLD);
            peer.movement.write_player_state(&mut peer.state);
        }
        None => {
            peer.state.set_raw_field(20, 500);
        }
    }
}
