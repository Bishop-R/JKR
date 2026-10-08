//! What a blow tells its attacker (`G_Damage`, `g_combat.c:4920-4932`): its hit counter
//! (`PERS_HITS`, one more for a foe, one less for a teammate), which clients with hit
//! sounds listen to, and what it struck had left (`PERS_ATTACKEE_ARMOR`, health and
//! armour). jaPRO's `g_stopHealthESP` (archived, announced, 0 as in jaPRO;
//! `g_combat.c:5448-5454` and `g_active.c:830-850` of jaPRO) keeps a cheat from reading
//! health off the wire: the struck one's numbers become a counter falling from 200 by 5,
//! and a pain event carries 50 instead of the health (1) or gives way to the pain sound
//! the health picks (2 and up).

use super::*;

/// `PERS_HITS`, `PERS_ATTACKEE_ARMOR`.
const PERS_HITS: usize = 1;
const PERS_ATTACKEE_ARMOR: usize = 7;
pub(super) const STOP_HEALTH_ESP: &[u8] = b"g_stopHealthESP";

impl NativeGame {
    /// A blow by player `client` struck a client: `hits` on its counter, and what the
    /// struck one had (`attackee`, `(health << 8) | armour`).
    pub(super) fn record_hit(&mut self, client: usize, hits: i32, attackee: Option<u32>) {
        let hide = self.cvars.integer(STOP_HEALTH_ESP) != 0;
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let persistent = &mut peer.state.persistent;
        persistent[PERS_HITS] = (persistent[PERS_HITS] as i32 + hits) as u32;
        persistent[PERS_ATTACKEE_ARMOR] = if !hide {
            attackee.unwrap_or(0)
        } else if persistent[PERS_ATTACKEE_ARMOR] as i32 > 0 {
            persistent[PERS_ATTACKEE_ARMOR] - 5
        } else {
            200
        };
    }
}

impl NativeGame {
    /// `G_EntitySound(player, CHAN_VOICE, G_SoundIndex(name))`: `g_stopHealthESP`'s pain
    /// sound in place of the pain event.
    pub(super) fn pain_sound(&mut self, client: usize, name: &[u8], level_time: i32) {
        let Some(origin) = self.peer(client).map(|peer| peer.state.origin()) else {
            return;
        };
        self.voice_sound(client as u16, origin, name, level_time);
    }

    /// `G_EntitySound(ent, CHAN_VOICE, G_SoundIndex(name))` for entity `number` at `origin`
    /// (`r.currentOrigin`).
    pub(super) fn voice_sound(
        &mut self,
        number: u16,
        origin: [f32; 3],
        name: &[u8],
        level_time: i32,
    ) {
        const CHAN_VOICE: u32 = 3;
        let told = &mut self.told;
        let index = self.sounds.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        });
        let mut sound = jkr_game_jka::knockdown::entity_sound(origin, number, CHAN_VOICE);
        sound.parameter = u32::from(index);
        let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Identity, NativeGame};
    use super::{PERS_ATTACKEE_ARMOR, PERS_HITS, STOP_HEALTH_ESP};
    use jkr_game_jka::damage::{Attacker, DamageRequest};
    use jkr_network::LegacyGameHost;
    use jkr_protocol::UserCommand;

    /// A blow through `G_Damage`'s own path counts on the attacker: a hit, and the struck
    /// one's health and armour — or, with `g_stopHealthESP`, a counter from 200 down.
    #[test]
    fn a_blow_counts_on_its_attacker() {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        for (client, name) in [(0, "a"), (1, "b")] {
            let userinfo = format!(
                "\\name\\{name}\\rate\\25000\\snaps\\40\\model\\kyle/default\\forcepowers\\7-1-032330000000001333\\color1\\4\\color2\\4\\handicap\\100\\cg_predictItems\\1\\saber1\\single_1\\saber2\\none\\char_color_red\\255\\char_color_green\\255\\char_color_blue\\255\\ip\\127.0.0.{}:1",
                client + 1
            );
            game.client_connect(client, userinfo.as_bytes()).unwrap();
            game.enter_world(client, &UserCommand::default(), 100);
        }
        let blow = DamageRequest {
            level_time: 200,
            attacker: Some(Attacker {
                npc: false,
                client: 0,
                max_health: 100,
                team: 0,
                saber_knockback: [1.0; 4],
            }),
            direction: None,
            point: None,
            damage: 10,
            flags: 0,
            means: 3,
        };
        let (health, armor) = {
            let victim = game.peer(1).unwrap();
            (victim.health as u32, victim.state.stats[5])
        };
        let _ = game.hurt(1, blow);
        let counters = |game: &NativeGame| {
            let state = &game.peer(0).unwrap().state;
            (
                state.persistent[PERS_HITS],
                state.persistent[PERS_ATTACKEE_ARMOR],
            )
        };
        assert_eq!(counters(&game), (1, (health << 8) | armor));
        game.cvars.set(STOP_HEALTH_ESP, b"1");
        let _ = game.hurt(1, blow);
        let _ = game.hurt(1, blow);
        assert_eq!(counters(&game), (3, (health << 8 | armor) - 10));
    }
}
