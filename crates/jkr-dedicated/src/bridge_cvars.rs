//! The server's console variables at work: what a changed value does, checked after
//! every console line and every frame, as `G_UpdateCvars` and `SV_Frame`'s serverinfo
//! and systeminfo checks do it.

use super::{NativeGame, Told};
use crate::cvars::{CVAR_INTERNAL, CVAR_SERVERINFO, CVAR_SYSTEMINFO, Cvars};
use jkr_network::{LegacyOobRates, LegacySessionSettings};

/// `CS_SYSTEMINFO`: the configstring of what every client keeps a copy of.
const CS_SYSTEMINFO: usize = 1;

impl NativeGame {
    /// The console variables, for the process to read what belongs to the endpoint
    /// (passwords, timeouts, rates, `sv_fps`).
    pub fn cvars(&self) -> &Cvars {
        &self.cvars
    }

    /// `duel_fraglimit`: the duels a player must win to end a tournament, 0 for none.
    pub fn set_duel_fraglimit(&mut self, duel_fraglimit: i32) {
        self.set_cvar("duel_fraglimit", &duel_fraglimit.to_string());
    }

    /// The limits that end a match — `fraglimit`, `timelimit`, `capturelimit` — set as
    /// the console variables they are.
    pub fn set_limits(&mut self, fraglimit: i32, timelimit: f32, capturelimit: i32) {
        self.set_cvar("fraglimit", &fraglimit.to_string());
        self.set_cvar("timelimit", &timelimit.to_string());
        self.set_cvar("capturelimit", &capturelimit.to_string());
    }

    /// `Cvar_Set`: the process's or the code's own change, past every protection, put
    /// into effect at once. How the command line's options reach the server.
    pub fn set_cvar(&mut self, name: &str, value: &str) {
        self.cvars.set(name.as_bytes(), value.as_bytes());
        self.apply_cvars();
    }

    /// Put every variable changed since the last look into effect: the game's own
    /// settings, the server-info string everyone is sent, and the announcement of a
    /// tracked variable (`print "Server: %s changed to %s\n"`).
    pub(super) fn apply_cvars(&mut self) {
        // G_CheckNeedPass: '%' cannot survive the legacy print/connect path.
        // Derive the read-only public flag from the actual admission password.
        if self.cvars.string(b"g_password").contains(&b'%') {
            let password: Vec<u8> = self
                .cvars
                .string(b"g_password")
                .iter()
                .map(|&byte| if byte == b'%' { b'.' } else { byte })
                .collect();
            self.cvars.set(b"g_password", &password);
        }
        let password = self.cvars.string(b"g_password");
        let required = !password.is_empty() && !password.eq_ignore_ascii_case(b"none");
        if self.cvars.integer(b"g_needpass") != i32::from(required) {
            self.cvars
                .set(b"g_needpass", if required { b"1" } else { b"0" });
        }
        let mut seen = std::mem::take(&mut self.cvars_seen);
        let changed = self.cvars.changed_since(&mut seen);
        self.cvars_seen = seen;
        if !changed.is_empty() {
            self.cvars_generation += 1;
        }
        let mut system_info_changed = false;
        for name in changed {
            let Some(var) = self.cvars.var(&name) else {
                continue;
            };
            let (value, flags, announced, integer) =
                (var.string.clone(), var.flags, var.announced, var.integer());
            system_info_changed |= flags & CVAR_SYSTEMINFO != 0;
            let text = String::from_utf8_lossy(&value).into_owned();
            let key = String::from_utf8_lossy(&name).into_owned();
            match key.to_ascii_lowercase().as_str() {
                "sv_hostname" => self.identity.hostname = value.clone(),
                // The process's configuration runs once the level is up: a message of the
                // day it sets reaches this level's loading screens too.
                "g_motd" => self.set_config_string(jkr_game_jka::worldspawn::CS_MOTD, &value),
                "fraglimit" => self.limits.fraglimit = integer,
                "timelimit" => self.limits.timelimit = var.value(),
                "capturelimit" => self.limits.capturelimit = integer,
                "duel_fraglimit" => self.limits.duel_fraglimit = integer,
                "d_nointermissionwait" => self.limits.no_intermission_wait = integer != 0,
                "g_allowvote" => self.vote_rules.0 = integer & super::bridge_votes::SUPPORTED_VOTES,
                "g_votedelay" => self.vote_rules.1 = integer,
                "g_tweakvote" => self.japro_votes.tweak = integer,
                "g_votetimeout" => self.japro_votes.timeout = integer,
                "g_teamautojoin" => self.settings.team_auto_join = integer != 0,
                "g_maxforcerank"
                | "g_forcepowerdisable"
                | "g_forcebasedteams"
                | "g_weapondisable"
                | "g_duelweapondisable" => self.refresh_rule_settings(),
                "sv_cheats" => self.settings.cheats = integer != 0,
                "g_flagdrag" => self.flag_drag = var.value(),
                "g_forceregentime" => self.force_rules.regen_time = integer,
                "g_usewhilethrowing" => self.force_rules.use_while_throwing = integer != 0,
                "g_saberdamagescale" => self.saber_settings.damage_scale = var.value(),
                "g_saberdmgdelay_idle" => self.saber_settings.idle_delay = integer,
                "g_saberdmgdelay_wound" => self.saber_settings.wound_delay = integer,
                "g_saberwalldamagescale" => self.saber_settings.wall_scale = var.value(),
                "g_saberlocking" => self.saber_settings.locking = integer != 0,
                "g_saberlockfactor" => self.saber_settings.lock_factor = integer,
                "g_saberlockrandomness" => self.saber_settings.lock_randomness = integer,
                "g_friendlysaber" => self.saber_settings.friendly = integer != 0,
                "g_fixsaberdisarmbonus" => self.saber_settings.disarm_bonus_fix = integer != 0,
                "g_smoothclients" => self.smooth_clients = integer != 0,
                "pmove_fixed" | "pmove_msec" | "pmove_float" | "g_stepslidefix"
                | "g_nospecmove" => self.refresh_move_cvars(),
                "g_synchronousclients" => self.synchronous_clients = integer != 0,
                "timescale" => self.timescale = var.value(),
                "g_fixsabermovedata" | "g_fixweaponattackanim" | "g_fixrunwalkanims" => {
                    self.publish_legacy_fixes()
                }
                "g_knockback" => self.damage_rules.strength = var.value(),
                "g_saberdmgvelocityscale" => {
                    self.damage_rules.saber_scale = var.value();
                    self.damage_rules.saber_push_holds = integer != 0;
                }
                "g_locationbaseddamage" => self.damage_rules.location_based = integer != 0,
                "g_armbreakage" => self.damage_rules.arm_breakage = integer != 0,
                "g_debugdamage" => self.damage_rules.debug_print = integer != 0,
                "g_debugmelee" => {
                    self.debug_melee = integer != 0;
                    self.force_rules.debug_melee = integer != 0;
                }
                "g_fixflaghitbox" => self.wide_flag_box = integer != 0,
                "g_startingitems" => self.starting_items_changed(),
                "g_selfdamagescale" => self.self_damage_scale = var.value(),
                "g_mercyrule" => self.limits.mercy = var.value(),
                "g_showhealth" => self.show_health_changed(integer != 0),
                "g_corpseremovaltime" => self.body_sink_ms = integer.saturating_mul(1_000),
                "g_lagicon" => self.lag_icon = integer != 0,
                "g_godchat" => self.god_chat = integer != 0,
                "g_autoquit" => self.auto_quit_days = integer,
                "g_gungame" => self.gun_game = integer,
                "g_friendlyfire" => self.force_rules.friendly_fire = integer != 0,
                "g_teamhealscale" => self.force_rules.team_heal_scale = var.value(),
                "g_teamenergizescale" => self.force_rules.team_energize_scale = var.value(),
                "g_teamabsorbscale" => self.force_rules.team_absorb_scale = var.value(),
                "g_forcepowerdisableffa" => {
                    self.force_rules.disabled_outside_duels = integer as u32
                }
                "g_gametype" if integer != self.gametype => self.set_gametype(integer),
                _ => {}
            }
            // `SV_SetConfigstring(CS_SERVERINFO, Cvar_InfoString(CVAR_SERVERINFO))`, one
            // key at a time into the string this server keeps in its own order; the
            // registration's own values are the level's first string, told to nobody.
            if flags & CVAR_SERVERINFO != 0 && flags & CVAR_INTERNAL == 0 {
                self.set_server_info(&key, &text, self.cvars_primed);
            }
            // The first look is the registration's own, which announces nothing.
            if announced && self.cvars_primed {
                self.told.push(Told::Everyone(
                    format!("print \"Server: {key} changed to {text}\n\"").into_bytes(),
                ));
            }
        }
        // `SV_Frame`: `SV_SetConfigstring(CS_SYSTEMINFO, Cvar_InfoString_Big(CVAR_SYSTEMINFO))`
        // once anything every client keeps a copy of has changed.
        if system_info_changed {
            let info = self.cvars.info_string_big(CVAR_SYSTEMINFO);
            self.set_config_string(CS_SYSTEMINFO, &info);
        }
        self.cvars_primed = true;
    }

    /// The movement variables every move reads ([`jkr_game_jka::pmove_rules::MoveCvars`]),
    /// `pmove_msec` put back within 8 to 33 as `ClientThink_real` puts it
    /// (`g_active.c:2031-2036`).
    fn refresh_move_cvars(&mut self) {
        let msec = self.cvars.integer(b"pmove_msec");
        if msec < 8 {
            self.cvars.set(b"pmove_msec", b"8");
        } else if msec > 33 {
            self.cvars.set(b"pmove_msec", b"33");
        }
        let cvars = &self.cvars;
        self.move_cvars = jkr_game_jka::pmove_rules::MoveCvars::new(
            cvars.integer(b"pmove_fixed"),
            cvars.integer(b"pmove_msec"),
            cvars.integer(b"pmove_float"),
            cvars.integer(b"g_stepSlideFix"),
            cvars.integer(b"g_noSpecMove"),
        );
    }

    /// The Force and weapon rules the game reads from its cvars (`g_xcvar.h`, all latched):
    /// the rank ceiling, the disabled powers, the sides by team, the disabled weapons.
    pub(super) fn refresh_rule_settings(&mut self) {
        self.settings.max_rank = self.integer_cvar(b"g_maxForceRank", 7);
        self.settings.disabled = self.integer_cvar(b"g_forcePowerDisable", 0) as u32;
        self.settings.force_based_teams = self.integer_cvar(b"g_forceBasedTeams", 0) != 0;
        self.refresh_weapons_disabled();
    }

    /// The disabled weapons this game type reads: `g_duelWeaponDisable` in the duels,
    /// else `g_weaponDisable` (`HasSetSaberOnly`, `G_SpawnItem`, `ClientSpawn`).
    pub(super) fn refresh_weapons_disabled(&mut self) {
        let duel =
            jkr_game_jka::force_config::ForceServerSettings::uses_duel_weapons(self.gametype);
        let (name, default): (&[u8], i32) = if duel {
            (b"g_duelWeaponDisable", 1)
        } else {
            (b"g_weaponDisable", 0)
        };
        self.settings.weapons_disabled = self.integer_cvar(name, default) as u32;
    }

    /// `Cvar_Get` of a latched variable when a level is built (`SV_Map_f`'s
    /// `g_gametype`): a change waiting for the next map takes effect.
    pub(super) fn take_latched_cvars(&mut self) {
        self.cvars.get(
            b"g_gametype",
            b"0",
            CVAR_SERVERINFO | crate::cvars::CVAR_LATCH,
            None,
        );
        self.apply_cvars();
    }

    /// `timescale`: how many times as fast as the wall clock the server's time runs, which
    /// the process's frame clock applies.
    pub fn timescale(&self) -> f32 {
        self.timescale
    }

    /// How many times the variables have changed; the process compares it to know when
    /// to read [`Self::endpoint_settings`] again.
    pub fn cvars_generation(&self) -> u64 {
        self.cvars_generation
    }

    /// The endpoint's share of the variables: the remote console's password, the
    /// private slots, the timeouts, flood protection, downloads, automatic demos and the
    /// rate and snapshot limits.
    pub fn endpoint_settings(&self, settings: &mut LegacySessionSettings) {
        let cvars = &self.cvars;
        settings.console.rcon_password = cvars.string(b"rconPassword").to_vec();
        // `status` says whether this is a LAN or a public server.
        settings.console.dedicated = cvars.integer(b"dedicated");
        settings.private_clients = cvars.integer(b"sv_privateClients");
        settings.private_password = cvars.string(b"sv_privatePassword").to_vec();
        settings.timeout_seconds = cvars.integer(b"sv_timeout");
        settings.zombie_seconds = cvars.integer(b"sv_zombietime");
        settings.reconnect_limit_seconds = cvars.integer(b"sv_reconnectlimit");
        settings.flood_protect = cvars.integer(b"sv_floodProtect");
        settings.legacy_fixes = cvars.integer(b"sv_legacyFixes") != 0;
        settings.allow_download = cvars.integer(b"sv_allowDownload") != 0;
        settings.pure = cvars.integer(b"sv_pure") != 0;
        settings.auto_whitelist = cvars.integer(b"sv_autoWhitelist") != 0;
        settings.ping_fix = cvars.integer(b"sv_pingFix") != 0;
        settings.anti_dst = cvars.integer(b"sv_antiDST") != 0;
        // TaystJK reads `sv_legacyFixes` as a mask whose bit 7 allows the broken models.
        let legacy_fixes = cvars.integer(b"sv_legacyFixes");
        settings.broken_models_fix = legacy_fixes != 0 && legacy_fixes & (1 << 7) == 0;
        settings.auto_demo = jkr_network::LegacyAutoDemoSettings {
            enabled: cvars.integer(b"sv_autoDemo") != 0,
            max_maps: cvars.integer(b"sv_autoDemoMaxMaps"),
        };
        settings.oob_rates = LegacyOobRates {
            per_address: cvars.integer(b"sv_maxOOBRateIP"),
            global: cvars.integer(b"sv_maxOOBRate"),
        };
        let rates = &mut settings.rates;
        rates.lan_force_rate = cvars.integer(b"sv_lanForceRate") != 0;
        rates.rate_policy = cvars.integer(b"sv_ratePolicy");
        rates.client_rate = cvars.integer(b"sv_clientRate");
        rates.min_rate = cvars.integer(b"sv_minRate");
        rates.max_rate = cvars.integer(b"sv_maxRate");
        rates.snaps_policy = cvars.integer(b"sv_snapsPolicy");
        rates.snaps_min = cvars.integer(b"sv_snapsMin");
        rates.snaps_max = cvars.integer(b"sv_snapsMax");
        // Simulation/snapshot time now advances in complete frames, so LAN peers
        // can use the actual server cadence without skipping an early wall-clock frame.
        rates.fps = cvars.integer(b"sv_fps").max(1);
    }

    /// `SV_Frame`'s frame length: a thousand milliseconds over `sv_fps`, never below
    /// one; an `sv_fps` below 1 is set back to 10 first, as the reference repairs it.
    pub fn frame_msec(&mut self) -> i32 {
        if let Some(msec) = self.hibernation_frame_msec() {
            return msec;
        }
        if self.cvars.integer(b"sv_fps") < 1 {
            self.set_cvar("sv_fps", "10");
        }
        (1000 / self.cvars.integer(b"sv_fps")).max(1)
    }

    /// One `CVAR_SERVERINFO` value rewritten in `CS_SERVERINFO` and the status reply;
    /// `tell` when clients already hold the old one.
    pub(super) fn set_server_info(&mut self, key: &str, value: &str, tell: bool) {
        self.status_info = server_info_with(&self.status_info, key, value);
        let info = self.status_info.clone();
        if tell {
            self.publish_config_string(0, &info);
            return;
        }
        if let Some(slot) = self
            .config_strings
            .iter_mut()
            .find(|(index, _)| *index == 0)
        {
            slot.1.clone_from(&info);
        }
        // A change told but not yet sent (the game type's, at startup) carries the string
        // as it stood then; what is sent, and kept, is the string as it stands now.
        for told in &mut self.told {
            if let Told::ConfigString {
                index: 0, value, ..
            } = told
            {
                value.clone_from(&info);
            }
        }
    }

    /// A console line for the cvar table: a cvar command or a variable by name.
    pub(super) fn cvar_line(&mut self, line: &[u8], print: &mut dyn FnMut(&[u8])) -> bool {
        let taken = self.cvars.command(line, print);
        if taken {
            self.apply_cvars();
        }
        taken
    }
}

/// `CS_SERVERINFO` with one key's value replaced — `mapname` on a map change, a limit
/// when a vote sets one — and the rest of that string, the engine's and the game's own,
/// left alone. A key the string does not hold yet is added at its end, if the string
/// stays within `MAX_INFO_STRING`.
pub(super) fn server_info_with(server_info: &[u8], key: &str, value: &str) -> Vec<u8> {
    let fields: Vec<&[u8]> = server_info.split(|byte| *byte == b'\\').collect();
    let mut rebuilt: Vec<u8> = Vec::with_capacity(server_info.len() + value.len());
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            rebuilt.push(b'\\');
        }
        if index >= 2 && index % 2 == 0 && fields[index - 1] == key.as_bytes() {
            rebuilt.extend_from_slice(value.as_bytes());
        } else {
            rebuilt.extend_from_slice(field);
        }
    }
    // `Info_SetValueForKey` refuses a pair that would reach `MAX_INFO_STRING`.
    let held = fields
        .iter()
        .skip(1)
        .step_by(2)
        .any(|known| *known == key.as_bytes());
    if !held && !value.is_empty() && rebuilt.len() + key.len() + value.len() + 2 < 1024 {
        rebuilt.extend_from_slice(format!("\\{key}\\{value}").as_bytes());
    }
    rebuilt
}

/// The `CVAR_SERVERINFO` variables a legacy cgame reads from `CS_SERVERINFO`, in the
/// order this server has always sent them; their values are the variables' own, filled
/// in when the table is first read.
pub(super) const SERVERINFO_DEFAULTS: [(&str, &str); 20] = [
    ("g_gametype", "0"),
    ("sv_fps", "20"),
    ("timelimit", "0"),
    ("fraglimit", "20"),
    ("capturelimit", "8"),
    ("duel_fraglimit", "10"),
    ("dmflags", "0"),
    ("g_needpass", "0"),
    ("g_jediVmerc", "0"),
    ("g_forcePowerDisable", "0"),
    ("g_weaponDisable", "0"),
    ("g_stepSlideFix", "1"),
    ("g_noSpecMove", "0"),
    ("g_debugMelee", "0"),
    ("g_showDuelHealths", "0"),
    ("g_siegeTeam1", "none"),
    ("g_siegeTeam2", "none"),
    ("g_siegeTeamSwitch", "1"),
    ("g_redTeam", "Empire"),
    ("g_blueTeam", "Rebellion"),
];

#[cfg(test)]
mod tests {
    use super::super::{Identity, NativeGame};
    use jkr_network::{LegacyGameHost, LegacyGameOutput};

    /// An `sv_fps` nobody set runs twenty frames a second; one set to anything, stock's
    /// own default of 40 included, keeps that value.
    #[test]
    fn sv_fps_keeps_whatever_was_set() {
        for (set, fps) in [(None, 20), (Some(&b"40"[..]), 40), (Some(&b"30"[..]), 30)] {
            let mut cvars = crate::cvars::Cvars::server();
            if let Some(value) = set {
                cvars.user_set(b"sv_fps", Some(value), &mut |_| {});
            }
            let identity = Identity {
                hostname: b"test".to_vec(),
                mapname: b"mp/ffa3".to_vec(),
            };
            let game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
            assert_eq!(game.cvars.integer(b"sv_fps"), fps, "{set:?}");
        }
    }

    /// Startup values reach the gamestate and the clients, though the game type's change
    /// was told before the other keys were filled in.
    #[test]
    fn startup_server_info_reaches_the_gamestate() {
        let mut cvars = crate::cvars::Cvars::server();
        cvars.set(b"g_showDuelHealths", b"1");
        cvars.set(b"g_gametype", b"3");
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/duel5".to_vec(),
        };
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        let mut sent = Vec::new();
        game.take_output(&mut |output| {
            if let LegacyGameOutput::ConfigString {
                index: 0, value, ..
            } = output
            {
                sent.push(value.to_vec());
            }
        });
        let kept = game
            .config_strings()
            .find(|(index, _)| *index == 0)
            .map(|(_, info)| info.to_vec())
            .unwrap();
        for info in sent.iter().chain([&kept]) {
            let info = String::from_utf8_lossy(info);
            assert!(info.contains("\\g_showDuelHealths\\1"), "{info}");
            assert!(info.contains("\\g_gametype\\3"), "{info}");
        }
    }

    /// The systeminfo is the table's `CVAR_SYSTEMINFO` variables, empty ones included, and
    /// a change to one of them reaches every client as `SV_Frame` sends it.
    #[test]
    fn system_info_follows_its_variables() {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        let system_info = |game: &NativeGame| {
            let info = game
                .config_strings()
                .find(|(index, _)| *index == 1)
                .unwrap()
                .1;
            String::from_utf8_lossy(info).into_owned()
        };
        let info = system_info(&game);
        for pair in [
            "\\sv_cheats\\",
            "\\fs_game\\\\",
            "\\pmove_fixed\\0",
            "\\timescale\\1",
        ] {
            assert!(info.contains(pair), "{pair} in {info}");
        }
        game.take_output(&mut |_| {});
        game.set_cvar("pmove_fixed", "1");
        let mut sent = Vec::new();
        game.take_output(&mut |output| {
            if let LegacyGameOutput::ConfigString {
                index: 1, value, ..
            } = output
            {
                sent.push(String::from_utf8_lossy(value).into_owned());
            }
        });
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(sent[0].contains("\\pmove_fixed\\1"), "{sent:?}");
        assert_eq!(system_info(&game), sent[0]);
    }

    /// `sv_cheats` is the operator's (the reference keeps it read-only, for `map` and
    /// `devmap` alone): a console or rcon line turns the cheat commands on and off at
    /// once and every client is told through the systeminfo, and the world commands
    /// still set it as the reference's do.
    #[test]
    fn operators_turn_the_cheats_on_and_off() {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        assert!(!game.settings.cheats);
        for (line, cheats) in [
            (&b"sv_cheats 1"[..], true),
            (b"sv_cheats 0", false),
            (b"devmap mp/ffa3", true),
            (b"map mp/ffa3", false),
        ] {
            let shown = String::from_utf8_lossy(line).into_owned();
            game.take_output(&mut |_| {});
            let mut printed = Vec::new();
            let known = game.console_line(
                line,
                0,
                &mut super::super::bridge_bots::NoBotSlots,
                &mut |text| printed.extend_from_slice(text),
            );
            let printed = String::from_utf8_lossy(&printed).into_owned();
            assert!(known && !printed.contains("read only"), "{shown}: {printed}");
            assert_eq!(game.settings.cheats, cheats, "{shown}");
            let pair = format!("\\sv_cheats\\{}", u8::from(cheats));
            let kept = game
                .config_strings()
                .find(|(index, _)| *index == 1)
                .map(|(_, info)| String::from_utf8_lossy(info).into_owned())
                .unwrap();
            assert!(kept.contains(&pair), "{shown}: {kept}");
            if line.starts_with(b"sv_cheats") {
                let mut told = false;
                game.take_output(&mut |output| {
                    if let LegacyGameOutput::ConfigString {
                        index: 1, value, ..
                    } = output
                    {
                        told |= String::from_utf8_lossy(value).contains(&pair);
                    }
                });
                assert!(told, "{shown}");
            }
        }
    }
}
