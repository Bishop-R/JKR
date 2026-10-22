//! The statistics log (`g_log.c`, `LOGGING_WEAPONS`): for each player, the weapons it
//! picked up, fired and died holding and the time it spent with them, and the damage it
//! dealt and the kills it made by means of death. With `g_statLog` on, the level's end
//! prints the game's totals in the game log and appends every player's tables to
//! `g_statLogFile`.
//!
//! Two of stock's tables are older than the game's weapons: its weapon names stop at the
//! det pack and are one off from the melee column on (the melee column is headed
//! "Saber"), the last three columns `(null)`; its means-of-death map credits the
//! concussion rifle's damage to no weapon, and the last means has no name. Here each
//! column carries its own weapon's or means' name, and the concussion rifle its damage.
//! Stock also counts the powerups, holdables and victims of each player, but never
//! writes them: a pickup of either only puts the player in the tables ([`StatLog::touch`]).

use std::collections::BTreeMap;

/// `WP_NUM_WEAPONS`.
pub const WEAPON_COUNT: usize = 19;
/// `MOD_MAX`.
pub const MEANS_COUNT: usize = 43;

/// `weaponNameFromIndex`, one name for each weapon.
const WEAPON_NAMES: [&str; WEAPON_COUNT] = [
    "No Weapon",
    "Stun Baton",
    "Melee",
    "Saber",
    "Bryar Pistol",
    "Blaster",
    "Disruptor",
    "Bowcaster",
    "Repeater",
    "Demp2",
    "Flechette",
    "Concussion Rifle",
    "Rocket Launcher",
    "Thermal",
    "Tripmine",
    "Detpack",
    "Old Bryar Pistol",
    "Emplaced gun",
    "Turret",
];

/// `modNames`, with `MOD_TEAM_CHANGE` named.
const MEANS_NAMES: [&str; MEANS_COUNT] = [
    "MOD_UNKNOWN",
    "MOD_STUN_BATON",
    "MOD_MELEE",
    "MOD_SABER",
    "MOD_BRYAR_PISTOL",
    "MOD_BRYAR_PISTOL_ALT",
    "MOD_BLASTER",
    "MOD_TURBLAST",
    "MOD_DISRUPTOR",
    "MOD_DISRUPTOR_SPLASH",
    "MOD_DISRUPTOR_SNIPER",
    "MOD_BOWCASTER",
    "MOD_REPEATER",
    "MOD_REPEATER_ALT",
    "MOD_REPEATER_ALT_SPLASH",
    "MOD_DEMP2",
    "MOD_DEMP2_ALT",
    "MOD_FLECHETTE",
    "MOD_FLECHETTE_ALT_SPLASH",
    "MOD_ROCKET",
    "MOD_ROCKET_SPLASH",
    "MOD_ROCKET_HOMING",
    "MOD_ROCKET_HOMING_SPLASH",
    "MOD_THERMAL",
    "MOD_THERMAL_SPLASH",
    "MOD_TRIP_MINE_SPLASH",
    "MOD_TIMED_MINE_SPLASH",
    "MOD_DET_PACK_SPLASH",
    "MOD_VEHICLE",
    "MOD_CONC",
    "MOD_CONC_ALT",
    "MOD_FORCE_DARK",
    "MOD_SENTRY",
    "MOD_WATER",
    "MOD_SLIME",
    "MOD_LAVA",
    "MOD_CRUSH",
    "MOD_TELEFRAG",
    "MOD_FALLING",
    "MOD_SUICIDE",
    "MOD_TARGET_LASER",
    "MOD_TRIGGER_HURT",
    "MOD_TEAM_CHANGE",
];

/// `weaponFromMOD`: the weapon a means of death is credited to. Means past
/// `MOD_SENTRY` (the world's) count for no weapon, not even "No Weapon".
const WEAPON_OF_MEANS: [usize; 33] = [
    0, 1, 2, 3, 4, 4, 5, 18, 6, 6, 6, 7, 8, 8, 8, 9, 9, 10, 10, 12, 12, 12, 12, 13, 13, 14, 14, 15,
    0, 11, 11, 0, 0,
];

/// The longest time one shot adds to its weapon's (`G_LogWeaponFire`), in milliseconds.
const MAX_SHOT_TIME: i32 = 5_000;

/// One player's tallies.
#[derive(Clone, Debug)]
struct Record {
    pickups: [i32; WEAPON_COUNT],
    shots: [i32; WEAPON_COUNT],
    time: [i32; WEAPON_COUNT],
    deaths: [i32; WEAPON_COUNT],
    damage: [i32; MEANS_COUNT],
    kills: [i32; MEANS_COUNT],
    last_shot: i32,
}

impl Default for Record {
    fn default() -> Self {
        Self {
            pickups: [0; WEAPON_COUNT],
            shots: [0; WEAPON_COUNT],
            time: [0; WEAPON_COUNT],
            deaths: [0; WEAPON_COUNT],
            damage: [0; MEANS_COUNT],
            kills: [0; MEANS_COUNT],
            last_shot: 0,
        }
    }
}

impl Record {
    /// Means-of-death tallies summed by the weapon each is credited to.
    fn by_weapon(tally: &[i32; MEANS_COUNT]) -> [i32; WEAPON_COUNT] {
        let mut weapons = [0; WEAPON_COUNT];
        for (means, weapon) in WEAPON_OF_MEANS.iter().enumerate() {
            weapons[*weapon] += tally[means];
        }
        weapons
    }
}

/// The level's tallies (`G_WeaponLog*`), kept for every player some event touched since
/// it last entered the game.
#[derive(Clone, Debug, Default)]
pub struct StatLog {
    players: BTreeMap<usize, Record>,
}

impl StatLog {
    fn record(&mut self, client: usize) -> &mut Record {
        self.players.entry(client).or_default()
    }

    /// `G_LogWeaponInit`: a new level counts from nothing.
    pub fn reset(&mut self) {
        self.players.clear();
    }

    /// `G_ClearClientLog`: a player entering the game (`ClientBegin`, a team change
    /// included) or leaving it starts again.
    pub fn clear(&mut self, client: usize) {
        self.players.remove(&client);
    }

    /// `G_LogWeaponPowerup` and `G_LogWeaponItem`: a powerup or holdable picked up puts the
    /// player in the tables, though no table counts it.
    pub fn touch(&mut self, client: usize) {
        self.record(client);
    }

    /// `G_LogWeaponPickup` (`Pickup_Weapon`).
    pub fn pickup(&mut self, client: usize, weapon: u32) {
        let record = self.record(client);
        if let Some(count) = record.pickups.get_mut(weapon as usize) {
            *count += 1;
        }
    }

    /// `G_LogWeaponFire` (`FireWeapon`): a shot, and the time since the player's last one
    /// (at most five seconds) spent with this weapon.
    pub fn fire(&mut self, client: usize, weapon: u32, level_time: i32) {
        let record = self.record(client);
        let since = level_time - record.last_shot;
        record.last_shot = level_time;
        let weapon = weapon as usize;
        if weapon < WEAPON_COUNT {
            record.shots[weapon] += 1;
            record.time[weapon] += since.min(MAX_SHOT_TIME);
        }
    }

    /// `G_LogWeaponDamage` (`G_Damage`): the health a blow by the player took, whatever
    /// it struck.
    pub fn damage(&mut self, client: usize, means: u32, take: i32) {
        let record = self.record(client);
        if let Some(total) = record.damage.get_mut(means as usize) {
            *total += take;
        }
    }

    /// `G_LogWeaponKill` (`player_die`): a kill by the player, itself or an NPC included.
    pub fn kill(&mut self, client: usize, means: u32) {
        let record = self.record(client);
        if let Some(count) = record.kills.get_mut(means as usize) {
            *count += 1;
        }
    }

    /// `G_LogWeaponDeath` (`player_die`): the player died holding `weapon`.
    pub fn death(&mut self, client: usize, weapon: u32) {
        let record = self.record(client);
        if let Some(count) = record.deaths.get_mut(weapon as usize) {
            *count += 1;
        }
    }

    /// The game's totals of one tally.
    fn total<const N: usize>(&self, tally: impl Fn(&Record) -> [i32; N]) -> [i32; N] {
        let mut sum = [0; N];
        for record in self.players.values() {
            for (total, value) in sum.iter_mut().zip(tally(record)) {
                *total += value;
            }
        }
        sum
    }

    /// `G_LogWeaponOutput`'s game-log half: one `G_LogPrintf` text each.
    pub fn summary(&self) -> Vec<String> {
        let pickups = self.total(|record| record.pickups);
        let time = self.total(|record| record.time);
        let deaths = self.total(|record| record.deaths);
        let shots = self.total(|record| record.shots);
        let damage_by_means = self.total(|record| record.damage);
        let kills_by_means = self.total(|record| record.kills);
        let damage = Record::by_weapon(&damage_by_means);
        let kills = Record::by_weapon(&kills_by_means);
        let mut lines = vec![
            "*****************************Weapon Log:\n".to_owned(),
            "\n****Data by Weapon:\n".to_owned(),
        ];
        for (weapon, name) in WEAPON_NAMES.iter().enumerate() {
            lines.push(format!(
                "{name:>15}:  Pickups: {:4},  Time:  {:5},  Deaths: {:5}\n",
                pickups[weapon],
                time[weapon] / 1_000,
                deaths[weapon]
            ));
        }
        lines.push("\n****Combat Data by Weapon:\n".to_owned());
        for (weapon, name) in WEAPON_NAMES.iter().enumerate() {
            let per_shot = if shots[weapon] > 0 {
                damage[weapon] as f32 / shots[weapon] as f32
            } else {
                0.0
            };
            lines.push(format!(
                "{name:>15}:  Damage: {:6},  Kills: {:5},  Dmg per Shot: {:.6}\n",
                damage[weapon],
                kills[weapon],
                f64::from(per_shot)
            ));
        }
        lines.push("\n****Combat Data By Damage Type:\n".to_owned());
        for (means, name) in MEANS_NAMES.iter().enumerate() {
            lines.push(format!(
                "{name:>25}:  Damage: {:6},  Kills: {:5}\n",
                damage_by_means[means], kills_by_means[means]
            ));
        }
        lines.push("\n".to_owned());
        lines
    }

    /// `G_LogWeaponOutput`'s file half: the level's name, then eight tables of a column
    /// per weapon or means, a row per player (`name` gives its name) and the totals.
    pub fn tables(&self, map: &[u8], name: impl Fn(usize) -> Vec<u8>) -> Vec<u8> {
        let mut text = b"\n\n\nLevel:\t".to_vec();
        text.extend_from_slice(map);
        text.extend_from_slice(b"\n\n\n");
        self.table(
            &mut text,
            "Weapon Pickups per Player",
            &WEAPON_NAMES,
            &name,
            |r| r.pickups.to_vec(),
        );
        self.table(
            &mut text,
            "Weapon Shots per Player",
            &WEAPON_NAMES,
            &name,
            |r| r.shots.to_vec(),
        );
        self.table(
            &mut text,
            "Weapon Use Time per Player",
            &WEAPON_NAMES,
            &name,
            |r| r.time.to_vec(),
        );
        self.table(
            &mut text,
            "Weapon Deaths per Player",
            &WEAPON_NAMES,
            &name,
            |r| r.deaths.to_vec(),
        );
        self.table(
            &mut text,
            "Weapon Damage per Player",
            &WEAPON_NAMES,
            &name,
            |r| Record::by_weapon(&r.damage).to_vec(),
        );
        self.table(
            &mut text,
            "Weapon Kills per Player",
            &WEAPON_NAMES,
            &name,
            |r| Record::by_weapon(&r.kills).to_vec(),
        );
        self.table(
            &mut text,
            "Typed Damage per Player",
            &MEANS_NAMES,
            &name,
            |r| r.damage.to_vec(),
        );
        self.table(
            &mut text,
            "Damage-Typed Kills per Player",
            &MEANS_NAMES,
            &name,
            |r| r.kills.to_vec(),
        );
        text
    }

    /// One table of [`Self::tables`].
    fn table(
        &self,
        text: &mut Vec<u8>,
        title: &str,
        columns: &[&str],
        name: &impl Fn(usize) -> Vec<u8>,
        row: impl Fn(&Record) -> Vec<i32>,
    ) {
        text.extend_from_slice(format!("{title}:\n\nPlayer").as_bytes());
        for column in columns {
            text.extend_from_slice(format!("\t{column}").as_bytes());
        }
        text.push(b'\n');
        let mut totals = vec![0; columns.len()];
        for (client, record) in &self.players {
            text.extend_from_slice(&name(*client));
            for (total, value) in totals.iter_mut().zip(row(record)) {
                text.extend_from_slice(format!("\t{value}").as_bytes());
                *total += value;
            }
            text.push(b'\n');
        }
        text.extend_from_slice(b"\n***TOTAL:");
        for total in totals {
            text.extend_from_slice(format!("\t{total}").as_bytes());
        }
        text.extend_from_slice(b"\n\n\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `G_LogWeaponFire`: the first shot of a level counts five seconds; later shots the
    /// time since the last, at most five seconds.
    #[test]
    fn a_shot_adds_the_time_since_the_last() {
        let mut log = StatLog::default();
        log.fire(0, 4, 12_000);
        log.fire(0, 4, 13_500);
        log.fire(0, 5, 30_000);
        let record = &log.players[&0];
        assert_eq!(record.shots[4], 2);
        assert_eq!(record.time[4], 6_500);
        assert_eq!(record.time[5], 5_000);
    }

    /// `G_LogWeaponOutput`: damage by means is credited to its weapon — the concussion
    /// rifle's to the rifle, the world's to none — and divided by the weapon's shots.
    #[test]
    fn the_summary_credits_damage_to_weapons() {
        let mut log = StatLog::default();
        log.fire(1, 11, 1_000);
        log.fire(1, 11, 1_100);
        log.damage(1, 29, 40);
        log.damage(1, 30, 35);
        log.damage(1, 38, 10);
        log.kill(1, 29);
        let lines = log.summary();
        assert_eq!(
            lines[2 + WEAPON_COUNT + 1 + 11],
            "Concussion Rifle:  Damage:     75,  Kills:     1,  Dmg per Shot: 37.500000\n"
        );
        assert_eq!(
            lines[2 + WEAPON_COUNT + 1],
            "      No Weapon:  Damage:      0,  Kills:     0,  Dmg per Shot: 0.000000\n"
        );
        assert_eq!(
            lines[2 + 2 * WEAPON_COUNT + 2 + 38],
            "              MOD_FALLING:  Damage:     10,  Kills:     0\n"
        );
    }

    /// The file: a row per player touched since it entered the game, in slot order.
    #[test]
    fn the_tables_list_each_touched_player() {
        let mut log = StatLog::default();
        log.pickup(3, 5);
        log.touch(1);
        log.pickup(2, 5);
        log.clear(2);
        let text = log.tables(b"mp/ffa3", |client| format!("p{client}").into_bytes());
        let text = String::from_utf8(text).unwrap();
        assert!(text.starts_with("\n\n\nLevel:\tmp/ffa3\n\n\nWeapon Pickups per Player:\n\nPlayer\tNo Weapon\tStun Baton\tMelee\t"));
        assert!(text.contains("\np1\t0\t0\t0\t0\t0\t0\t"));
        assert!(text.contains("\np3\t0\t0\t0\t0\t0\t1\t"));
        assert!(!text.contains("\np2\t"));
        assert!(text.contains("\n***TOTAL:\t0\t0\t0\t0\t0\t1\t0"));
        let last = format!("\n***TOTAL:{}\n\n\n", "\t0".repeat(MEANS_COUNT));
        assert!(text.ends_with(&last));
    }
}
