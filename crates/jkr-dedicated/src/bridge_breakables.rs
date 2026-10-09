//! The breakable brushes on this server ([`jkr_game_jka::breakables`]): spawned as
//! `ET_MOVER`s of their inline models, hurt through `G_Damage`'s gates, broken into their
//! debris and explosions, used by what targets them and — in Siege — mended by the class
//! the map names.

use super::*;
use jkr_game_jka::map_turret_world::publish_net_health;

/// `MOD_UNKNOWN`: a breakable's own blast.
const MOD_UNKNOWN: u32 = 0;

impl NativeGame {
    /// The breakable brushes the map placed, each an `ET_MOVER` standing where the map
    /// drew it — which is all a client needs to draw the brush and to watch it go.
    pub(super) fn spawn_breakables(&mut self) {
        let Some(map) = self.map.take() else { return };
        for brush in &map.breakables {
            let mut state = EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS);
            state.set_raw_field(ES_ENTITY_TYPE, jkr_game_jka::breakables::ET_MOVER);
            jkr_game_jka::triggers::set_brush_model(&mut state, brush.model);
            // `G_ScaleNetHealth`: a `showhealth` brush carries what a HUD draws a bar from —
            // a hundredth of it from a thousand up, as the wire's fields hold ten bits.
            if brush.max_health > 0 {
                publish_net_health(&mut state, brush.health, brush.max_health);
            }
            // `G_SpawnGEntityFromSpawnVars` precaches a `healingsound`.
            if let Some(sound) = &brush.healing.sound {
                self.sound_index_for(sound);
            }
            if let Some(number) = self.pool.spawn_entity(state, 0) {
                self.pool.set_bounds(number, brush.bounds);
                self.breakables.push((number, brush.clone()));
            }
        }
        self.map = Some(map);
    }

    /// A blow that landed on a pane of glass, a usable, a shootable door or button or a
    /// breakable brush (`G_Damage` for a non-client entity): a shot's, a blast's or a
    /// swing's. Returns whether it was
    /// one of ours at all, so the caller can fall back to a player.
    pub(super) fn hurt_brush(
        &mut self,
        number: u16,
        damage: i32,
        means: u32,
        owner: u16,
        level_time: i32,
    ) -> bool {
        let ours = self.is_glass(number)
            || self
                .breakables
                .iter()
                .any(|(ours, _)| ours.legacy_number() == number)
            || self.stock.usables.position(number).is_some()
            || self.shootable_mover(number).is_some();
        // jaPRO's god chat: a typing player's blow strikes, but deals nothing; and nothing
        // is hurt before a Siege round begins.
        let begun = self.siege.as_ref().is_some_and(|siege| siege.round.begun);
        if !ours
            || self.typing_player(owner)
            || super::bridge_client_impact::round_waiting(self.gametype, begun)
        {
            return ours;
        }
        // A blast's blow, dealt while the map is set aside for its lines, waits for the
        // blast to be over ([`Self::flush_brush_blows`]), as an NPC's does.
        if self.map.is_none() {
            self.brush_blows
                .push((number, damage, means, owner, level_time));
            return true;
        }
        if self.hurt_glass(number, damage, means, owner, level_time)
            || self.strike_usable(number, usize::from(owner), level_time)
            || self.shoot_mover(number, usize::from(owner), level_time)
        {
            return true;
        }
        let from = self
            .peer_mut(usize::from(owner))
            .map(|peer| peer.state.origin());
        self.hurt_breakable_from(number, damage, means, owner, from, level_time)
    }

    /// The blows a blast dealt brushes while the map was set aside, dealt now in order.
    pub(super) fn flush_brush_blows(&mut self) {
        for (number, damage, means, owner, time) in std::mem::take(&mut self.brush_blows) {
            self.hurt_brush(number, damage, means, owner, time);
        }
    }

    /// `G_Damage` on the breakable brush `number` by `owner`, standing at `from` (what its
    /// chunks are thrown from; `None` for no player). `false` when `number` is none.
    pub(super) fn hurt_breakable_from(
        &mut self,
        number: u16,
        damage: i32,
        means: u32,
        owner: u16,
        from: Option<[f32; 3]>,
        level_time: i32,
    ) -> bool {
        let Some(index) = self
            .breakables
            .iter()
            .position(|(ours, _)| ours.legacy_number() == number)
        else {
            return false;
        };
        // `G_Damage`'s gates for a thing that is no client: the Siege round, Siege's
        // `teamnodmg` (a side's own objective), the attacker's handicap, at least one.
        let blow = jkr_game_jka::map_turret_world::ObjectBlow {
            attacker: self.blow_attacker(owner),
            damage,
            flags: 0,
            means,
            siege: self.gametype == GAMETYPE_SIEGE,
            siege_round_begun: self.siege.as_ref().is_some_and(|siege| siege.round.begun),
            friendly_fire_objectives: self.integer_cvar(b"g_ff_objectives", 0) != 0,
        };
        let brush = &self.breakables[index].1;
        let Some(damage) = jkr_game_jka::map_turret_world::object_take(
            number,
            brush.takes_damage,
            brush.team_no_damage,
            &blow,
        ) else {
            return true;
        };
        let hit = jkr_game_jka::breakables::hurt(
            &mut self.breakables[index].1,
            damage,
            means,
            false,
            from,
            level_time,
        );
        if matches!(hit, jkr_game_jka::breakables::Hit::Hurt { .. }) {
            self.tally_damage_by(Some(owner), means, damage);
        }
        match hit {
            jkr_game_jka::breakables::Hit::Ignored => {}
            jkr_game_jka::breakables::Hit::Hurt { paintarget, .. } => {
                self.publish_breakable(index);
                if let Some(target) = paintarget {
                    self.fire_targets(&target, usize::from(owner), level_time);
                }
            }
            // The delay is the map's own; without one it comes apart in the same frame.
            jkr_game_jka::breakables::Hit::Broken { at } if at > level_time => {
                self.breakables[index].1.enemy = Some(owner);
                self.publish_breakable(index)
            }
            jkr_game_jka::breakables::Hit::Broken { .. } => {
                self.breakables[index].1.enemy = Some(owner);
                self.break_brush(index, from, level_time)
            }
        }
        true
    }

    /// `funcBBrushUse` for the breakable brushes called `name`, used for `client`: each
    /// comes apart (after its delay) or, marked so, only fires its targets.
    pub(super) fn use_breakables(&mut self, name: &str, client: usize, level_time: i32) {
        let mut index = 0;
        while index < self.breakables.len() {
            let named = self.breakables[index]
                .1
                .targetname
                .eq_ignore_ascii_case(name);
            if !named || self.use_breakable(index, client, level_time) {
                index += 1;
            }
        }
    }

    /// `funcBBrushUse` of breakable `index` for `client`: it comes apart (after its delay)
    /// or, marked so, only fires its targets. Whether it is still in the list.
    fn use_breakable(&mut self, index: usize, client: usize, level_time: i32) -> bool {
        let activator = u16::try_from(client).ok();
        let brush = &mut self.breakables[index].1;
        match jkr_game_jka::breakables::used(brush, level_time, activator) {
            Some(target) => {
                self.fire_targets(&target, client, level_time);
                true
            }
            None if brush.thinking == jkr_game_jka::breakables::Thinking::Breaking => true,
            None => {
                let from = self.breaker_origin(index);
                self.break_brush(index, from, level_time);
                false
            }
        }
    }

    /// `TryUse` reaching breakable entity `number`: one with `PLAYER_USE` is used
    /// (`ValidUseTarget`, `funcBBrushUse`), its user held in the use pose; any other is
    /// mended by a player of the Siege class the map names (`TryHeal`). Whether either
    /// happened.
    pub(super) fn use_breakable_by_key(&mut self, client: usize, number: u16, time: i32) -> bool {
        let Some(index) = self
            .breakables
            .iter()
            .position(|(id, _)| id.legacy_number() == number)
        else {
            return false;
        };
        if self.breakables[index].1.svflags & jkr_game_jka::breakables::SVF_PLAYER_USABLE != 0 {
            if let Some(peer) = self.peer_mut(client) {
                super::bridge_use::hold_use_pose(peer, true);
            }
            self.use_breakable(index, client, time);
            return true;
        }
        self.repair_breakable(client, index, time)
    }

    /// `TryHeal` on breakable `index` by `client`: ten points every `healingrate`, its
    /// bar told, its `healingsound` played on the player (the brush is a `SOLID_BMODEL`),
    /// and the player held in the use pose while it is the class's to mend. Whether it is.
    fn repair_breakable(&mut self, client: usize, index: usize, time: i32) -> bool {
        let class = self.healer_class(client);
        let gametype = self.gametype;
        let brush = &mut self.breakables[index].1;
        let (mut health, max) = (brush.health, brush.max_health);
        let healed = jkr_game_jka::try_heal::try_heal(
            &mut brush.healing,
            &mut health,
            max,
            (!class.is_empty()).then_some(class.as_bytes()),
            gametype,
            time,
        );
        let jkr_game_jka::try_heal::Heal::Yes { repaired } = healed else {
            return false;
        };
        brush.health = health;
        let sound = brush.healing.sound.clone().filter(|_| repaired);
        self.publish_breakable(index);
        if let Some(sound) = sound
            && let Some(origin) = self.peer(client).map(|peer| peer.state.origin())
        {
            self.healing_sound(&sound, origin, time);
        }
        if let Some(peer) = self.peer_mut(client) {
            super::bridge_repairs::hold_healer_pose(peer);
        }
        true
    }

    /// The breakable brushes whose delay has run out come apart (`funcBBrushDieGo`).
    pub(super) fn run_breakables(&mut self, level_time: i32) {
        let mut index = 0;
        while index < self.breakables.len() {
            if jkr_game_jka::breakables::breaking_due(&self.breakables[index].1, level_time) {
                let from = self.breaker_origin(index);
                self.break_brush(index, from, level_time);
            } else {
                index += 1;
            }
        }
    }

    /// Where the player who broke breakable `index` stands, its debris thrown from there.
    pub(super) fn breaker_origin(&mut self, index: usize) -> Option<[f32; 3]> {
        let enemy = self.breakables[index].1.enemy?;
        self.peer_mut(usize::from(enemy))
            .map(|peer| peer.state.origin())
    }

    /// `funcBBrushDieGo` on the server: the brush leaves the world, fires what it targets,
    /// throws its fireball and its debris as the two event entities a client draws them
    /// from, hurts whoever is close, and is freed.
    pub(super) fn break_brush(&mut self, index: usize, from: Option<[f32; 3]>, level_time: i32) {
        let (id, number) = (
            self.breakables[index].0,
            self.breakables[index].0.legacy_number(),
        );
        // `Q_flrand(0.0f, 1.0f)`, which decides how many chunks there are.
        let drawn = self.deaths.rng.flrand(0.0, 1.0);
        let broken = jkr_game_jka::breakables::break_apart(
            &mut self.breakables[index].1,
            from,
            number,
            drawn,
            level_time,
        );
        // For what broke it, and only when something did (`attacker != NULL`).
        if let (Some(target), Some(enemy)) = (&broken.target, self.breakables[index].1.enemy) {
            self.fire_targets(target, usize::from(enemy), level_time);
        }
        if let Some(explosion) = &broken.explosion {
            self.raise_event(
                jkr_game_jka::breakables::explosion_fields(explosion),
                level_time,
            );
        }
        self.raise_event(
            jkr_game_jka::breakables::debris_fields(&broken.debris),
            level_time,
        );
        if let Some((damage, radius, at)) = broken.splash {
            self.splash_from_brush(number, damage, radius, at, level_time);
        }
        // What it fired, or its own blast, may have broken others before it (or, used
        // again, itself): it is found again by its entity.
        let Some(index) = self.breakables.iter().position(|(ours, _)| *ours == id) else {
            return;
        };
        // The portal it stood in opens (`funcBBrushDieGo`, `g_mover.c:2514`).
        let bounds = self.breakables[index].1.bounds;
        self.adjust_portal([0.0; 3], bounds, true);
        self.pool.free(id, level_time);
        self.breakables.remove(index);
    }

    /// One event entity, built from the wire fields the reference crams its debris and
    /// its explosions into.
    pub(super) fn raise_event(&mut self, fields: Vec<(usize, u32)>, level_time: i32) {
        let mut state = EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS);
        for (index, bits) in fields {
            state.set_raw_field(index, bits);
        }
        let _ = self.pool.spawn_temporary(state, level_time, None);
    }

    /// `funcBBrushDieGo`'s `G_RadiusDamage`: from the middle of its box, whatever takes
    /// damage within `radius` — players, NPCs, turrets, shields, bodies, other brushes —
    /// the brush itself spared and as the attacker (so a Siege objective's blast spares
    /// the objectives of its own side), `MOD_UNKNOWN`.
    pub(super) fn splash_from_brush(
        &mut self,
        number: u16,
        damage: i32,
        radius: i32,
        at: [f32; 3],
        level_time: i32,
    ) {
        let (damage, radius) = (damage as f32, radius as f32);
        self.blast_from(number, (damage, radius), at, MOD_UNKNOWN, level_time);
    }

    /// `G_RadiusDamage(at, self, damage, radius, self, NULL, means)` for entity `number`
    /// that is no client: whatever takes damage within `radius` of `at` but itself, by the
    /// distance from its box and where a line reaches it, the entity the attacker.
    pub(super) fn blast_from(
        &mut self,
        number: u16,
        (damage, radius): (f32, f32),
        at: [f32; 3],
        means: u32,
        level_time: i32,
    ) {
        let attacker = Attacker {
            npc: false,
            client: number,
            max_health: 100,
            team: 0,
            saber_knockback: [0.0; 4],
        };
        self.gather_splash_targets();
        let targets = std::mem::take(&mut self.splash_targets);
        // The map is set aside while the blast's lines are traced through it.
        let map = self.map.take();
        let mut hurt = |target: u16, request: DamageRequest| {
            self.strike(usize::from(number), usize::from(target), request, false)
                .1
        };
        match &map {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                radius_damage(
                    at,
                    Some(attacker),
                    damage,
                    radius,
                    Some(number),
                    means,
                    level_time,
                    &targets,
                    &world,
                    &mut hurt,
                );
            }
            None => {
                radius_damage(
                    at,
                    Some(attacker),
                    damage,
                    radius,
                    Some(number),
                    means,
                    level_time,
                    &targets,
                    &Void,
                    &mut hurt,
                );
            }
        }
        self.map = map;
        self.flush_npc_blows();
        self.splash_targets = targets;
    }

    /// What a client is told of a brush that is still standing: the health bar, if the
    /// map asked for one.
    pub(super) fn publish_breakable(&mut self, index: usize) {
        let (number, brush) = &self.breakables[index];
        let (number, health, max_health) = (*number, brush.health, brush.max_health);
        if max_health <= 0 {
            return;
        }
        let Some(slot) = self.pool.state_mut(number) else {
            return;
        };
        publish_net_health(slot, health, max_health);
    }
}

/// The breakable brushes a blast may reach (`G_RadiusDamage`: whatever takes damage in its
/// box), each by `SV_LinkEntity`'s box a unit wider.
pub(super) fn breakable_splash_targets(
    breakables: &[(EntityId, jkr_game_jka::breakables::Breakable)],
) -> impl Iterator<Item = SplashTarget> + '_ {
    breakables
        .iter()
        .filter(|(_, brush)| brush.takes_damage)
        .map(|(id, brush)| SplashTarget {
            number: id.legacy_number(),
            bounds: (
                brush.bounds.0.map(|low| low - 1.0),
                brush.bounds.1.map(|high| high + 1.0),
            ),
            origin: [0.0; 3],
            takes_damage: true,
        })
}
