//! jaPRO's `serverConfig` (`Cmd_ServerConfig_f`, no gates): the server's settings that
//! differ from stock's, by section — in jaPRO's words and order, for the options this
//! server has (the saber, weapon, Force and movement tweaks it has not are left out);
//! and `aminfo` (`Cmd_Aminfo_f`): the jaPRO commands a player may use here.

use super::*;

/// `DF_NO_FALLING`; `GT_CTF`; `WP_CONCUSSION`; `NUM_FORCE_POWERS`.
const DF_NO_FALLING: i32 = 8;
const GT_CTF: i32 = 8;
const GT_TEAM: i32 = 6;
const WEAPONS_ALL_OFF: i32 = 1 << 15;
const FORCE_ALL_OFF: i32 = 1 << 18;

impl NativeGame {
    /// `serverconfig` from `client`: whether `command` was it.
    pub(super) fn server_config_command(&mut self, client: usize, command: &[u8]) -> bool {
        if !command.eq_ignore_ascii_case(b"serverconfig") {
            return false;
        }
        let sections = self.server_config();
        for section in sections {
            self.told.push(Told::One(
                client,
                format!("print \"{section}\"").into_bytes(),
            ));
        }
        true
    }

    /// `aminfo` from `client`: whether `command` was it. Its lines as jaPRO prints them,
    /// naming only the commands this server has (no accounts, clans, race or plugin), and
    /// the greeting saying what the server is.
    pub(super) fn aminfo_command(&mut self, client: usize, command: &[u8]) -> bool {
        if !command.eq_ignore_ascii_case(b"aminfo") {
            return false;
        }
        let Some(peer) = self.peer(client) else {
            return true;
        };
        let (name, powers) = (
            String::from_utf8_lossy(&peer.name).into_owned(),
            peer.session.account_flags,
        );
        let int = |name: &[u8]| self.cvars.integer(name);
        let mut game = String::from("   ^3Game commands: amMOTD ");
        if int(b"g_allowFlagThrow") != 0 && self.gametype == GT_CTF {
            game += "throwFlag ";
        }
        if self.gametype >= GT_TEAM && int(b"g_allowSpotting") != 0 {
            game += "spot ";
        }
        let disabled = int(b"g_emotesDisable") as u32;
        let mut emotes = String::from("   ^3Emote commands: ");
        for (family, help) in jkr_game_jka::japro_emotes::HELP_NAMES {
            if disabled & (1 << family) == 0 {
                emotes += help;
                emotes.push(' ');
            }
        }
        let mut admin = String::from("   ^3Admin commands: ");
        if powers == 0 {
            admin += "you are not an administrator on this server.\n";
        } else {
            use crate::admin::*;
            for (power, help) in [
                (A_ADMINTELE, "amTele "),
                (A_TELEMARK, "amTeleMark "),
                (A_FREEZE, "amFreeze "),
                (A_ADMINBAN, "amBan "),
                (A_ADMINKICK, "amKick "),
                (A_KILLVOTE, "amKillVote "),
                (A_NPC, "NPC "),
                (A_NOCLIP, "Noclip "),
                (A_GRANTADMIN, "amGrantAdmin "),
                (A_CHANGEMAP, "amMap "),
                (A_CSPRINT, "amPsay "),
                (A_FORCETEAM, "amForceTeam "),
                (A_VSTR, "amVstr "),
                (A_RENAME, "amRename "),
            ] {
                if powers & power != 0 {
                    admin += help;
                }
            }
        }
        let lines = [
            format!(
                "^5 Hi there, {name}^5. This server runs JKR with jaPRO's commands.\n   ^3To display server settings, type ^7serverConfig"
            ),
            String::from("   ^3Chat commands: ignore amSay"),
            game,
            emotes,
            admin,
        ];
        for line in lines {
            self.told.push(Told::One(
                client,
                format!("print \"{line}\n\"").into_bytes(),
            ));
        }
        true
    }

    /// The sections, each one print.
    fn server_config(&self) -> Vec<String> {
        let int = |name: &[u8]| self.cvars.integer(name);
        let value = |name: &[u8]| self.cvars.value(name);
        let yes = |on: bool| if on { "Yes" } else { "No" };
        let mut sections = Vec::new();
        let mut global = String::from(" ^3Global Changes:\n");
        global += &format!("   ^5Server tickrate^3: ^2{}\n", int(b"sv_fps"));
        global += &format!(
            "   ^5Force regen time (BaseJKA 'sv_fps 20' equivalent)^3: ^2{}\n",
            int(b"g_forceRegenTime") - 1000 / 20
        );
        global += &format!(
            "   ^5Location based damage^3: ^2{}\n",
            yes(int(b"g_locationBasedDamage") != 0)
        );
        if int(b"dmflags") & DF_NO_FALLING == 0 && int(b"g_maxFallDmg") != 0 {
            global += &format!("   ^5Fall damage capped at^3: ^2{}\n", int(b"g_maxFallDmg"));
        }
        if int(b"g_showHealth") != 0 {
            global += "   ^5Healthbars visible for all players\n";
        }
        sections.push(global);
        let mut saber = String::from(" ^3Saber Changes:\n");
        saber += &format!(
            "   ^5Saber style damage^3: ^2{}\n",
            if int(b"d_saberSPStyleDamage") != 0 {
                "SP"
            } else {
                "MP"
            }
        );
        if value(b"g_saberDamageScale") != 1.0 {
            saber += &format!(
                "   ^5Saber damage scale: ^2{:.2}\n",
                value(b"g_saberDamageScale")
            );
        }
        if int(b"d_saberGhoul2Collision") == 0 {
            saber += "   ^5Larger, square hitboxes for lightsabers\n";
        }
        if int(b"d_saberBoxTraceSize") != 0 {
            saber += &format!(
                "   ^5Increased saber hitbox size by: ^2{}\n",
                int(b"d_saberBoxTraceSize")
            );
        }
        if int(b"d_saberInterpolate") != 0 {
            saber += "   ^5Saber interpolate\n";
        }
        if int(b"g_duelStartArmor") != 0 {
            saber += &format!(
                "   ^5Duelers start with ^2{} ^5armor\n",
                int(b"g_duelStartArmor")
            );
        }
        let start_health = int(b"g_duelStartHealth");
        if start_health != 0 && start_health != 100 {
            saber += &format!("   ^5Duelers start with ^2{start_health} ^5health\n");
        }
        if int(b"g_saberDmgDelay_Idle") != 350 {
            saber += &format!(
                "   ^5Idle saber damage delay^3: ^2{}\n",
                int(b"g_saberDmgDelay_Idle")
            );
        }
        if int(b"g_saberDmgDelay_Wound") != 0 {
            saber += &format!(
                "   ^5Saber wound delay^3: ^2{}\n",
                int(b"g_saberDmgDelay_Wound")
            );
        }
        saber += &format!(
            "   ^5Saber kick tweak^3: ^2{}\n",
            yes(int(b"d_saberKickTweak") != 0)
        );
        sections.push(saber);
        if int(b"g_weaponDisable") < WEAPONS_ALL_OFF {
            let mut weapons = String::from(" ^3Weapon Changes:\n");
            match int(b"d_projectileGhoul2Collision") {
                0 => weapons += "   ^5Larger, square hitboxes for projectiles and hitscan\n",
                1 => {}
                _ => weapons += "   ^5Larger, square hitboxes for projectiles\n",
            }
            if value(b"g_selfDamageScale") != 0.5 {
                weapons += &format!(
                    "   ^5Self damage scale: ^2{:.2}\n",
                    value(b"g_selfDamageScale")
                );
            }
            sections.push(weapons);
        }
        if self.gametype == GT_CTF {
            let mut ctf = String::from(" ^3CTF Changes:\n");
            if value(b"g_flagDrag") != 0.0 {
                ctf += &format!("   ^5Flag Drag: ^2{:.3}\n", value(b"g_flagDrag"));
            }
            for (name, line) in [
                (&b"g_fixFlagSuicide"[..], "   ^5Flag suicide fix enabled\n"),
                (b"g_allowFlagThrow", "   ^5Flag throw enabled\n"),
                (b"g_fixFlagHitbox", "   ^5Increased flag hitbox size\n"),
                (b"g_fixCTFScores", "   ^5Tweaked CTF scores\n"),
            ] {
                if int(name) != 0 {
                    ctf += line;
                }
            }
            sections.push(ctf);
        }
        sections.push(String::from(" ^3Physics Changes:\n"));
        if int(b"g_forcePowerDisable") < FORCE_ALL_OFF {
            sections.push(String::from(" ^3Force Changes:\n"));
        }
        sections
    }
}
