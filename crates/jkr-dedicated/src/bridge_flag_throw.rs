//! jaPRO's flag options on this server: throwing ([`jkr_game_jka::flag_throw`];
//! `g_allowFlagThrow`, `g_fixFlagSuicide`), `throwflag`, the thrower's second before
//! touching a flag again, a flag that comes to rest in a death pit going home, thrown
//! flags knocked and punted; and the carriers' air drag (`g_flagDrag`) and the flags'
//! wider pickup box (`g_fixFlagHitbox`), which the cvar hook keeps. All off, as in jaPRO.

use super::bridge_cheats::{ALIVE, NO_INTERMISSION};
use super::*;
use jkr_game_jka::flag_throw::{self, FlagRules};

const ALLOW_FLAG_THROW: &[u8] = b"g_allowFlagThrow";
const FIX_FLAG_SUICIDE: &[u8] = b"g_fixFlagSuicide";
const FLAG_DRAG: &[u8] = b"g_flagDrag";
const FIX_FLAG_HITBOX: &[u8] = b"g_fixFlagHitbox";
/// `GT_CTF`: the only game `throwflag` works in here (jaPRO's neutral-flag modes are
/// not ported).
const GT_CTF: i32 = 8;
/// `CONTENTS_TRIGGER`: what the death-pit check meets.
const CONTENTS_TRIGGER: u32 = 0x400;

impl NativeGame {
    /// The flag options, archived and announced, off (jaPRO's).
    pub(super) fn register_flag_options(&mut self) {
        let options: [(&[u8], &[u8]); 4] = [
            (
                ALLOW_FLAG_THROW,
                b"jaPRO: CTF carriers may throw the flag (throwflag)",
            ),
            (
                FIX_FLAG_SUICIDE,
                b"jaPRO: a suicide drops the flag instead of sending it home",
            ),
            (FLAG_DRAG, b"jaPRO: a flag carrier's drag in the air"),
            (FIX_FLAG_HITBOX, b"jaPRO: flags are taken from further away"),
        ];
        for (name, about) in options {
            self.cvars
                .get_announced(name, b"0", crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }

    /// The flag options as they stand.
    pub(super) fn flag_rules(&self) -> FlagRules {
        FlagRules {
            throw: self.cvars.integer(ALLOW_FLAG_THROW) != 0,
            suicide_drops: self.cvars.integer(FIX_FLAG_SUICIDE) != 0,
        }
    }

    /// `throwflag` from `client` (`Cmd_Throwflag_f`, `CMD_ALIVE|CMD_NOINTERMISSION`):
    /// whether `command` was it. In CTF with throwing on, the flag carried is thrown, and
    /// its status is `FLAG_DROPPED` (`Team_CheckDroppedItem`); otherwise nothing happens.
    pub(super) fn flag_throw_command(
        &mut self,
        client: usize,
        command: &[u8],
        level_time: i32,
    ) -> bool {
        if !command.eq_ignore_ascii_case(b"throwflag") {
            return false;
        }
        if !self.passes_gates(client, command, NO_INTERMISSION | ALIVE)
            || self.gametype != GT_CTF
            || !self.flag_rules().throw
        {
            return true;
        }
        let Some(peer) = self.peer_mut(client) else {
            return true;
        };
        let Some(thrown) = flag_throw::throw_flag(&mut peer.state, peer.entity.state(), level_time)
        else {
            return true;
        };
        peer.last_flag_throw = level_time;
        peer.movement = peer.movement.reseeded(&peer.state);
        let item = thrown.item;
        if let Some(number) = self.pool.spawn_entity(thrown.state.clone(), level_time) {
            self.pool.set_bounds(number, thrown.bounds);
            self.items.push((number, thrown));
            self.flag_launched(item, level_time);
        }
        true
    }

    /// `Touch_Item` under throwing: whether `client` threw a flag too recently to take
    /// one with `tag`.
    pub(super) fn flag_locked_out(&self, client: usize, tag: i32, level_time: i32) -> bool {
        self.peer(client).is_some_and(|peer| {
            flag_throw::locked_out(self.flag_rules(), tag, peer.last_flag_throw, level_time)
        })
    }

    /// `G_BounceItem`'s overboard check for flags that came to rest this frame
    /// (`landed`): one inside a death pit's trigger (`trigger_hurt` with `dmg -1`) goes
    /// home at the next frame instead of in thirty seconds. jaPRO checks every dropped
    /// flag; here only while throwing is on, so a stock game keeps stock's wait.
    pub(super) fn flags_in_death_pits(&mut self, landed: &[EntityId], level_time: i32) {
        if landed.is_empty() || !self.flag_rules().throw {
            return;
        }
        let Some(map) = self.map.as_ref() else {
            return;
        };
        for (number, pickup) in &mut self.items {
            if !landed.contains(number) || !in_death_pit(map, &self.triggers, pickup) {
                continue;
            }
            if let Some(dropped) = pickup.dropped.as_mut() {
                dropped.free_at = level_time;
            }
        }
    }
}

/// Whether `pickup`'s box overlaps a linked death-pit trigger's brushes (the reference's
/// trace against `CONTENTS_TRIGGER`, from where it rests to the same place).
fn in_death_pit(
    map: &LoadedMap,
    triggers: &[(EntityId, jkr_game_jka::triggers::Hurt)],
    pickup: &Pickup,
) -> bool {
    let half = (pickup.bounds.1[0] - pickup.bounds.0[0]) / 2.0;
    let Ok(bounds) = jkr_bsp::Aabb::new([-half; 3], [half; 3]) else {
        return false;
    };
    triggers.iter().any(|(_, trigger)| {
        trigger.linked
            && trigger.damage == -1
            && map
                .bsp
                .trace_model_box(
                    trigger.model,
                    pickup.origin,
                    pickup.origin,
                    bounds,
                    CONTENTS_TRIGGER,
                )
                .start_solid
    })
}

/// `EntitiesInBox`' dropped items for a Force push or pull under flag throwing: those not
/// taken whose box, grown by the unit `SV_LinkEntity` adds, meets `mins`..`maxs`.
pub(super) fn dropped_item_candidates(
    items: &[(EntityId, Pickup)],
    mins: [f32; 3],
    maxs: [f32; 3],
    out: &mut Vec<jkr_game_jka::force_throw::Candidate>,
) {
    for (number, pickup) in items {
        if pickup.dropped.is_none_or(|dropped| dropped.taken) {
            continue;
        }
        let (absmin, absmax) = jkr_game_jka::entity_clip::linked_box(pickup.origin, pickup.bounds);
        if super::bridge_force::overlaps(absmin, absmax, mins, maxs) {
            out.push(jkr_game_jka::force_throw::Candidate {
                number: number.legacy_number(),
                kind: jkr_game_jka::force_throw::CandidateKind::DroppedItem,
                absmin,
                absmax,
                origin: jkr_game_jka::items::trajectory_base(pickup),
            });
        }
    }
}

/// A Force push or pull on dropped item `item`: it flies from where it is at `velocity`.
pub(super) fn push_dropped_item(
    items: &mut [(EntityId, Pickup)],
    pool: &mut EntityPool,
    item: u16,
    velocity: [f32; 3],
    level_time: i32,
) {
    if let Some((number, pickup)) = items
        .iter_mut()
        .find(|(number, _)| number.legacy_number() == item)
    {
        jkr_game_jka::dropped_items::kick(pickup, velocity, false, level_time);
        pool.set_state(*number, &pickup.state);
    }
}

impl NativeGame {
    /// `G_RadiusDamage`'s knock on the thrown flags (jaPRO, with flag throwing on): a
    /// blast of `damage` within `radius` of `origin` adds its push to each one it reaches
    /// ([`flag_throw::blast_knock`]), which flies on from where it is.
    pub(super) fn blast_thrown_flags(
        &mut self,
        origin: [f32; 3],
        damage: f32,
        radius: f32,
        level_time: i32,
    ) {
        if !self.flag_rules().throw {
            return;
        }
        let knockback = self.cvars.value(b"g_knockback");
        let Self {
            items, pool, map, ..
        } = self;
        for (number, pickup) in items.iter_mut() {
            if pickup.contents & jkr_game_jka::corpses::CONTENTS_CORPSE == 0 {
                continue;
            }
            let target = jkr_game_jka::damage::SplashTarget {
                number: number.legacy_number(),
                bounds: jkr_game_jka::entity_clip::linked_box(pickup.origin, pickup.bounds),
                origin: pickup.origin,
                takes_damage: false,
            };
            let knock = match map.as_ref() {
                Some(map) => {
                    let world = WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    };
                    flag_throw::blast_knock(&target, origin, damage, radius, knockback, &world)
                }
                None => flag_throw::blast_knock(&target, origin, damage, radius, knockback, &Void),
            };
            if let Some(knock) = knock {
                jkr_game_jka::dropped_items::kick(pickup, knock, true, level_time);
                pool.set_state(*number, &pickup.state);
            }
        }
    }

    /// jaPRO's flag punting (`G_MissileImpact`): `missile` struck item `struck` (a thrown
    /// flag, the only item shots meet); a rocket, a concussion bolt, a thermal or a
    /// repeater's blob sends it on along its flight.
    pub(super) fn punt_flag(&mut self, struck: u16, missile: &Missile, level_time: i32) {
        if !self.flag_rules().throw {
            return;
        }
        let read = |index: usize| missile.state.raw_field(index).unwrap_or(0);
        // `BG_EvaluateTrajectoryDelta(&ent->s.pos, level.time, velocity)`.
        let velocity = jkr_game_jka::trajectory::legacy_evaluate_trajectory_delta(
            ES_POS_DELTA.map(|index| f32::from_bits(read(index))),
            read(ES_POS_TYPE) as u8,
            read(ES_POS_TIME) as i32,
            read(ES_POS_DURATION) as i32,
            level_time,
        );
        let alt = read(ES_EFLAGS) & EF_ALT_FIRING != 0;
        let Some(punt) = flag_throw::punt(read(ES_WEAPON), alt, velocity) else {
            return;
        };
        let Self { items, pool, .. } = self;
        if let Some((number, pickup)) = items
            .iter_mut()
            .find(|(number, _)| number.legacy_number() == struck)
        {
            jkr_game_jka::dropped_items::kick(pickup, punt, true, level_time);
            pool.set_state(*number, &pickup.state);
        }
    }
}

/// A missile's `s.pos` (`trTime`, `trDelta`, `trDuration`, `trType`), `s.weapon`,
/// `s.eFlags`, and `EF_ALT_FIRING`.
const ES_POS_TIME: usize = 0;
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_POS_DURATION: usize = 20;
const ES_POS_TYPE: usize = 23;
const ES_WEAPON: usize = 14;
const ES_EFLAGS: usize = 19;
const EF_ALT_FIRING: u32 = 1 << 10;
