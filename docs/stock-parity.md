# Stock multiplayer parity audit

Source audit of JKR `af65396fb98acad96a7e3bc3f32bc3c12ca12897`, 2026-10-05.
This is an implementation inventory and repair backlog, not runtime certification.
This baseline audit preceded implementation. See [repair progress](stock-parity-progress.md)
for subsequent changes and their evidence; tables here describe the baseline.

## Reference and method

The inventory uses the original PC multiplayer source preserved by the
[Jedi Academy source mirror at 3fd984a](https://github.com/zachlatta/jedi-academy/tree/3fd984a7b82f29bc3f54410de248de69091c9699/codemp).
This is a mirror of the released source, not a Raven-operated repository; the
download was not authenticated against the original release ZIP. Another mirror
examined includes Xbox differences and is not used interchangeably with this PC
baseline. Only `codemp` defines multiplayer behavior here.

[EternalJK at a40e793](https://github.com/eternalcodes/EternalJK/tree/a40e793a802c8db8df6132b711eea2fc846e5dd1/codemp)
is a secondary reference for maintained behavior and extensions. An EternalJK or
TaystJK feature is not automatically a stock requirement. Subsequent combat and
wire changes still require the **OpenJK codemp evidence** specified in
[development.md](development.md#verification-appropriate-to-the-change).

The pass inventoried all 185 entries in the PC `g_spawn.c` spawn table, the
188 case labels in `CG_EntityEvent`, `CG_AddCEntity` entity families, reliable
cgame commands, and the major client/server subsystems below. Entry counts are
not implemented-feature counts. Conditional compilation, reference no-ops and
shared handlers matter. The [spawn appendix](stock-spawns.md) records every map
spawn entry; ordinary pickup items are dispatched separately through `bg_itemlist`.

Findings trace call sites and production host implementations where possible.
A parser, registered cvar, shared helper, test-host default, or comment saying
“ported” is not sufficient evidence of end-to-end support. Likewise, several old
“not ported” comments now contradict executable code.

**Missing** means the inspected production path has no implementation for the
identified operation. **Partial** means a path exists but omits specific stock
behavior. **Present / unverified** means implementation was found but this audit
did not establish exact parity. No percentage of overall completion is claimed.

## System coverage

| System | Source finding | Remaining verification or gap |
| --- | --- | --- |
| Protocol 26 transport, deltas, reliable commands | Present | New byte-fixture campaign not run; command consumers have gaps below |
| Connect, downloads, client pure package checks | Present | Broader missing/mismatched PK3 and reconnect matrix remains open |
| Native pure-server enforcement | Missing | N1; client package-check support does not supply server enforcement |
| Browser queries, heartbeats, RCON, bans, rate limits | Present, query fields partial | N2; query policy, password and populated counts need comparison |
| Map changes, restart, intermission, demo I/O | Present | Record/playback and mixed native/legacy lifecycle comparison still needed |
| Walking, swimming, air movement, wall moves, prediction | Present, specific omissions | G1; exact command-step parity is not re-established by this audit |
| Saber moves, locks, throw/return, hit traces | Present | Timing, collision and damage need reference fixtures; old stub comments are stale |
| Guns, missiles, mines, damage, death | Present, specific omissions | G2–G4; player rocket homing exists, vehicle homing differs |
| Force powers and profile negotiation | Present, partial integration | G3, G4, N3; do not confuse absent mover interactions with absent Force powers |
| Pickups, powerups, holdables | Present, partial | N4; shield simulation exists but client shield drawing is missing |
| FFA, team FFA, duel, power duel | Present | Full match progression and team transitions unverified in this pass |
| CTF / CTY | Present, client feedback partial | C4, C8; captures, returns, assists and HUD need an end-to-end matrix |
| Holocron / Jedi Master | Present | Dedicated bridge and rule modules exist; full match behavior unverified |
| Siege | Substantial server implementation, incomplete client | C1–C3, N3, M1, S1–S3; do not advertise complete parity |
| Bots and navigation | Present | Route loading, objectives, combat and map transitions need mixed-client runs |
| NPC spawning, class AI, pain and death | Present | Most named `Stub` branches now dispatch to real behavior; scripting remains partial |
| Vehicles, boarding, riders, turrets | Present, partial weapons/aim | G2, C10; broader vehicle definitions and destruction sequences unverified |
| Movers, triggers, targets and map scenery | Present, sizeable missing subset | M1–M4 and complete spawn appendix |
| ICARUS scripts / ROFF | Interpreter present, host partial / ROFF missing | S1–S3; a valid script can still have ineffective actions |
| RBSP, inline models, collision, portals | Present | Nested BSP/RMG path missing on native server; not equivalent to ordinary BSP terrain |
| PK3, skins, GLM/GLA, MD3, appearance refresh | Present | Broad community-asset compatibility unverified; C5 identifies lifecycle omissions |
| Player/NPC animations, bolts and equipment | Present, animation events partial | C5, C6; model-specific timing and surface behavior need fixtures |
| Shader stages, fog, weather, remaps, light grids | Present, content semantics partial | C11, C12; modern lighting is intentional, not a pixel-parity failure |
| Scripted EFX, weapon impacts, decals, model emitters | Present, stock event dispatch incomplete | C6–C8; generic effects support does not cover all hard-coded cgame effects |
| Positional sound, voices, loops, ambient sets, music | Present, partial | C2, C9, C13; ordinary footsteps/taunts and loop events already implemented |
| HUD, chat, scoreboards, selection and menus | Present, missing stock information | C1–C4, C10; preserve information, not legacy menu appearance |
| Console, binds, config, filesystem layout | Present | N3 identifies commands missing at the server end; registration alone is not parity |
| Linux / Windows packaging and runtime | Packaging present | This audit performs no fresh GPU or Windows runtime validation |

## Client findings: affect play on existing servers

| ID | Status and visible consequence | Evidence and repair boundary |
| --- | --- | --- |
| C1 | **Missing:** Siege briefing (`sb`) and extended teammate health/ammo data (`sxd`) are not consumed. | Raven `cg_servercmds.c:CG_ServerCommand`; JKR [session dispatch](../crates/jkr-client/src/lib.rs). The native server already emits `sxd` in [bridge_siege_clients.rs](../crates/jkr-dedicated/src/bridge_siege_clients.rs). Class-selection commands `scl`/`spc` do work. |
| C2 | **Missing:** Siege voice-command indicator above the speaker. | Raven `cg_players.c:CG_PlayerSprites` and `cg_event.c:EV_VOICECMD_SOUND`; [player_sprites.rs](../crates/jkr-client/src/player_sprites.rs) only handles connection/talk. Voice-command audio is already consumed by [sound_events.rs](../crates/jkr-client/src/sound_events.rs). |
| C3 | **Missing:** radar and `EV_LOCALTIMER` action-progress UI; Siege round/objective event presentation has no corresponding consumer. | Raven `cg_draw.c:CG_DrawRadar`, `cg_event.c:EV_LOCALTIMER`, `EV_SIEGE_ROUNDOVER`, `EV_SIEGE_OBJECTIVECOMPLETE`; [HUD](../crates/jkr-viewer/src/hud.rs), [combat announcements](../crates/jkr-viewer/src/chat/combat.rs). The `cg_drawradar` check in [enemy_info.rs](../crates/jkr-viewer/src/hud/enemy_info.rs) only suppresses another overlay. The normal match clock is not this action timer. |
| C4 | **Missing:** entity-specific portable-shield and beam presentation. | Raven `cg_ents.c:CG_Special`, `CG_Beam`, `CG_TeamBase`; [presentation.rs](../crates/jkr-client/src/presentation.rs) falls through to generic appearance rather than these special renderers. Shield IDs are not `CS_MODELS` indices. Current OpenJK deliberately ignores `ET_TEAM`, unlike the old Raven team-base renderer. Server shield collision/health exist, so invisible or unrelated geometry is a gameplay risk. Flag pickup models and player armor-hit effects are separate implemented paths. |
| C5 | **Missing / partial:** detached limb presentation, client ragdolls, Ghoul2-attached damage marks, corpse fade, and associated mutable-model cleanup. | Raven `cg_ents.c:CG_General`, `cg_players.c:CG_RagDoll`, `cg_event.c:CG_G2MarkEvent` / `EV_BODYFADE`; [presentation.rs](../crates/jkr-client/src/presentation.rs), [actor_load.rs](../crates/jkr-viewer/src/actor_load.rs). `G2_MODEL_PART` is deliberately excluded from generic model lookup. Server dismemberment and basic body-copy/`kg2` support exist; neither supplies all client effects. Ragdoll behavior must be checked under the reference's cvar/eligibility conditions. |
| C6 | **Missing:** animation-triggered `AEV_EFFECT` cues. | Raven `cg_players.c:CG_PlayerAnimEventDo`; [animation event parser](../crates/jkr-client/src/animation_events/parser.rs) and cue type only produce sound/footstep cues. Add model/weapon bolt-aware effect submission rather than another sound special case. |
| C7 | **Missing:** `EV_GLASS_SHATTER`, `EV_DEBRIS`, `EV_MISC_MODEL_EXP`, and `EV_PLAY_ROFF` presentation. | Raven `cg_event.c`; [event effect selection](../crates/jkr-viewer/src/combat_effects.rs), [effect runtime](../crates/jkr-viewer/src/effect_runtime.rs), [impact extraction](../crates/jkr-client/src/impact_events.rs). Native breakables already emit glass/debris events. This concerns stock breakage, independently of the parked reflection experiment. |
| C8 | **Missing:** specific event paths for screen shake, concussion alternate-fire beam/impact, and CTF message text. | Raven `cg_event.c:EV_SCREENSHAKE`, `EV_CONC_ALT_IMPACT`, `EV_CTFMESSAGE`; inspected impact, sound, announcement and effect consumers have no corresponding complete path. EFX camera-shake components, ordinary missile impacts, CTF announcer audio and score plums exist; those do not substitute for these events. |
| C9 | **Missing / partial:** reliable `kls` loop-stop commands and automatic `ET_SPEAKER` random/wait playback. | Raven `cg_servercmds.c:kls`, `cg_ents.c:CG_Speaker`; [session dispatch](../crates/jkr-client/src/lib.rs), [loop_sounds.rs](../crates/jkr-client/src/loop_sounds.rs), [sound event latches](../crates/jkr-client/src/sound_event_latches.rs). Ordinary entity loops, ambient sets and `EV_STARTLOOPINGSOUND` / `EV_STOPLOOPINGSOUND` exist. |
| C10 | **Partial:** vehicle/emplaced crosshair aiming uses camera fallback, not the stock muzzle/precision paths. | Explicit exclusion in [crosshair_dynamic.rs](../crates/jkr-viewer/src/crosshair_dynamic.rs); Raven `cg_draw.c:CG_ScanForCrosshairEntity` and `CG_CalcVehicleMuzzlePoint`. This is evidence of a display discrepancy, not proof every authoritative shot goes the wrong way. |
| C11 | **Missing:** animated/switched light-style consumption. | Raven `cg_light.c:CG_RunLightStyles`; [entity_lighting.rs](../crates/jkr-viewer/src/entity_lighting.rs) supplies constant white style values. BSP style IDs are parsed but do not establish animated rendering. Fixing this preserves authored flicker/switching; it does not by itself solve the general dark-indoor-lighting problem. |
| C12 | **Missing:** `videoMap` / RoQ-backed shader surfaces. | Raven `renderer/tr_shader.cpp` handles `videoMap` through the cinematic service; [stage_parse.rs](../crates/jkr-shader/src/stage_parse.rs) has no such stage operation, and there is no cinematic decoder/playback service. Ordinary `animMap` stages are a different implemented feature. |
| C13 | **Partial:** dynamic music selection lacks marker transitions/crossfades and rejects boss/death states. | [audio_commands.rs](../crates/jkr-viewer/src/audio_commands.rs), Raven `client/snd_music.cpp` / `snd_dma.cpp`. Content-dependent; normal intro/loop playback and explore/action selection exist. |
| C14 | **Missing:** reliable localized center-print `cps`; `clientLevelShot` has no consumer. | Raven `cg_servercmds.c`; [session dispatch](../crates/jkr-client/src/lib.rs). Plain `cp` works. Level-shot capture is a developer feature, lower priority than gameplay messaging. |

C5 also warrants event-specific verification of `EV_DESTROY_GHOUL2_INSTANCE`
and `EV_DESTROY_WEAPON_MODEL`, beyond reliable `kg2`. The rank/free-saber/Force
disable events need a separate comparison with the implemented reliable `nfr`
and serverinfo paths. Their presence in the reference event switch does not
prove they are the active transport on every server.

## Native-server and shared-game findings

These do not imply that the same action is broken on an existing OpenJK/Tayst
server: the remote server may already own the missing authoritative behavior.
Shared prediction omissions can affect both clients and native hosting.

| ID | Status and consequence | Evidence and repair boundary |
| --- | --- | --- |
| N1 | **Missing:** native pure-client validation. | [server session host](../crates/jkr-network/src/server_session/hosts.rs) fixes `pure_required` to false and swallows `cp` / `vdr`; [bridge_maps.rs](../crates/jkr-dedicated/src/bridge_maps.rs) publishes `sv_pure=0`. Reading a `sv_pure` cvar does not implement validation. Compare reference `SV_VerifyPaks_f` and admission before offering pure hosting. |
| N2 | **Partial:** server-browser responses contain placeholder settings. | [bridge_host.rs](../crates/jkr-dedicated/src/bridge_host.rs) supplies gametype/needpass/weapon/Force restrictions as zero and uses peer count for humans. [datagram.rs](../crates/jkr-network/src/server_session/datagram.rs) serializes that object directly. Non-FFA, passworded and bot-only servers need accurate query metadata; `getstatus` uses a separate path. |
| N3 | **Missing / partial:** stock command handling, notably `forcechanged`, `voice_cmd`, `god`, `notarget`, `giveother`, `where`, `levelshot`, and named-item `give`. | [bridge_commands.rs](../crates/jkr-dedicated/src/bridge_commands.rs), [bridge_chat.rs](../crates/jkr-dedicated/src/bridge_chat.rs), [bridge_cheats.rs](../crates/jkr-dedicated/src/bridge_cheats.rs), [give.rs](../crates/jkr-game-jka/src/give.rs). `Given::Item` is not applied. `forcechanged` should apply spectator Force changes / defer active-player changes and handle its optional team argument; userinfo updates alone are not proof of those semantics. Noclip, NPC, `t_use`, team/follow/vote/chat/`gc` do have routes. |
| N4 | **Missing:** native use of sentry, jetpack and E-Web holdables. | Explicit `UNPORTED` list in [bridge_holdables.rs](../crates/jkr-dedicated/src/bridge_holdables.rs). Use is refused or the item restored. Movement helpers and item definitions alone do not spawn/manage the deployable. Portable shield server behavior is implemented in [bridge_shields.rs](../crates/jkr-dedicated/src/bridge_shields.rs). |
| G1 | **Partial:** jump path omits the `PMF_RESPAWNED` gate and zero-gravity push-off. | [pmove_jump.rs](../crates/jkr-game-jka/src/pmove_jump.rs) explicitly bypasses both; reference `bg_pmove.c:PM_CheckJump`. An old synthetic-test accommodation is not a stock gameplay policy. Repair only with 8/7/4/3 ms comparisons, including prediction and respawn input. |
| G2 | **Partial:** vehicle guns lack vehicle lock-on/homing, zero-speed mine touch/owner-solid callbacks, shoot-down health, and long-range fighter camera aiming. | [vehicle_weapons.rs](../crates/jkr-game-jka/src/vehicle_weapons.rs), [pmove_vehicle.rs](../crates/jkr-game-jka/src/pmove_vehicle.rs). Projectile construction and firing work but do not populate these behaviors. Ordinary player rocket homing and walker camera correction already exist. |
| G3 | **Partial:** Force push/pull does not enumerate map movers/items as stock interaction targets. | [bridge_force.rs](../crates/jkr-dedicated/src/bridge_force.rs) gathers players, missiles and NPCs; [force_throw.rs](../crates/jkr-game-jka/src/force_throw.rs) has no corresponding door/button/static interaction implementation. Reference `w_force.c:ForceThrow` has entity-specific branches. Check force-operated geometry independently of ordinary player knockback. |
| G4 | **Partial:** player Force Sight dodge is a boolean eligibility test, not the reference dodge action with its side effects. Rocket target acquisition is also player-only and lacks cloak rejection. | [disruptor.rs](../crates/jkr-game-jka/src/disruptor.rs), its calls in [bridge.rs](../crates/jkr-dedicated/src/bridge.rs), and [pmove_weapon_charge.rs](../crates/jkr-game-jka/src/pmove_weapon_charge.rs). Compare costs, animation, speed and `g_forceDodge` policy before drawing balance conclusions. NPC dodge has a separate substantial implementation. |

## Map and scripting findings

| ID | Missing or partial behavior | Scope and evidence |
| --- | --- | --- |
| M1 | Ammo/health/shield recharge stations | `misc_ammo_floor_unit`, `misc_shield_floor_unit`, and the three `misc_model_*_power_converter` types. Stock shield-floor station is gametype-gated. Raven `g_misc.c` contains real use/recharge callbacks; JKR [spawn_table.rs](../crates/jkr-game-jka/src/spawn_table.rs) marks them unported and the bridge has no implementation. |
| M2 | Map target logic and toggleable walls | `func_wall`, `target_give`, `target_remove_powerups`, `target_score`, `target_laser`, `target_push`, `target_level_change`, `target_interest`, `target_screenshake`. See Raven `g_mover.c`, `g_target.c`, `g_trigger.c`, `g_misc.c`, `NPC_senses.c`. `target_interest` registers data before freeing itself; freeing it is not enough. Ordinary `trigger_push` is already implemented. |
| M3 | Scripted hazards and map machinery | `trigger_lightningstrike`, `trigger_asteroid_field`, `misc_maglock`, `misc_faller`, `misc_weapon_shooter`, `shooter_blaster`. These have active Raven callbacks, not merely editor declarations. Effects alone cannot substitute for damage, spawning, use links or timing. |
| M4 | Conditional and alternate map construction paths | Native `misc_bsp`, RMG `terrain`, `gametype_item`, named switchable `light`, health-bearing `func_rotating`, and binary-use behavior on path movers. [bridge_path_movers.rs](../crates/jkr-dedicated/src/bridge_path_movers.rs) explicitly reports its missing use path. Normal terrain made from BSP geometry and ordinary rotating movers are separate supported paths. Sub-BSP client rendering requires its own verification; this finding is about native map construction. |
| S1 | Scripts cannot address much of the actual game world | [bridge_icarus.rs](../crates/jkr-dedicated/src/bridge_icarus.rs) registers players and selected static/script-runner/trigger entities. Named doors, counters, hurt brushes and NPCs are not general script targets; corresponding `affect`/find operations cannot work as stock does. |
| S2 | Script client/NPC actions stop at the host boundary | [bridge_icarus_world.rs](../crates/jkr-dedicated/src/bridge_icarus_world.rs) applies health, armor, death and teleport. View angle, velocity, gravity, scale, nodraw, Force levels, weapon, saber toggle and animation actions instead report unimplemented. NPC actions report unsupported. The action enums and VM already exist; connect them to real ownership rather than extending the parser. |
| S3 | ROFF and scripted rotation integration absent | Same host returns zero from ROFF caching and does nothing for playback; mover pushing discards the turn delta. Stock ROFF playback also needs the client path in C7. Compare actual files, task completion and entities riding the mover. |

## Things deliberately not counted as missing stock features

- `misc_model` and `misc_G2model` are labelled unported in JKR's table, but the
  selected PC reference disables their old runtime implementation and frees them.
  Map-compiler-baked static geometry is not dependent on those handlers.
- `misc_model_breakable` appears in JKR's table but not the selected PC spawn
  table. Treat another reference's support as a separate compatibility question.
- `target_escapetrig` is retained only for `GT_SINGLE_PLAYER` in the multiplayer
  source. It is not ordinary multiplayer missing functionality.
- Current OpenJK disables `teamtask` and ignores `ET_TEAM`; their older Raven
  code is not a requirement to restore those behaviors.
- Raven's `stats` command has an entirely commented-out body and current OpenJK
  removes it. EternalJK's account statistics are an extension, not missing stock behavior.
- Reference multiplayer `AEV_FIRE` and `AEV_MOVE` handlers do nothing. The
  health/ammo dispenser holdable use calls are commented out in the reference.
- The reference `spd` reliable handler is under `#if 0`. `loaddefered` concerns
  legacy deferred loading; native asynchronous loading needs an equivalence test,
  not necessarily a same-named command implementation.
- Most NPC `Behavior::Stub` values dispatch onward to implemented Jedi, creature,
  soldier or state routines. Test-only stand-ins are not production absences.
  The old “slope poses not ported” and “rocket lock not ported” comments also
  conflict with existing slope and homing implementations.
- A false `ClientCommandPolicy.active` initializer is overwritten from the actual
  connection phase before command admission in [server_message/mod.rs](../crates/jkr-network/src/server_message/mod.rs).
  This is not evidence that flood protection is absent.
- EFX sound, light, shake, decal, emitter/model, nested-runner and geometric
  primitives have implementations. Remaining flag combinations, custom blend
  fallbacks and any unrecognized primitive types need content-level coverage.
- Modern menus, independent chat, lighting, faster loading and the removed
  sustained-map experiment are intentional product decisions. They do not remove
  the obligation to display stock gameplay information.
- Tayst race cosmetics, hats/capes, jaPRO additions and the sand-creature extension
  are separate from PC stock multiplayer parity. Native hosting does not execute
  arbitrary third-party game DLLs; a compatibility userinfo profile cannot promise that.

## Recommended repair order and acceptance evidence

1. **Stock-client gameplay information and visible entities:** C4 (especially
   portable shields), C1/C3/C9/C14, then C7/C8. Use an isolated reference server
   sending the actual commands/entities. Compare a legacy client and JKR at the
   same positions; test join-in-progress, entity reuse, restart and demo playback.
   Do not automate chat-message tests.
2. **Native server essentials:** N3, M1, N4, N2 and N1. Verify with both a legacy
   client and JKR. For pure mode, prove valid/invalid/missing pak admission with
   reference-generated traffic; preserve protocol bytes and download policy.
3. **Finish Siege as a complete flow:** classes, objectives, use timers,
   briefings, teammates, vehicles, round endings and map changes. Missing script
   and map-entity dependencies must be included, not masked by client UI work.
4. **Combat/movement exceptions:** G1–G4. Compare reference output at 8/7/4/3 ms
   with both client prediction and native authority; measure affected hot paths.
   Do not implement a plausible approximation based on playtest appearance.
5. **Broader map/script/content compatibility:** remaining M/S rows, C5/C6,
   light styles, video surfaces and music transitions. Use small original maps
   and installed content without redistributing retail data.

The owner subsequently authorized implementing this backlog. Progress and
remaining acceptance checks are tracked in [repair progress](stock-parity-progress.md).
Implementation authorization does not authorize publication.

## Verification limits

Linux workspace formatting, locked build and locked tests (27 tests) pass for
this documentation-only audit. Relative documentation links were checked.
No live server, GPU, audio, performance, Windows or combat fixture run was made.
The existing tests do not validate the missing operations catalogued here.

The entire source tree was searched for candidate omissions and major production
entry points were reviewed. This is not a statement that every line, cvar,
effect flag, asset, NPC decision, event side effect or reference platform has
been exhaustively compared. Present systems above still need end-to-end parity
tests; unmapped event sub-effects and build-conditional reference behavior remain
explicit review work. Preserve this distinction when updating the backlog.
