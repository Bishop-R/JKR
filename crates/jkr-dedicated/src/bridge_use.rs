//! The use key on this server (`ClientThink_real`, `codemp/game/g_active.c:3368-3372`, and
//! `TryUse`, `g_utils.c:1594-1760`): while the key is held, every 100 ms (`ps.useDelay`),
//! a player not busy reaches for what is in front of its eyes — or, on a vehicle, gets off
//! it. A vehicle it reaches it boards; a button is pressed, a `func_usable` fires, a siege
//! objective is taken.

use super::*;
use jkr_game_jka::npc_spawn::NpcHost;
use jkr_game_jka::vehicle_drive::VehicleUse;

/// `BUTTON_USE`.
const BUTTON_USE: u16 = jkr_game_jka::use_key::BUTTON_USE;
/// `BOTH_BUTTON_HOLD`, `BOTH_CONSOLE1`: the poses a weapon's time does not stop using in.
const BOTH_BUTTON_HOLD: u16 = 1_328;
const BOTH_CONSOLE1: u16 = 954;
/// `HANDEXTEND_NONE`, `HANDEXTEND_DRAGGING`.
const HANDEXTEND_NONE: u8 = 0;
const HANDEXTEND_DRAGGING: u8 = 15;
/// `PMF_FOLLOW`.
const PMF_FOLLOW: u16 = 4_096;
/// `SETANIM_TORSO`, `SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD`.
const SETANIM_TORSO: u8 = 1;
const OVERRIDE_HOLD: u8 = 1 | 2;
/// `STAT_HOLDABLE_ITEMS`; `HI_HEALTHDISP`, `HI_AMMODISP`.
const STAT_HOLDABLE_ITEMS: usize = 2;
const DISPENSERS: u32 =
    (1 << jkr_game_jka::holdables::HI_HEALTHDISP) | (1 << jkr_game_jka::holdables::HI_AMMODISP);
/// `GT_POWERDUEL`, `GT_SINGLE_PLAYER`, `GT_TEAM`.
const GT_POWERDUEL: i32 = 4;
const GT_SINGLE_PLAYER: i32 = 5;
const GT_TEAM: i32 = 6;

/// The use pose `TryUse` holds a player in while it uses something: a pose already held
/// (the button's, and with `console_too` the console's) kept 500 ms more, else the button's
/// begun; the weapon waits as long (`ps.weaponTime = ps.torsoTimer`).
pub(super) fn hold_use_pose(peer: &mut crate::peer::Peer, console_too: bool) {
    let torso = peer.state.torso_animation();
    if torso == BOTH_BUTTON_HOLD || (console_too && torso == BOTH_CONSOLE1) {
        peer.state.set_raw_field(20, 500);
    } else {
        peer.movement
            .set_animation_parts(SETANIM_TORSO, BOTH_BUTTON_HOLD, OVERRIDE_HOLD);
        peer.movement.write_player_state(&mut peer.state);
    }
    let torso_timer = peer.state.torso_timer();
    peer.state.set_raw_field(10, torso_timer as u32);
}

impl NativeGame {
    /// `ClientThink_real`'s use key: held, `TryUse` every 100 ms.
    pub(super) fn try_use(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if peer.last_command.buttons & BUTTON_USE == 0 || peer.riding.use_delay >= level_time {
            return;
        }
        self.use_now(client, level_time);
        if let Some(peer) = self.peer_mut(client) {
            peer.riding.use_delay = level_time + 100;
        }
    }

    /// `TryUse` (`g_utils.c:1594-1760`).
    fn use_now(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let state = &peer.state;
        let torso = state.torso_animation();
        let busy = state.weapon_time() > 0 && torso != BOTH_BUTTON_HOLD && torso != BOTH_CONSOLE1;
        let hands = state.force_hand_extend();
        if busy
            || peer.health < 1
            || !peer.playing()
            || state.movement_flags() & PMF_FOLLOW != 0
            || !matches!(hands, HANDEXTEND_NONE | HANDEXTEND_DRAGGING)
            || state.emplaced_index() != 0
        {
            return;
        }
        if state.vehicle_entity_num() != 0 {
            // "on an emplaced gun or using a vehicle": off it, unless still boarding.
            let _ = self.with_rider(client, |roster, rider, host| {
                let bodies = Self::bodies_but_owned(roster, rider.number);
                let owner = rider.number;
                let mut trace = |start, mins, maxs, end, mask| {
                    host.trace(start, mins, maxs, end, owner, mask, &bodies)
                };
                roster.use_while_riding(rider, level_time, &mut trace)
            });
            return;
        }
        // "can't use anything else so jp is off" (`goto tryJetPack`).
        if peer.gear.packs.jetpack_on {
            self.jetpack_by_use_key(client, level_time);
            return;
        }
        let Some(found) = self.use_target(client) else {
            self.jetpack_by_use_key(client, level_time);
            return;
        };
        if self.use_vehicle(client, found, level_time)
            || self.use_dispensers(client, found, level_time)
            || self.use_gun(client, found, level_time)
        {
            return;
        }
        if self.use_station(client, found, level_time) {
            return;
        }
        // A button the use key reached is pressed (`Use_BinaryMover`).
        if let Some(index) = self.doors.iter().position(|(number, door)| {
            number.legacy_number() == found && door.kind == jkr_game_jka::movers::MoverKind::Button
        }) {
            let fired = jkr_game_jka::mover_team::use_mover(
                &mut self.doors,
                index,
                level_time,
                Some(client),
            );
            self.publish_team(index);
            if let Some(target) = fired
                && !target.is_empty()
            {
                self.fire_targets(&target, client, level_time);
            }
            return;
        }
        // A `func_usable` the key may use (`ValidUseTarget`, `GlobalUse`).
        if self.use_usable_by_key(client, found, level_time) {
            return;
        }
        self.jetpack_by_use_key(client, level_time);
    }

    /// `TryUse`'s vehicle (`g_utils.c:1699-1723`): the one the player rides it gets off, any
    /// other it boards (its allied team's alone in a team game); either way the press is
    /// spent. Whether `found` is a vehicle.
    fn use_vehicle(&mut self, client: usize, found: u16, level_time: i32) -> bool {
        let gametype = self.gametype;
        let result = self.with_rider(client, |roster, rider, host| {
            let (zoomed, team) = (rider.player.zoom_mode() != 0, 0);
            let wanted = roster.vehicle_use(found, rider, zoomed, gametype, team);
            match wanted {
                VehicleUse::NotOne => return false,
                VehicleUse::GetOff => {
                    let bodies = Self::bodies_but_owned(roster, rider.number);
                    let owner = rider.number;
                    let mut trace = |start, mins, maxs, end, mask| {
                        host.trace(start, mins, maxs, end, owner, mask, &bodies)
                    };
                    roster.eject(found, rider, false, level_time, &mut trace);
                }
                VehicleUse::GetOn => {
                    roster.board(found, rider, level_time, host);
                }
                VehicleUse::Refused => {}
            }
            rider.command.buttons &= !BUTTON_USE;
            true
        });
        result.unwrap_or(false)
    }

    /// `TryUse`'s dispensers (`g_utils.c:1752-1777`): a player carrying either, facing a
    /// live teammate (`OnSameTeam`) who needs health or ammunition, gives it what it needs
    /// (`G_UseDispenserOn`) and holds the use pose. Whether it did. An NPC is nobody's
    /// teammate here; a vehicle on the user's team, which stock would serve, is not.
    fn use_dispensers(&mut self, client: usize, found: u16, level_time: i32) -> bool {
        let target = usize::from(found);
        let gametype = self.gametype;
        let (Some(user), Some(mate)) = (self.peer(client), self.peer(target)) else {
            return false;
        };
        let same_team = match gametype {
            GT_POWERDUEL => user.session.duel_team == mate.session.duel_team,
            GT_SINGLE_PLAYER => user.bot == mate.bot,
            _ if gametype < GT_TEAM => false,
            _ => user.session.team == mate.session.team,
        };
        if user.state.stats[STAT_HOLDABLE_ITEMS] & DISPENSERS == 0
            || target == client
            || !mate.playing()
            || mate.health <= 0
            || !same_team
        {
            return false;
        }
        let health = jkr_game_jka::dispensers::needs_health(&mate.state, mate.health);
        if !health && !jkr_game_jka::dispensers::needs_ammo(&mate.state, mate.health) {
            return false;
        }
        let mut pace = user.service.supply_debounce;
        let Some(mate) = self.peer_mut(target) else {
            return false;
        };
        if health {
            jkr_game_jka::dispensers::heal(
                &mut mate.state,
                &mut mate.health,
                &mut mate.service,
                level_time,
            );
        }
        if jkr_game_jka::dispensers::needs_ammo(&mate.state, mate.health) {
            jkr_game_jka::dispensers::supply(
                &mut mate.state,
                &mut mate.service,
                &mut pace,
                level_time,
            );
        }
        mate.movement = mate.movement.reseeded(&mate.state);
        if let Some(user) = self.peer_mut(client) {
            user.service.supply_debounce = pace;
            hold_use_pose(user, false);
        }
        true
    }

    /// The NPCs' bodies a rider's traces meet: all but the vehicle it owns.
    fn bodies_but_owned(
        roster: &jkr_game_jka::npc_roster::NpcRoster,
        rider: u16,
    ) -> Vec<BoxObstacle> {
        roster
            .actors
            .iter()
            .filter(|npc| npc.contents != 0 && jkr_game_jka::vehicle_board::owner_of(npc) != rider)
            .map(|npc| npc.body())
            .collect()
    }

    /// `TryUse`'s trace (`g_utils.c:1661-1675`): what the player is looking at within reach,
    /// if anything.
    fn use_target(&mut self, client: usize) -> Option<u16> {
        let (eyes, angles) = {
            let peer = self.peer_mut(client)?;
            let origin = peer.state.origin();
            let height = peer.state.view_height() as f32;
            (
                [origin[0], origin[1], origin[2] + height],
                peer.state.view_angles(),
            )
        };
        let (from, to) = jkr_game_jka::use_key::reach(eyes, angles);
        self.gather_obstacles(client);
        let Self { map, obstacles, .. } = self;
        let map = map.as_ref()?;
        let world = WorldCollision {
            bsp: &map.bsp,
            scratch: &map.scratch,
        };
        let trace = WithPlayers {
            world,
            players: obstacles,
        }
        .trace(
            from,
            [0.0; 3],
            [0.0; 3],
            to,
            jkr_game_jka::use_key::USE_MASK,
        );
        // Nothing within reach, or the world itself.
        if trace.fraction == 1.0 || trace.entity_number >= 1_022 {
            return None;
        }
        Some(trace.entity_number)
    }
}

#[path = "bridge_emplaced.rs"]
pub(super) mod bridge_emplaced;
