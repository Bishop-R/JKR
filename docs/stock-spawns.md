# Stock map spawn routing inventory

Companion to the [stock multiplayer audit](stock-parity.md), inspected at JKR
`af65396` on 2026-10-05. All 185 entries from the selected
[PC reference spawn table](https://github.com/zachlatta/jedi-academy/blob/3fd984a7b82f29bc3f54410de248de69091c9699/codemp/game/g_spawn.c)
are listed below, retaining reference spellings. Comments were excluded when
extracting entries. The JKR entry point is
[spawn_table.rs](../crates/jkr-game-jka/src/spawn_table.rs), followed by the native
bridge's separate entity registries.

“Present route” records declared routing, not complete behavior. NPC aliases use
the shared NPC loader rather than 69 separately implemented spawn functions.
“Point/data route” and “Removal declared” must be checked against the actual
reference function when changing them. The principal unported entries were
checked against reference bodies; exceptions to misleading JKR labels are
recorded explicitly. This table is not a runtime map compatibility certificate.

Ordinary items are matched before this table, through
[items.rs](../crates/jkr-game-jka/src/items.rs); worldspawn initialization also has
its own path. Neither is included in the 185-entry count. Unknown third-party
classnames are outside this stock inventory. Conditional mover behavior is
described in the main audit, not represented by a percentage here.

| Classname | Reference spawn function | Audit classification | Note |
| --- | --- | --- | --- |
| `info_player_start` | `SP_info_player_start` | Present route | Behavior parity unverified. |
| `info_player_duel` | `SP_info_player_duel` | Present route | Behavior parity unverified. |
| `info_player_duel1` | `SP_info_player_duel1` | Present route | Behavior parity unverified. |
| `info_player_duel2` | `SP_info_player_duel2` | Present route | Behavior parity unverified. |
| `info_player_deathmatch` | `SP_info_player_deathmatch` | Present route | Behavior parity unverified. |
| `info_player_siegeteam1` | `SP_info_player_siegeteam1` | Present route | Behavior parity unverified. |
| `info_player_siegeteam2` | `SP_info_player_siegeteam2` | Present route | Behavior parity unverified. |
| `info_player_intermission` | `SP_info_player_intermission` | Present route | Behavior parity unverified. |
| `info_player_intermission_red` | `SP_info_player_intermission_red` | Present route | Behavior parity unverified. |
| `info_player_intermission_blue` | `SP_info_player_intermission_blue` | Present route | Behavior parity unverified. |
| `info_jedimaster_start` | `SP_info_jedimaster_start` | Present route | Behavior parity unverified. |
| `info_player_start_red` | `SP_info_player_start_red` | Present route | Behavior parity unverified. |
| `info_player_start_blue` | `SP_info_player_start_blue` | Present route | Behavior parity unverified. |
| `info_null` | `SP_info_null` | Removal declared | Compare reference conditions before changing. |
| `info_notnull` | `SP_info_notnull` | Point/data route | Check every consumer; no runtime model is required. |
| `info_camp` | `SP_info_camp` | Point/data route | Check every consumer; no runtime model is required. |
| `info_siege_objective` | `SP_info_siege_objective` | Present route | Behavior parity unverified. |
| `info_siege_radaricon` | `SP_info_siege_radaricon` | Present route | Behavior parity unverified. |
| `info_siege_decomplete` | `SP_info_siege_decomplete` | Present route | Behavior parity unverified. |
| `target_siege_end` | `SP_target_siege_end` | Present route | Behavior parity unverified. |
| `misc_siege_item` | `SP_misc_siege_item` | Present route | Behavior parity unverified. |
| `func_plat` | `SP_func_plat` | Present route | Behavior parity unverified. |
| `func_button` | `SP_func_button` | Present route | Behavior parity unverified. |
| `func_door` | `SP_func_door` | Present route | Behavior parity unverified. |
| `func_static` | `SP_func_static` | Present route | Behavior parity unverified. |
| `func_rotating` | `SP_func_rotating` | Partial | Normal rotation present; health-bearing variant unported. |
| `func_bobbing` | `SP_func_bobbing` | Present route | Behavior parity unverified. |
| `func_pendulum` | `SP_func_pendulum` | Present route | Behavior parity unverified. |
| `func_train` | `SP_func_train` | Present route | Present when valid; targetless train removed as reference does. |
| `func_group` | `SP_info_null` | Removal declared | Compare reference conditions before changing. |
| `func_timer` | `SP_func_timer` | Present route | Behavior parity unverified. |
| `func_breakable` | `SP_func_breakable` | Present route | Behavior parity unverified. |
| `func_glass` | `SP_func_glass` | Present route | Behavior parity unverified. |
| `func_usable` | `SP_func_usable` | Present route | Behavior parity unverified. |
| `func_wall` | `SP_func_wall` | Missing | See M1–M4 in stock-parity.md. |
| `trigger_lightningstrike` | `SP_trigger_lightningstrike` | Missing | See M1–M4 in stock-parity.md. |
| `trigger_always` | `SP_trigger_always` | Present route | Behavior parity unverified. |
| `trigger_multiple` | `SP_trigger_multiple` | Present route | Behavior parity unverified. |
| `trigger_once` | `SP_trigger_once` | Present route | Behavior parity unverified. |
| `trigger_push` | `SP_trigger_push` | Present route | Behavior parity unverified. |
| `trigger_teleport` | `SP_trigger_teleport` | Present route | Behavior parity unverified. |
| `trigger_hurt` | `SP_trigger_hurt` | Present route | Behavior parity unverified. |
| `trigger_space` | `SP_trigger_space` | Present route | Behavior parity unverified. |
| `trigger_shipboundary` | `SP_trigger_shipboundary` | Present route | Behavior parity unverified. |
| `trigger_hyperspace` | `SP_trigger_hyperspace` | Present route | Behavior parity unverified. |
| `trigger_asteroid_field` | `SP_trigger_asteroid_field` | Missing | See M1–M4 in stock-parity.md. |
| `target_give` | `SP_target_give` | Missing | See M1–M4 in stock-parity.md. |
| `target_remove_powerups` | `SP_target_remove_powerups` | Missing | See M1–M4 in stock-parity.md. |
| `target_delay` | `SP_target_delay` | Present route | Behavior parity unverified. |
| `target_speaker` | `SP_target_speaker` | Present route | Behavior parity unverified. |
| `target_print` | `SP_target_print` | Present route | Behavior parity unverified. |
| `target_laser` | `SP_target_laser` | Missing | See M1–M4 in stock-parity.md. |
| `target_score` | `SP_target_score` | Missing | See M1–M4 in stock-parity.md. |
| `target_teleporter` | `SP_target_teleporter` | Present route | Behavior parity unverified. |
| `target_relay` | `SP_target_relay` | Present route | Behavior parity unverified. |
| `target_kill` | `SP_target_kill` | Present route | Behavior parity unverified. |
| `target_position` | `SP_target_position` | Point/data route | Check every consumer; no runtime model is required. |
| `target_location` | `SP_target_location` | Present route | Behavior parity unverified. |
| `target_counter` | `SP_target_counter` | Present route | Behavior parity unverified. |
| `target_random` | `SP_target_random` | Present route | Behavior parity unverified. |
| `target_scriptrunner` | `SP_target_scriptrunner` | Present route | Behavior parity unverified. |
| `target_interest` | `SP_target_interest` | Missing | Missing interest-point registration; reference then frees entity. |
| `target_activate` | `SP_target_activate` | Present route | Behavior parity unverified. |
| `target_deactivate` | `SP_target_deactivate` | Present route | Behavior parity unverified. |
| `target_level_change` | `SP_target_level_change` | Missing | See M1–M4 in stock-parity.md. |
| `target_play_music` | `SP_target_play_music` | Present route | Behavior parity unverified. |
| `target_push` | `SP_target_push` | Missing | See M1–M4 in stock-parity.md. |
| `light` | `SP_light` | Partial | Unnamed entity removed; named switchable style handler missing. |
| `path_corner` | `SP_path_corner` | Point/data route | Check every consumer; no runtime model is required. |
| `misc_teleporter_dest` | `SP_misc_teleporter_dest` | Point/data route | Check every consumer; no runtime model is required. |
| `misc_model` | `SP_misc_model` | Reference no-op | PC reference frees entity; old implementation under #if 0. |
| `misc_model_static` | `SP_misc_model_static` | Removal declared | Compare reference conditions before changing. |
| `misc_G2model` | `SP_misc_G2model` | Reference no-op | PC reference frees entity; old implementation under #if 0. |
| `misc_portal_surface` | `SP_misc_portal_surface` | Present route | Behavior parity unverified. |
| `misc_portal_camera` | `SP_misc_portal_camera` | Present route | Behavior parity unverified. |
| `misc_weather_zone` | `SP_misc_weather_zone` | Removal declared | Compare reference conditions before changing. |
| `misc_bsp` | `SP_misc_bsp` | Missing | See M1–M4 in stock-parity.md. |
| `terrain` | `SP_terrain` | Missing | Missing RMG terrain spawn path, not normal BSP terrain geometry. |
| `misc_skyportal_orient` | `SP_misc_skyportal_orient` | Removal declared | Compare reference conditions before changing. |
| `misc_skyportal` | `SP_misc_skyportal` | Present route | Behavior parity unverified. |
| `gametype_item` | `SP_gametype_item` | Missing | See M1–M4 in stock-parity.md. |
| `misc_ammo_floor_unit` | `SP_misc_ammo_floor_unit` | Missing | See M1–M4 in stock-parity.md. |
| `misc_shield_floor_unit` | `SP_misc_shield_floor_unit` | Missing | Missing; reference only retains for CTF/CTY/Siege. |
| `misc_model_shield_power_converter` | `SP_misc_model_shield_power_converter` | Missing | See M1–M4 in stock-parity.md. |
| `misc_model_ammo_power_converter` | `SP_misc_model_ammo_power_converter` | Missing | See M1–M4 in stock-parity.md. |
| `misc_model_health_power_converter` | `SP_misc_model_health_power_converter` | Missing | See M1–M4 in stock-parity.md. |
| `fx_runner` | `SP_fx_runner` | Present route | Effect required; missing fxFile is refused. |
| `target_screenshake` | `SP_target_screenshake` | Missing | See M1–M4 in stock-parity.md. |
| `target_escapetrig` | `SP_target_escapetrig` | Outside ordinary MP | Reference frees unless GT_SINGLE_PLAYER. |
| `misc_maglock` | `SP_misc_maglock` | Missing | See M1–M4 in stock-parity.md. |
| `misc_faller` | `SP_misc_faller` | Missing | See M1–M4 in stock-parity.md. |
| `ref_tag` | `SP_reference_tag` | Point/data route | Check every consumer; no runtime model is required. |
| `ref_tag_huge` | `SP_reference_tag` | Point/data route | Check every consumer; no runtime model is required. |
| `misc_weapon_shooter` | `SP_misc_weapon_shooter` | Missing | See M1–M4 in stock-parity.md. |
| `NPC_spawner` | `SP_NPC_spawner` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Vehicle` | `SP_NPC_Vehicle` | Present route | Behavior parity unverified. |
| `NPC_Kyle` | `SP_NPC_Kyle` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Lando` | `SP_NPC_Lando` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Jan` | `SP_NPC_Jan` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Luke` | `SP_NPC_Luke` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_MonMothma` | `SP_NPC_MonMothma` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Tavion` | `SP_NPC_Tavion` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Tavion_New` | `SP_NPC_Tavion_New` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Alora` | `SP_NPC_Alora` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Reelo` | `SP_NPC_Reelo` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Galak` | `SP_NPC_Galak` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Desann` | `SP_NPC_Desann` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Bartender` | `SP_NPC_Bartender` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_MorganKatarn` | `SP_NPC_MorganKatarn` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Jedi` | `SP_NPC_Jedi` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Prisoner` | `SP_NPC_Prisoner` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Rebel` | `SP_NPC_Rebel` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Stormtrooper` | `SP_NPC_Stormtrooper` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_StormtrooperOfficer` | `SP_NPC_StormtrooperOfficer` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Snowtrooper` | `SP_NPC_Snowtrooper` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Tie_Pilot` | `SP_NPC_Tie_Pilot` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Ugnaught` | `SP_NPC_Ugnaught` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Jawa` | `SP_NPC_Jawa` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Gran` | `SP_NPC_Gran` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Rodian` | `SP_NPC_Rodian` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Weequay` | `SP_NPC_Weequay` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Trandoshan` | `SP_NPC_Trandoshan` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Tusken` | `SP_NPC_Tusken` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Noghri` | `SP_NPC_Noghri` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_SwampTrooper` | `SP_NPC_SwampTrooper` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Imperial` | `SP_NPC_Imperial` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_ImpWorker` | `SP_NPC_ImpWorker` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_BespinCop` | `SP_NPC_BespinCop` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Reborn` | `SP_NPC_Reborn` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_ShadowTrooper` | `SP_NPC_ShadowTrooper` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Murjj` | `SP_NPC_Monster_Murjj` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Swamp` | `SP_NPC_Monster_Swamp` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Howler` | `SP_NPC_Monster_Howler` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_MineMonster` | `SP_NPC_MineMonster` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Claw` | `SP_NPC_Monster_Claw` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Glider` | `SP_NPC_Monster_Glider` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Flier2` | `SP_NPC_Monster_Flier2` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Lizard` | `SP_NPC_Monster_Lizard` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Fish` | `SP_NPC_Monster_Fish` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Wampa` | `SP_NPC_Monster_Wampa` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Monster_Rancor` | `SP_NPC_Monster_Rancor` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Interrogator` | `SP_NPC_Droid_Interrogator` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Probe` | `SP_NPC_Droid_Probe` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Mark1` | `SP_NPC_Droid_Mark1` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Mark2` | `SP_NPC_Droid_Mark2` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_ATST` | `SP_NPC_Droid_ATST` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Seeker` | `SP_NPC_Droid_Seeker` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Remote` | `SP_NPC_Droid_Remote` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Sentry` | `SP_NPC_Droid_Sentry` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Gonk` | `SP_NPC_Droid_Gonk` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Mouse` | `SP_NPC_Droid_Mouse` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_R2D2` | `SP_NPC_Droid_R2D2` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_R5D2` | `SP_NPC_Droid_R5D2` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Droid_Protocol` | `SP_NPC_Droid_Protocol` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Reborn_New` | `SP_NPC_Reborn_New` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Cultist` | `SP_NPC_Cultist` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Cultist_Saber` | `SP_NPC_Cultist_Saber` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Cultist_Saber_Powers` | `SP_NPC_Cultist_Saber_Powers` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Cultist_Destroyer` | `SP_NPC_Cultist_Destroyer` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Cultist_Commando` | `SP_NPC_Cultist_Commando` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Colombian_Soldier` | `SP_NPC_Reborn` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Colombian_Rebel` | `SP_NPC_Reborn` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Colombian_EmplacedGunner` | `SP_NPC_ShadowTrooper` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `NPC_Manuel_Vergara_RMG` | `SP_NPC_Desann` | NPC route | Shared NPC spawn routing; alias/assets/AI not individually certified. |
| `waypoint` | `SP_waypoint` | Present route | Behavior parity unverified. |
| `waypoint_small` | `SP_waypoint_small` | Present route | Behavior parity unverified. |
| `waypoint_navgoal` | `SP_waypoint_navgoal` | Present route | Behavior parity unverified. |
| `waypoint_navgoal_8` | `SP_waypoint_navgoal_8` | Present route | Behavior parity unverified. |
| `waypoint_navgoal_4` | `SP_waypoint_navgoal_4` | Present route | Behavior parity unverified. |
| `waypoint_navgoal_2` | `SP_waypoint_navgoal_2` | Present route | Behavior parity unverified. |
| `waypoint_navgoal_1` | `SP_waypoint_navgoal_1` | Present route | Behavior parity unverified. |
| `fx_spacedust` | `SP_CreateSpaceDust` | Present route | Behavior parity unverified. |
| `fx_rain` | `SP_CreateRain` | Present route | Behavior parity unverified. |
| `fx_snow` | `SP_CreateSnow` | Present route | Behavior parity unverified. |
| `point_combat` | `SP_point_combat` | Present route | Behavior parity unverified. |
| `misc_holocron` | `SP_misc_holocron` | Present route | Behavior parity unverified. |
| `shooter_blaster` | `SP_shooter_blaster` | Missing | See M1–M4 in stock-parity.md. |
| `team_CTF_redplayer` | `SP_team_CTF_redplayer` | Present route | Behavior parity unverified. |
| `team_CTF_blueplayer` | `SP_team_CTF_blueplayer` | Present route | Behavior parity unverified. |
| `team_CTF_redspawn` | `SP_team_CTF_redspawn` | Present route | Behavior parity unverified. |
| `team_CTF_bluespawn` | `SP_team_CTF_bluespawn` | Present route | Behavior parity unverified. |
| `item_botroam` | `SP_item_botroam` | Point/data route | Check every consumer; no runtime model is required. |
| `emplaced_gun` | `SP_emplaced_gun` | Present route | Behavior parity unverified. |
| `misc_turret` | `SP_misc_turret` | Present route | Behavior parity unverified. |
| `misc_turretG2` | `SP_misc_turretG2` | Present route | Behavior parity unverified. |
