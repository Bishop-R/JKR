//! Private duels (`Cmd_EngageDuel_f`, `g_cmds.c:2996-3143`, and `ClientThink_real`'s duel,
//! `g_active.c:2405-2528`): outside the team games and the duel game types, a player with
//! its saber in hand challenges the one it aims at within 256 units; the other, aiming
//! back while the challenge stands (five seconds), accepts. Both are holstered and held
//! still two seconds, then their sabers light and they fight each other alone — no one
//! else can hurt them or be hurt by them (`G_Damage`, `g_combat.c:4540-4556`; a missile at
//! a duellist is spent on it harmlessly, `G_MissileImpact`'s `killProj`), and between them
//! only the saber hurts. The duel ends when one dies (the other healed to its maximum and
//! protected as at a spawn, everyone told), when either leaves, or when they are 1024
//! units apart.
//!
//! jaPRO adds two kinds ([`DuelKind`]): the Force duel, in which every power may be used on
//! the opponent and anything but a mine's blast hurts, and the gun duel, fought with one
//! weapon (or all of them) and anything hurting; both start with full Force and every
//! timed power stopped, and end with jaPRO's own prints. A stock saber duel is untouched.
//!
//! The wire fields are the player state's own (`duelInProgress`, `duelIndex`,
//! `duelTime`); the movement freezes and arms a duellist from them.

pub use crate::duel_kind::DuelKind;
use crate::means_of_death::{MOD_DET_PACK_SPLASH, MOD_FALLING, MOD_TRIP_MINE_SPLASH};
use jkr_protocol::PlayerState;

/// `EV_PRIVATE_DUEL`: 1 accepted, 2 begun, 0 over.
pub const EV_PRIVATE_DUEL: u32 = 15;
const PS_WEAPON_TIME: usize = 10;
const PS_EFLAGS: usize = 17;
const PS_SPEED: usize = 12;
const PS_DUEL_INDEX: usize = 44;
const PS_WEAPON: usize = 47;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_SABER_HOLSTERED: usize = 81;
const PS_SABER_IN_FLIGHT: usize = 88;
const PS_DUEL_TIME: usize = 118;
const PS_DUEL_IN_PROGRESS: usize = 119;
const STAT_HEALTH: usize = 0;
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_WEAPONS: usize = 4;
const STAT_ARMOR: usize = 5;
const STAT_MAX_HEALTH: usize = 8;
/// `HI_JETPACK`, `PW_NEUTRALFLAG`, `HANDEXTEND_KNOCKDOWN`.
const HI_JETPACK: u32 = 7;
const PW_NEUTRALFLAG: usize = 6;
const HANDEXTEND_KNOCKDOWN: u32 = 8;
/// The armour jaPRO leaves a Force or gun duel's winner with.
const WINNER_ARMOR: u32 = 25;

/// jaPRO's duel options (`g_duelStartHealth`, `g_duelStartArmor`, `g_duelDistanceLimit`)
/// and stock's `g_spawnInvulnerability`; the defaults are stock's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuelRules {
    /// The duellists' health once a duel is accepted; 0 leaves it as it is.
    pub start_health: i32,
    /// Their armour then (only with a start health).
    pub start_armor: i32,
    /// A duel ends when the two are 1024 apart (stock; jaPRO's default lifts it).
    pub distance_limit: bool,
    /// `g_spawnInvulnerability`: the winner's protection in milliseconds; 0 for none.
    pub protection: i32,
}

impl Default for DuelRules {
    fn default() -> Self {
        Self {
            start_health: 0,
            start_armor: 0,
            distance_limit: true,
            protection: 3_000,
        }
    }
}
const WP_SABER: u32 = 3;
const HANDEXTEND_DUELCHALLENGE: u32 = 9;
const EF_INVULNERABLE: u32 = 1 << 27;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_TEAM: i32 = 6;
const MOD_SABER: u32 = 3;
const MAX_CLIENTS: u16 = 32;

/// Whether `state`'s player duels somebody other than player `other`.
pub fn elsewhere(state: &PlayerState, other: u16) -> bool {
    state.duel_in_progress() && state.duel_index() != other
}

/// `G_Damage`'s duel rule (`g_combat.c:4540-4556`): a duellist is hurt by nobody but its
/// opponent, and by it only with the saber; a duellist hurts nobody but its opponent, and
/// it only with the saber. In jaPRO's kinds (`g_combat.c:4850-4881` of jaPRO) a Force duel
/// bars only a mine's blast and a gun duel nothing, and a fall always hurts. `target` and
/// `attacker` are a client's number, state and duel kind.
pub fn damage_refused(
    (target, target_state, target_kind): (u16, &PlayerState, DuelKind),
    attacker: Option<(u16, &PlayerState, DuelKind)>,
    means: u32,
) -> bool {
    let Some((attacker, attacker_state, attacker_kind)) = attacker else {
        return false;
    };
    let japro =
        |state: &PlayerState, kind: DuelKind| state.duel_in_progress() && kind != DuelKind::Saber;
    if means == MOD_FALLING
        && (japro(target_state, target_kind) || japro(attacker_state, attacker_kind))
    {
        return false;
    }
    let mine = matches!(means, MOD_TRIP_MINE_SPLASH | MOD_DET_PACK_SPLASH);
    let barred = |saber_kind: DuelKind| {
        (means != MOD_SABER && saber_kind == DuelKind::Saber)
            || (mine && attacker_kind == DuelKind::Force)
    };
    if target_state.duel_in_progress()
        && (attacker != target_state.duel_index() || barred(attacker_kind))
    {
        return true;
    }
    attacker_state.duel_in_progress()
        && (target != attacker_state.duel_index() || barred(target_kind))
}

/// What jaPRO keeps of a duel for a Force duel's closing print (`pers.stats`): the damage
/// the duellist did its opponent and the lowest health it fell to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DuelRecord {
    pub damage_given: i32,
    pub lowest_health: i32,
}

/// A player as a duel reads and changes it.
pub struct Duellist<'a> {
    pub number: u16,
    pub state: &'a mut PlayerState,
    /// `gentity_t::health`.
    pub health: &'a mut i32,
    /// `pers.netname`, as messages print it.
    pub name: &'a [u8],
    /// `forceHandExtendTime` (the knockdown's memory).
    pub hand_extend_time: &'a mut i32,
    /// `invulnerableTimer`.
    pub invulnerable_until: &'a mut i32,
    /// `sess.sessionTeam`, `inuse`.
    pub team: i32,
    /// jaPRO's `dueltypes[n]`: the kind of its duel, or of its last challenge.
    pub kind: &'a mut DuelKind,
    /// The duel so far, for a Force duel's closing print.
    pub record: DuelRecord,
}

/// What a challenge or a duel's think asks the caller to do besides the states.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Told {
    /// Server commands: to a client, or to everyone (`None`).
    pub commands: Vec<(Option<u16>, Vec<u8>)>,
    /// Saber sounds (`G_Sound` on `CHAN_AUTO`) on a player: `true` its on-sound, `false`
    /// its off-sound — for each of its two sabers, the second counted as the game counts
    /// it.
    pub sounds: Vec<(u16, crate::generic_commands::SaberSound)>,
    /// The players an event was added to (`G_AddEvent`: `eventTime` is the caller's).
    pub raised: Vec<u16>,
    /// The players a jaPRO duel has begun for: their Force full, their rage's recovery
    /// over, and rage, protection, speed, mind trick, absorption and sight stopped.
    pub refilled: Vec<u16>,
    /// The players a duel of every weapon has ended for, armed again as at a spawn.
    pub rearmed: Vec<u16>,
}

/// `G_AddEvent(player, EV_PRIVATE_DUEL, parameter)`.
fn add_event(state: &mut PlayerState, parameter: u32) {
    crate::player_entity::add_event(state, EV_PRIVATE_DUEL, parameter);
}

/// The line `Cmd_EngageDuel_f` traces (`MASK_PLAYERSOLID`, from the origin): 256 units
/// along the view, to eye height.
pub fn challenge_line(state: &PlayerState) -> ([f32; 3], [f32; 3]) {
    let origin = state.origin();
    let forward = crate::pmove::flight::flight_axes(state.view_angles())
        .0
        .to_array();
    let view_height = state.raw_field(22).unwrap_or(0) as i32 as f32;
    let end = [
        origin[0] + forward[0] * 256.0,
        origin[1] + forward[1] * 256.0,
        (origin[2] + view_height) + forward[2] * 256.0,
    ];
    (origin, end)
}

/// Whether `Cmd_EngageDuel_f` goes as far as its trace for this player: private duels on,
/// a free-for-all, not holding off a challenge of its own, the saber in hand (for a saber
/// or Force duel) and not thrown, no duel already. jaPRO's kinds also refuse a spectator,
/// one knocked down or carrying the neutral flag, and the Force duel one with a jetpack.
pub fn may_challenge(
    state: &PlayerState,
    gametype: i32,
    private_duels: bool,
    level_time: i32,
    (kind, spectator): (DuelKind, bool),
) -> bool {
    let field = |index: usize| state.raw_field(index).unwrap_or(0);
    let stock = private_duels
        && gametype != GT_DUEL
        && gametype != GT_POWERDUEL
        && gametype < GT_TEAM
        && state.duel_time() < level_time
        && (!kind.needs_saber() || field(PS_WEAPON) == WP_SABER)
        && field(PS_SABER_IN_FLIGHT) == 0
        && !state.duel_in_progress();
    stock
        && (kind == DuelKind::Saber
            || !(spectator
                || field(PS_FORCE_HAND_EXTEND) == HANDEXTEND_KNOCKDOWN
                || state.powerups[PW_NEUTRALFLAG] != 0
                || (kind == DuelKind::Force
                    && state.stats[STAT_HOLDABLE_ITEMS] & (1 << HI_JETPACK) != 0)))
}

/// The rest of `Cmd_EngageDuel_f` for `me`, whose trace struck `other` (a client
/// number), with the duel key of `kind`: accepted if `other` challenged `me` to a duel
/// `kind` accepts and its challenge stands, else a challenge — either way `me` gestures
/// and names `other` for five seconds. Nothing for one dead, without its saber in hand
/// (for a saber or Force duel), duelling or with its saber thrown. Accepted under jaPRO's
/// `g_duelStartHealth`, both start with that health and `rules`' armour.
pub fn engage(
    me: &mut Duellist,
    other: &mut Duellist,
    gametype: i32,
    level_time: i32,
    (rules, kind): (DuelRules, DuelKind),
) -> Told {
    let mut told = Told::default();
    let field = |state: &PlayerState, index: usize| state.raw_field(index).unwrap_or(0);
    if other.number >= MAX_CLIENTS
        || *other.health < 1
        || (other.state.stats[STAT_HEALTH] as i32) < 1
        || (kind.needs_saber() && field(other.state, PS_WEAPON) != WP_SABER)
        || other.state.duel_in_progress()
        || field(other.state, PS_SABER_IN_FLIGHT) != 0
    {
        return told;
    }
    if gametype >= GT_TEAM && other.team == me.team {
        return told;
    }
    if other.state.duel_index() == me.number
        && other.state.duel_time() >= level_time
        && kind.accepts(*other.kind)
    {
        // A gun duel is fought with the challenger's weapon.
        let fought = if kind.needs_saber() {
            kind
        } else {
            *other.kind
        };
        *me.kind = fought;
        let text = match fought.result_label() {
            None => [
                b"print \"".as_slice(),
                other.name,
                b" @@@PLDUELACCEPT ",
                me.name,
                b"!\n\"",
            ]
            .concat(),
            Some(label) => [
                b"print \"".as_slice(),
                other.name,
                b"^7 @@@PLDUELACCEPT ",
                me.name,
                b"^7! ",
                label.as_bytes(),
                b"\n\"",
            ]
            .concat(),
        };
        told.commands.push((None, text));
        for state in [&mut *me.state, &mut *other.state] {
            state.set_raw_field(PS_DUEL_IN_PROGRESS, 1);
            state.set_raw_field(PS_DUEL_TIME, (level_time + 2_000) as u32);
        }
        for (number, state) in [
            (me.number, &mut *me.state),
            (other.number, &mut *other.state),
        ] {
            add_event(state, fought.accept_event());
            told.raised.push(number);
        }
        // Holstered until the duel begins, to light them then.
        for (number, state) in [
            (me.number, &mut *me.state),
            (other.number, &mut *other.state),
        ] {
            if field(state, PS_SABER_HOLSTERED) == 0 {
                // `saber[0].soundOff`; the second saber's only with a model.
                told.sounds.extend(
                    crate::generic_commands::SaberSound::both(false).map(|sound| (number, sound)),
                );
                state.set_raw_field(PS_WEAPON_TIME, 400);
                state.set_raw_field(PS_SABER_HOLSTERED, 2);
            }
        }
        if rules.start_health != 0 {
            // jaPRO weighs the health by the kind of the accepting key.
            let health_now = kind.start_health(rules.start_health);
            for (state, health) in [
                (&mut *me.state, &mut *me.health),
                (&mut *other.state, &mut *other.health),
            ] {
                state.stats[STAT_ARMOR] = rules.start_armor as u32;
                *health = health_now;
                state.stats[STAT_HEALTH] = health_now as u32;
            }
        }
        if fought != DuelKind::Saber {
            told.refilled.extend([me.number, other.number]);
        }
        let (every_weapon, ammo) = fought.arms();
        if let DuelKind::Gun(_) = fought {
            for (state, hand_extend_time) in [
                (&mut *me.state, &mut *me.hand_extend_time),
                (&mut *other.state, &mut *other.hand_extend_time),
            ] {
                if every_weapon {
                    state.stats[STAT_WEAPONS] |=
                        (1..=16).fold(0, |bits, weapon| bits | 1 << weapon);
                }
                for &(index, count) in &ammo {
                    state.ammo[index] = count;
                }
                // Two seconds with the weapon held still at the start.
                state.set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_DUELCHALLENGE);
                *hand_extend_time = level_time + 2_000;
            }
        }
    } else {
        *me.kind = kind;
        let label = kind.challenge_label();
        let (to_other, to_me) = match &label {
            None => (
                [b"cp \"".as_slice(), me.name, b" @@@PLDUELCHALLENGE\n\""].concat(),
                [b"cp \"@@@PLDUELCHALLENGED ".as_slice(), other.name, b"\n\""].concat(),
            ),
            Some(label) => (
                [
                    b"cp \"".as_slice(),
                    me.name,
                    b" ^7@@@PLDUELCHALLENGE\n",
                    label.as_bytes(),
                    b"\n\"",
                ]
                .concat(),
                [
                    b"cp \"@@@PLDUELCHALLENGED ".as_slice(),
                    other.name,
                    b"\n",
                    label.as_bytes(),
                    b"\n\"",
                ]
                .concat(),
            ),
        };
        told.commands.push((Some(other.number), to_other));
        told.commands.push((Some(me.number), to_me));
    }
    me.state
        .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_DUELCHALLENGE);
    // An accepted gun duel's two seconds stand.
    if !(me.state.duel_in_progress() && matches!(*me.kind, DuelKind::Gun(_))) {
        *me.hand_extend_time = level_time + 1_000;
    }
    me.state
        .set_raw_field(PS_DUEL_INDEX, u32::from(other.number));
    me.state
        .set_raw_field(PS_DUEL_TIME, (level_time + 5_000) as u32);
    told
}

/// Both duellists out of their duel, and told so (`EV_PRIVATE_DUEL` 0).
fn end_both(me: &mut Duellist, other: &mut Duellist, told: &mut Told) {
    for state in [&mut *me.state, &mut *other.state] {
        state.set_raw_field(PS_DUEL_IN_PROGRESS, 0);
    }
    for (number, state) in [
        (me.number, &mut *me.state),
        (other.number, &mut *other.state),
    ] {
        add_event(state, 0);
        told.raised.push(number);
    }
}

/// What the duel's think leaves of the command: held still while the duel has not begun.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Thought {
    /// `speed`, `basespeed` and the command's moves zeroed.
    pub frozen: bool,
}

/// `ClientThink_real`'s duel for `me`, duelling `against` (`None` for one gone): the
/// sabers lit once the wait is over; the duel ended when the other is gone or no longer
/// duels `me`, when it is dead (the winner healed and protected), or when they are 1024
/// units apart (unless jaPRO's `g_duelDistanceLimit 0` lifts that, `rules`).
pub fn think(
    me: &mut Duellist,
    against: Option<&mut Duellist>,
    level_time: i32,
    rules: &DuelRules,
    told: &mut Told,
) -> Thought {
    let mut thought = Thought::default();
    if !me.state.duel_in_progress() {
        return thought;
    }
    let field = |state: &PlayerState, index: usize| state.raw_field(index).unwrap_or(0);
    let mut against = against;
    let kind = *me.kind;
    if me.state.duel_time() < level_time {
        // Bring out the sabers: this player's, then the opponent's. jaPRO's Force duel
        // raises no start (its acceptance raised 2 already).
        let bring_out = |number: u16, state: &mut PlayerState, told: &mut Told| {
            if field(state, PS_WEAPON) == WP_SABER
                && field(state, PS_SABER_HOLSTERED) != 0
                && state.duel_time() != 0
            {
                state.set_raw_field(PS_SABER_HOLSTERED, 0);
                // Both sabers' on-sounds, the empty second's default one too.
                told.sounds.extend(
                    crate::generic_commands::SaberSound::both(true).map(|sound| (number, sound)),
                );
                if kind == DuelKind::Saber {
                    add_event(state, 2);
                    told.raised.push(number);
                }
                state.set_raw_field(PS_DUEL_TIME, 0);
            }
        };
        bring_out(me.number, me.state, told);
        if let Some(other) = against.as_deref_mut() {
            bring_out(other.number, other.state, told);
        }
    } else {
        me.state.set_raw_field(PS_SPEED, 0f32.to_bits());
        thought.frozen = true;
    }
    let Some(other) = against.filter(|other| other.state.duel_index() == me.number) else {
        me.state.set_raw_field(PS_DUEL_IN_PROGRESS, 0);
        add_event(me.state, 0);
        told.raised.push(me.number);
        return thought;
    };
    if *other.health < 1 || (other.state.stats[STAT_HEALTH] as i32) < 1 {
        end_both(me, other, told);
        let alive = *me.health > 0 && me.state.stats[STAT_HEALTH] as i32 > 0;
        if kind != DuelKind::Saber {
            japro_end(me, other, alive, (level_time, rules.protection), told);
            return thought;
        }
        if alive {
            let max = me.state.stats[STAT_MAX_HEALTH] as i32;
            if *me.health < max {
                *me.health = max;
                me.state.stats[STAT_HEALTH] = max as u32;
            }
            protect(me, level_time, rules.protection);
            told.commands.push((
                None,
                [
                    b"cp \"".as_slice(),
                    me.name,
                    b" @@@PLDUELWINNER ",
                    other.name,
                    b"!\n\"",
                ]
                .concat(),
            ));
        } else {
            // Both died in the same frame.
            told.commands
                .push((None, b"cp \"@@@PLDUELTIE\n\"".to_vec()));
        }
        return thought;
    }
    let (mine, theirs) = (me.state.origin(), other.state.origin());
    let apart = (0..3)
        .map(|axis| (mine[axis] - theirs[axis]) * (mine[axis] - theirs[axis]))
        .sum::<f32>()
        .sqrt();
    if rules.distance_limit && apart >= 1_024.0 {
        end_both(me, other, told);
        told.commands
            .push((None, b"print \"@@@PLDUELSTOP\n\"".to_vec()));
    }
    thought
}

/// The end of a Force or gun duel whose opponent died (`g_active.c:4455-4525` of jaPRO):
/// told everyone with the winner's health and armour — and in a Force duel its Force,
/// its share of the damage and the lowest it fell to — then a gun duel's ammunition
/// left (or every weapon given back to both), the winner healed to its maximum with 25
/// armour and protected as at a spawn. A tie names both, with a Force duel's Force and
/// damage.
fn japro_end(
    me: &mut Duellist,
    other: &Duellist,
    alive: bool,
    (level_time, protection): (i32, i32),
    told: &mut Told,
) {
    let kind = *me.kind;
    let label = kind.result_label().unwrap_or("");
    let (given, taken) = (me.record.damage_given, other.record.damage_given);
    let force = me.state.force_power();
    let stats = match (alive, kind) {
        (true, DuelKind::Force) => {
            let share = if given + taken != 0 {
                100 * given / (given + taken)
            } else {
                0
            };
            format!(
                "(^1{}^7/^2{}/^4{force}^7/^3{share}^7/^6{}^7) ",
                me.state.stats[STAT_HEALTH] as i32,
                me.state.stats[STAT_ARMOR] as i32,
                me.record.lowest_health
            )
        }
        (true, _) => format!(
            "(^1{}^7/^2{}^7) ",
            me.state.stats[STAT_HEALTH] as i32, me.state.stats[STAT_ARMOR] as i32
        ),
        (false, DuelKind::Force) => format!("(^4{force}^7/^5{given}^7/^3{taken}^7) "),
        (false, _) => String::new(),
    };
    let verdict: &[u8] = if alive {
        b"^7 @@@PLDUELWINNER "
    } else {
        b"^7 @@@PLDUELTIE "
    };
    told.commands.push((
        None,
        [
            b"print \"".as_slice(),
            me.name,
            verdict,
            other.name,
            b"^7! ",
            stats.as_bytes(),
            label.as_bytes(),
            b"\n\"",
        ]
        .concat(),
    ));
    if !alive {
        return;
    }
    if kind.every_weapon() {
        told.rearmed.extend([me.number, other.number]);
    } else if let Some((index, count)) = kind.winner_ammo() {
        me.state.ammo[index] = count;
    }
    let max = me.state.stats[STAT_MAX_HEALTH];
    *me.health = max as i32;
    me.state.stats[STAT_HEALTH] = max;
    me.state.stats[STAT_ARMOR] = WINNER_ARMOR;
    protect(me, level_time, protection);
}

/// The winner's `g_spawnInvulnerability` milliseconds of protection, none at 0.
fn protect(me: &mut Duellist, level_time: i32, protection: i32) {
    if protection != 0 {
        let flags = me.state.raw_field(PS_EFLAGS).unwrap_or(0);
        me.state.set_raw_field(PS_EFLAGS, flags | EF_INVULNERABLE);
        *me.invulnerable_until = level_time + protection;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two saber players, `b` having challenged `a` (number 0) until 5000.
    fn pair() -> (PlayerState, PlayerState) {
        let (mut a, mut b) = (PlayerState::default(), PlayerState::default());
        for state in [&mut a, &mut b] {
            state.set_raw_field(PS_WEAPON, WP_SABER);
            state.stats[STAT_HEALTH] = 100;
        }
        b.set_raw_field(PS_DUEL_INDEX, 0);
        b.set_raw_field(PS_DUEL_TIME, 5_000);
        (a, b)
    }

    #[test]
    fn an_accepted_duel_starts_with_japros_health_and_armour() {
        for (rules, expected) in [
            (DuelRules::default(), (100, 0)),
            (
                DuelRules {
                    start_health: 250,
                    start_armor: 50,
                    ..DuelRules::default()
                },
                (250, 50),
            ),
        ] {
            let (mut a, mut b) = pair();
            let (mut health_a, mut health_b) = (100, 100);
            let (mut hand_a, mut hand_b, mut until_a, mut until_b) = (0, 0, 0, 0);
            let (mut kind_a, mut kind_b) = (DuelKind::Saber, DuelKind::Saber);
            let mut me = Duellist {
                number: 0,
                state: &mut a,
                health: &mut health_a,
                name: b"a",
                hand_extend_time: &mut hand_a,
                invulnerable_until: &mut until_a,
                team: 0,
                kind: &mut kind_a,
                record: DuelRecord::default(),
            };
            let mut other = Duellist {
                number: 1,
                state: &mut b,
                health: &mut health_b,
                name: b"b",
                hand_extend_time: &mut hand_b,
                invulnerable_until: &mut until_b,
                team: 0,
                kind: &mut kind_b,
                record: DuelRecord::default(),
            };
            let _ = engage(&mut me, &mut other, 0, 1_000, (rules, DuelKind::Saber));
            assert!(a.duel_in_progress() && b.duel_in_progress());
            for (health, state) in [(health_a, &a), (health_b, &b)] {
                assert_eq!(
                    (
                        health,
                        state.stats[STAT_HEALTH] as i32,
                        state.stats[STAT_ARMOR] as i32
                    ),
                    (expected.0, expected.0, expected.1)
                );
            }
        }
    }

    /// A Force duel is accepted only with a Force duel key: the saber key challenges
    /// anew instead.
    #[test]
    fn a_force_duel_is_accepted_only_in_kind() {
        for (key, accepted) in [(DuelKind::Saber, false), (DuelKind::Force, true)] {
            let (mut a, mut b) = pair();
            let (mut health_a, mut health_b) = (100, 100);
            let (mut hand_a, mut hand_b, mut until_a, mut until_b) = (0, 0, 0, 0);
            let (mut kind_a, mut kind_b) = (DuelKind::Saber, DuelKind::Force);
            let mut me = Duellist {
                number: 0,
                state: &mut a,
                health: &mut health_a,
                name: b"a",
                hand_extend_time: &mut hand_a,
                invulnerable_until: &mut until_a,
                team: 0,
                kind: &mut kind_a,
                record: DuelRecord::default(),
            };
            let mut other = Duellist {
                number: 1,
                state: &mut b,
                health: &mut health_b,
                name: b"b",
                hand_extend_time: &mut hand_b,
                invulnerable_until: &mut until_b,
                team: 0,
                kind: &mut kind_b,
                record: DuelRecord::default(),
            };
            let told = engage(&mut me, &mut other, 0, 1_000, (DuelRules::default(), key));
            assert_eq!(a.duel_in_progress(), accepted);
            assert_eq!(kind_a, key);
            if accepted {
                assert_eq!(told.refilled, [0, 1]);
                assert!(told.commands[0].1.ends_with(b"! (Force)\n\""));
            }
        }
    }

    /// Between Force duellists anything but a mine's blast hurts; between saber
    /// duellists only the saber; a fall always hurts a Force duellist.
    #[test]
    fn a_force_duel_allows_more_than_the_saber() {
        let (mut a, mut b) = (PlayerState::default(), PlayerState::default());
        for (state, other) in [(&mut a, 1), (&mut b, 0)] {
            state.set_raw_field(PS_DUEL_IN_PROGRESS, 1);
            state.set_raw_field(PS_DUEL_INDEX, other);
        }
        let refused =
            |kind: DuelKind, means: u32| damage_refused((0, &a, kind), Some((1, &b, kind)), means);
        // `MOD_FORCE_DARK`, `MOD_FALLING`, `MOD_TRIP_MINE_SPLASH`.
        assert!(refused(DuelKind::Saber, 31));
        assert!(!refused(DuelKind::Saber, MOD_SABER));
        assert!(!refused(DuelKind::Force, 31));
        assert!(!refused(DuelKind::Force, MOD_FALLING));
        assert!(refused(DuelKind::Force, MOD_TRIP_MINE_SPLASH));
        assert!(!refused(DuelKind::Gun(13), MOD_TRIP_MINE_SPLASH));
        // A third party is refused whatever the kind.
        assert!(damage_refused(
            (0, &a, DuelKind::Force),
            Some((2, &PlayerState::default(), DuelKind::Saber)),
            31
        ));
    }
}
