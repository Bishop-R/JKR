//! OpenJK's `forcetoggle` and `weapontoggle` (`sv_ccmds.cpp:1295-1456`): the Force powers
//! and weapons a server disables, listed by number (`[X]` for a disabled one) and flipped
//! one at a time in `g_forcePowerDisable`, `g_weaponDisable` or, in the duels,
//! `g_duelWeaponDisable`.

use super::NativeGame;

/// `forceToggleNamePrints`, by power.
const FORCE_NAMES: [&str; 18] = [
    "HEAL",
    "JUMP",
    "SPEED",
    "PUSH",
    "PULL",
    "MINDTRICK",
    "GRIP",
    "LIGHTNING",
    "DARK RAGE",
    "PROTECT",
    "ABSORB",
    "TEAM HEAL",
    "TEAM REPLENISH",
    "DRAIN",
    "SEEING",
    "SABER OFFENSE",
    "SABER DEFENSE",
    "SABER THROW",
];

/// `weaponToggleNamePrints`, by weapon.
const WEAPON_NAMES: [&str; 19] = [
    "NO WEAPON",
    "STUN BATON",
    "MELEE",
    "SABER",
    "BRYAR PISTOL",
    "BLASTER",
    "DISRUPTOR",
    "BOWCASTER",
    "REPEATER",
    "DEMP2",
    "FLECHETTE",
    "ROCKET LAUNCHER",
    "THERMAL",
    "TRIP MINE",
    "DET PACK",
    "CONCUSSION",
    "BRYAR OLD",
    "EMPLACED GUN",
    "TURRET",
];

/// One toggle command's table: its name, its cvar, its names and how its usage reads.
struct Toggle {
    command: &'static str,
    cvar: &'static str,
    names: &'static [&'static str],
    /// "power" or "weapon".
    kind: &'static str,
    /// The example's number and what it toggles.
    example: (u32, &'static str),
}

impl NativeGame {
    /// `SV_ForceToggle_f`.
    pub(super) fn force_toggle(&mut self, words: &[&[u8]], print: &mut dyn FnMut(&[u8])) {
        let toggle = Toggle {
            command: "forcetoggle",
            cvar: "g_forcePowerDisable",
            names: &FORCE_NAMES,
            kind: "power",
            example: (3, "PUSH"),
        };
        self.toggle(&toggle, words, print);
    }

    /// `SV_WeaponToggle_f`: `g_duelWeaponDisable` in Duel and Power Duel, else
    /// `g_weaponDisable`.
    pub(super) fn weapon_toggle(&mut self, words: &[&[u8]], print: &mut dyn FnMut(&[u8])) {
        let duel =
            jkr_game_jka::force_config::ForceServerSettings::uses_duel_weapons(self.gametype);
        let toggle = Toggle {
            command: "weapontoggle",
            cvar: if duel {
                "g_duelWeaponDisable"
            } else {
                "g_weaponDisable"
            },
            names: &WEAPON_NAMES,
            kind: "weapon",
            example: (3, "SABER"),
        };
        self.toggle(&toggle, words, print);
    }

    /// The list without one number given; the flip and what it left with one in range;
    /// the list and the usage for anything else. The reference prints "Enabled" for a
    /// bit it has just set, which disables the power; this tells what the flip did.
    fn toggle(&mut self, toggle: &Toggle, words: &[&[u8]], print: &mut dyn FnMut(&[u8])) {
        let bits = self.cvars.integer(toggle.cvar.as_bytes());
        let list = |print: &mut dyn FnMut(&[u8])| {
            for (number, name) in toggle.names.iter().enumerate() {
                let mark = if bits & (1 << number) != 0 { 'X' } else { ' ' };
                print(format!("{number:2} [{mark}] {name}\n").as_bytes());
            }
        };
        let (example, toggles) = toggle.example;
        let command = toggle.command;
        let [_, word] = words else {
            list(print);
            print(format!("Example usage: {command} {example}(toggles {toggles})\n").as_bytes());
            return;
        };
        let number = crate::cvars::is_a_number(word)
            .then(|| jkr_game_jka::userinfo::atoi(word))
            .filter(|&number| (0..toggle.names.len() as i32).contains(&number));
        let Some(number) = number else {
            list(print);
            print(
                format!(
                    "Specified a {} that does not exist.\nExample usage: {command} {example}\n(toggles {toggles})\n",
                    toggle.kind
                )
                .as_bytes(),
            );
            return;
        };
        let bits = bits ^ (1 << number);
        self.set_cvar(toggle.cvar, &bits.to_string());
        let state = if bits & (1 << number) != 0 {
            "^1Disabled"
        } else {
            "^2Enabled"
        };
        print(format!("{} {state}^7\n", toggle.names[number as usize]).as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::super::Identity;
    use super::*;

    /// A number in range flips its bit and tells what the flip left; anything else lists
    /// the powers with the usage. `2.5` is a number, as `Q_isanumber` reads it.
    #[test]
    fn a_toggle_flips_one_bit() {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        let mut run = |game: &mut NativeGame, words: &[&[u8]]| {
            let mut out = Vec::new();
            game.force_toggle(words, &mut |text| out.extend_from_slice(text));
            String::from_utf8(out).unwrap()
        };
        assert_eq!(
            run(&mut game, &[b"forcetoggle", b"3"]),
            "PUSH ^1Disabled^7\n"
        );
        assert_eq!(game.cvars.integer(b"g_forcePowerDisable"), 8);
        assert_eq!(
            run(&mut game, &[b"forcetoggle", b"2.5"]),
            "SPEED ^1Disabled^7\n"
        );
        assert_eq!(
            run(&mut game, &[b"forcetoggle", b"3"]),
            "PUSH ^2Enabled^7\n"
        );
        assert_eq!(game.cvars.integer(b"g_forcePowerDisable"), 4);
        let listed = run(&mut game, &[b"forcetoggle", b"18"]);
        assert!(listed.starts_with(" 0 [ ] HEAL\n 1 [ ] JUMP\n 2 [X] SPEED\n"));
        assert!(listed.ends_with(
            "Specified a power that does not exist.\nExample usage: forcetoggle 3\n(toggles PUSH)\n"
        ));
    }
}
