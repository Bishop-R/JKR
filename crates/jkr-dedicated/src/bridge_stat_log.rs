//! The statistics log on this server ([`jkr_game_jka::stat_log`]): tallied as stock
//! tallies it whatever `g_statLog` says, and written at the level's end while it is on.
//! Only players are tallied; an NPC's shots, blows and kills count for nobody. A blow is
//! counted only when what it struck lives through it: `G_Damage` returns after a `die`
//! before it logs, so a killing blow (or one on a body) adds no damage, only the kill.

use super::NativeGame;
use jkr_game_jka::damage::Attacker;
use jkr_game_jka::items::{ITEMS, Kind};
use jkr_game_jka::map_turret_world::ObjectBlow;
use std::io::Write;

impl NativeGame {
    /// `G_LogWeaponOutput` (`G_ShutdownGame`): with `g_statLog` on, the game's totals in
    /// the game log and every player's tables appended to `g_statLogFile` in the home
    /// directory.
    pub(super) fn write_stat_log(&mut self) {
        if self.cvars.integer(b"g_statLog") == 0 {
            return;
        }
        for line in self.stat_log.summary() {
            self.log(&line);
        }
        let name = String::from_utf8_lossy(self.cvars.string(b"g_statLogFile")).into_owned();
        let Some(mut file) = self.config_files.append_home(&name) else {
            return;
        };
        let tables = self.stat_log.tables(&self.identity.mapname, |client| {
            self.peer(client)
                .map_or_else(|| b"<Unknown>".to_vec(), |peer| peer.name.clone())
        });
        // A log that cannot be written is not the match's concern.
        let _ = file.write_all(&tables);
    }

    /// `Pickup_Weapon`'s, `Pickup_Powerup`'s and `Pickup_Holdable`'s tallies: a weapon
    /// counted, a holdable or a powerup the player lacked (`had_powerup` false) only
    /// putting it in the tables.
    pub(super) fn tally_pickup(&mut self, client: usize, item: usize, had_powerup: bool) {
        let Some(item) = ITEMS.get(item) else {
            return;
        };
        match item.kind {
            Kind::Weapon => self.stat_log.pickup(client, item.tag as u32),
            Kind::Holdable => self.stat_log.touch(client),
            Kind::Powerup if !had_powerup => self.stat_log.touch(client),
            _ => {}
        }
    }

    /// `G_LogWeaponDamage`: what a player's blow took from anything it struck.
    pub(super) fn tally_damage(&mut self, attacker: Option<Attacker>, means: u32, take: i32) {
        let attacker = attacker
            .filter(|attacker| !attacker.npc)
            .map(|attacker| attacker.client);
        self.tally_damage_by(attacker, means, take);
    }

    /// [`Self::tally_damage`] for a blow on a thing that is no client.
    pub(super) fn tally_object_damage(&mut self, blow: &ObjectBlow, take: i32) {
        let attacker = blow
            .attacker
            .filter(|attacker| attacker.player)
            .map(|attacker| attacker.number);
        self.tally_damage_by(attacker, blow.means, take);
    }

    /// [`Self::tally_damage`] for a blow by entity `attacker`, counted when it is a player.
    pub(super) fn tally_damage_by(&mut self, attacker: Option<u16>, means: u32, take: i32) {
        if take == 0 {
            return;
        }
        if let Some(client) = attacker.map(usize::from)
            && self.peer(client).is_some()
        {
            self.stat_log.damage(client, means, take);
        }
    }

    /// `player_die`'s tallies for a player's death: the killer's kill when a player
    /// killed it (itself included), and the weapon it held.
    pub(super) fn tally_death(&mut self, client: usize, killer: Option<u16>, means: u32) {
        self.tally_kill(killer, means);
        if let Some(weapon) = self.peer(client).map(|peer| peer.state.weapon()) {
            self.stat_log.death(client, u32::from(weapon));
        }
    }

    /// `G_LogWeaponKill`: a kill by `killer` when it is a player.
    pub(super) fn tally_kill(&mut self, killer: Option<u16>, means: u32) {
        if let Some(killer) = killer.map(usize::from)
            && self.peer(killer).is_some()
        {
            self.stat_log.kill(killer, means);
        }
    }
}
