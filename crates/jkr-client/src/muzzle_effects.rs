//! BaseJKA third-person muzzle-effect selection and impulse lifetime.
//!
//! The weapon table mirrors `CG_RegisterWeapon` in
//! `codemp/cgame/cg_weaponinit.c:173-611`. `CG_FireWeapon` records the event
//! time (`cg_weapons.c:1831-1848`), while `CG_AddPlayerWeapon` replays the
//! selected EFX graph every rendered frame (`cg_weapons.c:688-762`).

use jkr_protocol::{MAX_LEGACY_ENTITIES, Snapshot};

const MAX_CLIENTS: usize = 32; // codemp/qcommon/q_shared.h:890
const WP_DEMP2: u8 = 9; // codemp/game/bg_weapons.h:41
const WP_NUM_WEAPONS: usize = 19; // codemp/game/bg_weapons.h:30-60
const EF_FIRING: u32 = 1 << 9; // codemp/game/bg_public.h:648
const EF_ALT_FIRING: u32 = 1 << 10; // codemp/game/bg_public.h:649
const EV_FIRE_WEAPON: u16 = 27; // codemp/game/bg_public.h:844
const EV_ALT_FIRE: u16 = 28; // codemp/game/bg_public.h:845
const EVENT_VALUE_MASK: u16 = 0xff; // codemp/game/bg_public.h:783-790
const ET_EVENTS: u8 = 18; // codemp/game/bg_public.h:1264
const EF_PLAYER_EVENT: u32 = 1 << 5; // codemp/game/bg_public.h:636
const MUZZLE_FLASH_TIME: i32 = 20; // codemp/cgame/cg_local.h:61

/// Primary and alternate EFX graph registered for one legacy weapon number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyMuzzleEffectPair {
    pub primary: Option<&'static str>,
    pub alternate: Option<&'static str>,
}

const NONE: LegacyMuzzleEffectPair = LegacyMuzzleEffectPair {
    primary: None,
    alternate: None,
};

/// `weapon_t`-ordered muzzle registrations from `CG_RegisterWeapon`.
///
/// Registration rows are concussion `:173-190`, Bryar/old Bryar `:206-224`,
/// blaster/emplaced `:248-266`, disruptor `:281-298`, bowcaster `:330-347`,
/// repeater `:363-380`, DEMP2 `:396-413`, flechette `:431-448`, and rocket
/// `:462-480` in `codemp/cgame/cg_weaponinit.c`.
pub const LEGACY_MUZZLE_EFFECTS: [LegacyMuzzleEffectPair; WP_NUM_WEAPONS] = [
    NONE, // WP_NONE
    NONE, // WP_STUN_BATON
    NONE, // WP_MELEE
    NONE, // WP_SABER
    pair("bryar/muzzle_flash", "bryar/muzzle_flash"),
    pair("blaster/muzzle_flash", "blaster/muzzle_flash"),
    pair("disruptor/muzzle_flash", "disruptor/muzzle_flash"),
    // cg_weaponinit.c:330-353 assigns the alt fields before the primary fields.
    pair("bowcaster/muzzle_flash", "bowcaster/muzzle_flash"),
    pair("repeater/muzzle_flash", "repeater/muzzle_flash"),
    pair("demp2/muzzle_flash", "demp2/muzzle_flash"),
    pair("flechette/muzzle_flash", "flechette/muzzle_flash"),
    pair("rocket/muzzle_flash", "rocket/altmuzzle_flash"),
    NONE, // WP_THERMAL
    NONE, // WP_TRIP_MINE
    NONE, // WP_DET_PACK
    pair("concussion/muzzle_flash", "concussion/altmuzzle_flash"),
    pair("bryar/muzzle_flash", "bryar/muzzle_flash"),
    pair("blaster/muzzle_flash", "blaster/muzzle_flash"),
    NONE, // WP_TURRET
];

const fn pair(primary: &'static str, alternate: &'static str) -> LegacyMuzzleEffectPair {
    LegacyMuzzleEffectPair {
        primary: Some(primary),
        alternate: Some(alternate),
    }
}

/// Return the registered effect for the current weapon and firing eFlag.
pub fn legacy_muzzle_effect(weapon: u8, alternate: bool) -> Option<&'static str> {
    let pair = LEGACY_MUZZLE_EFFECTS.get(usize::from(weapon))?;
    if alternate {
        pair.alternate
    } else {
        pair.primary
    }
}

fn muzzle_effect_for_flags(weapon: u8, flags: u32) -> Option<(bool, &'static str)> {
    let alternate = flags & EF_ALT_FIRING != 0;
    legacy_muzzle_effect(weapon, alternate).map(|effect| (alternate, effect))
}

/// Whether `CG_AddPlayerWeapon` reaches its EFX dispatch at this age.
///
/// The ordinary impulse returns after 20 ms. DEMP2 with `EF_FIRING` bypasses
/// that return, but the inner `MUZZLE_FLASH_TIME + 10` test still caps a
/// dispatch at 30 ms (`codemp/cgame/cg_weapons.c:688-733`).
pub fn legacy_muzzle_window(weapon: u8, flags: u32, age_millis: i32) -> bool {
    if age_millis < 0 || age_millis > MUZZLE_FLASH_TIME + 10 {
        return false;
    }
    let continuous = weapon == WP_DEMP2 && flags & EF_FIRING != 0;
    continuous || age_millis <= MUZZLE_FLASH_TIME
}

/// One per-rendered-frame request for a complete retail muzzle EFX graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyMuzzleEffectRequest {
    pub client_num: u16,
    pub weapon: u8,
    pub alternate: bool,
    pub effect_name: &'static str,
    pub age_millis: i32,
}

#[derive(Clone, Copy, Debug)]
struct EventState {
    fired_at: Option<i32>,
}

impl EventState {
    const EMPTY: Self = Self { fired_at: None };
}

#[derive(Clone, Copy, Debug)]
struct ClientState {
    weapon: u8,
    flags: u32,
}

/// Fixed-capacity equivalent of each `centity_t::muzzleFlashTime`.
///
/// Snapshot receipt records fire-event time. [`update`](Self::update) then
/// derives frame requests from current weapon/eFlags, so an alternate fire
/// event never overrides the authoritative `EF_ALT_FIRING` selection.
pub struct LegacyMuzzleEffects {
    events: [EventState; MAX_CLIENTS],
    previous_entity_event: [u16; MAX_LEGACY_ENTITIES],
    seen_entity_epoch: [u32; MAX_LEGACY_ENTITIES],
    entity_epoch: u32,
    player_event_sequence: i32,
    requests: [Option<LegacyMuzzleEffectRequest>; MAX_CLIENTS],
    request_count: usize,
}

impl Default for LegacyMuzzleEffects {
    fn default() -> Self {
        Self::new()
    }
}

impl LegacyMuzzleEffects {
    /// Create empty fixed storage; no update grows heap memory.
    pub const fn new() -> Self {
        Self {
            events: [EventState::EMPTY; MAX_CLIENTS],
            previous_entity_event: [0; MAX_LEGACY_ENTITIES],
            seen_entity_epoch: [0; MAX_LEGACY_ENTITIES],
            entity_epoch: 0,
            player_event_sequence: 0,
            requests: [None; MAX_CLIENTS],
            request_count: 0,
        }
    }

    /// Consume new `EV_FIRE_WEAPON`/`EV_ALT_FIRE` occurrences at a snapshot.
    pub fn observe(&mut self, snapshot: &Snapshot) -> usize {
        let mut observed = 0;
        self.entity_epoch = self.entity_epoch.wrapping_add(1).max(1);
        for entity in &snapshot.entities {
            let entity_number = usize::from(entity.number());
            if entity_number >= MAX_LEGACY_ENTITIES {
                continue;
            }
            self.seen_entity_epoch[entity_number] = self.entity_epoch;
            let event_entity = entity.entity_type() >= ET_EVENTS;
            let signature = if event_entity {
                u16::from(entity.entity_type())
            } else {
                entity.event()
            };
            if self.previous_entity_event[entity_number] == signature {
                continue;
            }
            self.previous_entity_event[entity_number] = signature;
            let event = if event_entity {
                u16::from(entity.entity_type() - ET_EVENTS)
            } else {
                signature & EVENT_VALUE_MASK
            };
            if matches!(event, EV_FIRE_WEAPON | EV_ALT_FIRE) {
                let client = if event_entity && entity.e_flags() & EF_PLAYER_EVENT != 0 {
                    usize::from(entity.other_entity_num())
                } else {
                    usize::from(entity.client_num())
                };
                if client >= MAX_CLIENTS {
                    continue;
                }
                self.events[client].fired_at = Some(snapshot.server_time);
                observed += 1;
            }
        }
        for entity_number in 0..MAX_LEGACY_ENTITIES {
            if self.seen_entity_epoch[entity_number] != self.entity_epoch {
                self.previous_entity_event[entity_number] = 0;
            }
        }
        let sequence = snapshot.player.event_sequence();
        for event_sequence in self.player_event_sequence.max(sequence - 2)..sequence {
            let slot = usize::try_from(event_sequence & 1).unwrap_or(0);
            let Some(event) = snapshot.player.event(slot) else {
                continue;
            };
            if matches!(event & EVENT_VALUE_MASK, EV_FIRE_WEAPON | EV_ALT_FIRE) {
                let client = usize::from(snapshot.player.client_num());
                if client < MAX_CLIENTS {
                    self.events[client].fired_at = Some(snapshot.server_time);
                    observed += 1;
                }
            }
        }
        self.player_event_sequence = sequence;
        observed
    }

    /// Rebuild per-frame play requests from current authoritative state.
    pub fn update(&mut self, snapshot: &Snapshot, presentation_time: i32) {
        self.requests.fill(None);
        self.request_count = 0;
        let mut clients = [None; MAX_CLIENTS];
        for entity in &snapshot.entities {
            if matches!(entity.entity_type(), 1 | 13) {
                let client = usize::from(entity.client_num());
                if client < MAX_CLIENTS {
                    clients[client] = Some(ClientState {
                        weapon: entity.weapon(),
                        flags: entity.e_flags(),
                    });
                }
            }
        }
        let local = usize::from(snapshot.player.client_num());
        if local < MAX_CLIENTS {
            clients[local] = Some(ClientState {
                weapon: snapshot.player.weapon(),
                flags: snapshot.player.entity_flags(),
            });
        }
        for (client, state) in clients.into_iter().enumerate() {
            let Some(state) = state else { continue };
            let Some(fired_at) = self.events[client].fired_at else {
                continue;
            };
            let age_millis = presentation_time.wrapping_sub(fired_at);
            if !legacy_muzzle_window(state.weapon, state.flags, age_millis) {
                continue;
            }
            let Some((alternate, effect_name)) = muzzle_effect_for_flags(state.weapon, state.flags)
            else {
                continue;
            };
            self.requests[self.request_count] = Some(LegacyMuzzleEffectRequest {
                client_num: client as u16,
                weapon: state.weapon,
                alternate,
                effect_name,
                age_millis,
            });
            self.request_count += 1;
        }
    }

    /// Iterate active requests without allocating.
    pub fn requests(&self) -> impl Iterator<Item = LegacyMuzzleEffectRequest> + '_ {
        self.requests[..self.request_count]
            .iter()
            .flatten()
            .copied()
    }

    /// Find the active request associated with one rendered client actor.
    pub fn request(&self, client_num: u16) -> Option<LegacyMuzzleEffectRequest> {
        self.requests()
            .find(|request| request.client_num == client_num)
    }
}
