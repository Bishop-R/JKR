//! `func_usable` (`SP_func_usable`, `func_usable_use`, `func_wait_return_solid`,
//! `func_usable_think`; OpenJK `codemp/game/g_mover.c:3012-3215`): a brush a map turns on
//! and off. Used while it is there, it goes away — no contents, not sent — and fires its
//! `target`; used again it comes back solid and fires its `target2`, with spawnflag 16
//! only once no body stands in its box. An `ALWAYS_ON` one only fires its `target`, and
//! then answers no use until `wait` seconds have passed; one with an `endframe` steps
//! its shader a frame instead. One with `health` is used by every blow
//! (`G_Damage`'s mover branch), its health never touched.

use crate::use_key::{FL_INACTIVE, SVF_PLAYER_USABLE};
use jkr_entity::Entity;

/// `START_OFF` (1): not there until used.
const START_OFF: u32 = 1;
/// `ALWAYS_ON` (8): a use fires its targets and it never goes away.
pub const ALWAYS_ON: u32 = 8;
/// Spawnflag 16: it comes back only once no body stands in its box.
const RETURN_WHEN_CLEAR: u32 = 16;
/// `MOVER_PLAYER_USE` (64) and `MOVER_INACTIVE` (128), which `InitMover` reads.
const PLAYER_USE: u32 = 64;
const INACTIVE: u32 = 128;
/// `SVF_NOCLIENT`.
pub const SVF_NOCLIENT: u32 = 0x1;
/// `EF_SHADER_ANIM`, `EF_NODRAW`.
pub const EF_SHADER_ANIM: u32 = 1 << 4;
const EF_NODRAW: u32 = 1 << 8;
/// The contents of a brush that is there. `SV_SetBrushModel` gives -1 and lets the
/// brushes' own contents decide; this server's brush entities say solid, the brushes
/// still deciding what each trace meets.
const CONTENTS_SOLID: u32 = 1;
/// `FRAMETIME`: how soon a brush waiting for its box to clear looks again.
const FRAMETIME: i32 = 100;
/// `GT_SIEGE`.
const GT_SIEGE: i32 = 7;

/// One placed `func_usable`.
#[derive(Clone, Debug, PartialEq)]
pub struct Usable {
    /// The inline model it is made of, and that model's box (`r.mins`, `r.maxs`).
    pub model: usize,
    pub bounds: ([f32; 3], [f32; 3]),
    /// `s.origin`: where an origin brush put it; the world's origin otherwise.
    pub origin: [f32; 3],
    /// `model2`, drawn with the brush (`s.modelindex2`); multiplayer draws no `.glm`.
    pub model2: Option<String>,
    /// `InitMover`'s `s.constantLight`, from `light` and `color`.
    pub constant_light: Option<u32>,
    /// Fired when it goes away, steps a frame or is used always-on.
    pub target: String,
    /// Fired when it comes back.
    pub target2: String,
    pub targetname: String,
    pub spawnflags: u32,
    /// `alliedTeam` (`teamuser`): in Siege, the team whose use key it does not answer.
    pub team_user: i32,
    /// `count`: whether it is there, or on its way back. One that starts off is not.
    pub present: bool,
    /// `r.contents`: nothing while it is away.
    pub contents: u32,
    /// `r.svFlags`: `SVF_PLAYER_USABLE` with `PLAYER_USE`, `SVF_NOCLIENT` while away.
    pub svflags: u32,
    /// `s.eFlags`: `EF_NODRAW` while away, `EF_SHADER_ANIM` with an `endframe`.
    pub eflags: u32,
    /// `wait`, in seconds: how long an `ALWAYS_ON` one answers no use after one.
    pub wait: f32,
    /// `takedamage`: given `health`, a blow uses it.
    pub takes_damage: bool,
    /// `genericValue5` (`endframe`): the last frame of its shader.
    pub end_frame: i32,
    /// `s.frame`: the frame its shader shows.
    pub frame: i32,
    /// `use`: an `ALWAYS_ON` one has none until its wait is over.
    pub has_use: bool,
    /// `FL_INACTIVE` (`target_deactivate`, or the `INACTIVE` spawnflag).
    pub inactive: bool,
    /// What it will do next, and when (`think`, `nextthink`).
    pub think: Option<(Think, i32)>,
}

/// A `func_usable`'s `think`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Think {
    /// `func_usable_think`: an `ALWAYS_ON` one answers uses again.
    Usable,
    /// `func_wait_return_solid`: it looks again whether its box is clear.
    ReturnSolid,
}

/// What a use, a blow or a think came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Used {
    /// Nothing changed that fires anything.
    Nothing,
    /// Its `target` fires: it stepped a frame, was used always-on, or went away.
    Target,
    /// It came back solid, and its `target2` fires.
    Returned,
    /// It is on its way back, waiting for its box to clear.
    Waiting,
}

/// `SP_func_usable` for `entity`, or `None` when it is something else or names no inline
/// model `bounds_of` knows.
pub fn spawn(
    entity: &Entity,
    bounds_of: impl FnOnce(usize) -> Option<([f32; 3], [f32; 3])>,
) -> Option<Usable> {
    if !entity.classname()?.eq_ignore_ascii_case("func_usable") {
        return None;
    }
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|index| index.parse().ok())?;
    let bounds = bounds_of(model)?;
    let integer = |key: &str| {
        entity
            .get(key)
            .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()))
    };
    let spawnflags = integer("spawnflags") as u32;
    let end_frame = integer("endframe");
    let model2 = entity
        .get("model2")
        .filter(|name| !name.is_empty() && !name.contains(".glm"))
        .map(str::to_owned);
    let team_user = entity
        .get("teamuser")
        .or_else(|| entity.get("alliedteam"))
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
    let mut usable = Usable {
        model,
        bounds,
        origin: crate::fx_runner::vector(entity, "origin"),
        model2,
        constant_light: constant_light(entity),
        target: entity.get("target").unwrap_or_default().to_owned(),
        target2: entity.get("target2").unwrap_or_default().to_owned(),
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        spawnflags,
        team_user,
        present: true,
        contents: CONTENTS_SOLID,
        svflags: 0,
        eflags: 0,
        wait: entity
            .get("wait")
            .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes())),
        takes_damage: integer("health") != 0,
        end_frame,
        frame: 0,
        has_use: true,
        inactive: false,
        think: None,
    };
    usable.init_mover();
    if spawnflags & START_OFF != 0 {
        usable.go_away();
    }
    if end_frame > 0 {
        usable.eflags |= EF_SHADER_ANIM;
    }
    Some(usable)
}

/// `InitMover`'s `s.constantLight`: `color` (each part times 255, at most 255) and `light`
/// (a quarter of it, at most 255), when the map set either — `light` 100 and white
/// otherwise.
pub fn constant_light(entity: &Entity) -> Option<u32> {
    let light = entity.get("light");
    let color = entity.get("color");
    if light.is_none() && color.is_none() {
        return None;
    }
    let light = light.map_or(100.0, |value| crate::text_parse::atof(value.as_bytes()));
    let color = match color {
        Some(_) => crate::fx_runner::vector(entity, "color"),
        None => [1.0; 3],
    };
    let [r, g, b] = color.map(|part| ((part * 255.0) as i32).min(255));
    let intensity = ((light / 4.0) as i32).min(255);
    Some((r | (g << 8) | (b << 16) | (intensity << 24)) as u32)
}

impl Usable {
    /// `GlobalUse` and `func_usable_use`: nothing for an inactive one or one with no use;
    /// else a shader steps a frame, an `ALWAYS_ON` one rests for its `wait`, one that is
    /// there goes away and one that is not comes back — at once, or with spawnflag 16
    /// once `clear` says no body stands in its box.
    pub fn used(&mut self, level_time: i32, clear: bool) -> Used {
        if self.inactive || !self.has_use {
            return Used::Nothing;
        }
        if self.eflags & EF_SHADER_ANIM != 0 {
            self.frame += 1;
            if self.frame > self.end_frame {
                self.frame = 0;
            }
            return Used::Target;
        }
        if self.spawnflags & ALWAYS_ON != 0 {
            self.svflags &= !SVF_PLAYER_USABLE;
            self.has_use = false;
            if self.wait != 0.0 {
                let at = (level_time as f32 + self.wait * 1_000.0) as i32;
                self.think = Some((Think::Usable, at));
            }
            return Used::Target;
        }
        if !self.present {
            self.present = true;
            return self.return_solid(level_time, clear);
        }
        self.go_away();
        self.think = None;
        Used::Target
    }

    /// `G_Damage` on it: a blow on one with health is a use (`GlobalUse`), its health
    /// left as it was.
    pub fn struck(&mut self, level_time: i32, clear: bool) -> Used {
        if !self.takes_damage {
            return Used::Nothing;
        }
        self.used(level_time, clear)
    }

    /// Whether its think is due at `level_time` (`G_RunThink`).
    pub fn think_due(&self, level_time: i32) -> bool {
        self.think.is_some_and(|(_, at)| at > 0 && at <= level_time)
    }

    /// `G_RunThink`: a due think runs once. `clear` as for [`Self::used`].
    pub fn think(&mut self, level_time: i32, clear: bool) -> Used {
        if !self.think_due(level_time) {
            return Used::Nothing;
        }
        let Some((think, _)) = self.think.take() else {
            return Used::Nothing;
        };
        match think {
            Think::Usable => {
                // `func_usable_think`: usable by the key again — even one the map never
                // let players use.
                if self.spawnflags & ALWAYS_ON != 0 {
                    self.svflags |= SVF_PLAYER_USABLE;
                    self.has_use = true;
                }
                Used::Nothing
            }
            Think::ReturnSolid => self.return_solid(level_time, clear),
        }
    }

    /// Whether coming back waits for its box to clear, so a caller needs to look.
    pub fn waits_for_clear(&self) -> bool {
        self.spawnflags & RETURN_WHEN_CLEAR != 0
    }

    /// Whether it is waiting to come back, its box to be looked at when its think is due.
    pub fn returning(&self) -> bool {
        matches!(self.think, Some((Think::ReturnSolid, _)))
    }

    /// `ValidUseTarget` and `TryUse`'s Siege rule: a player of team `team` may use it by
    /// key — it has a use, is active and usable by players, and in Siege is not its
    /// `teamuser`'s, unless `g_ff_objectives` is on.
    pub fn answers_use_key(&self, gametype: i32, team: i32, ff_objectives: bool) -> bool {
        let flags = if self.inactive { FL_INACTIVE } else { 0 };
        crate::use_key::answers_use(self.has_use, flags, self.svflags)
            && (gametype != GT_SIEGE
                || self.team_user == 0
                || self.team_user != team
                || ff_objectives)
    }

    /// Whether clients are sent it (`SVF_NOCLIENT` clear).
    pub fn sent(&self) -> bool {
        self.svflags & SVF_NOCLIENT == 0
    }

    /// `s.time`: the frame count a shader-animated one tells clients.
    pub fn shader_frames(&self) -> i32 {
        if self.end_frame > 0 {
            self.end_frame + 1
        } else {
            0
        }
    }

    /// `func_wait_return_solid`: with its box clear (or not waiting on it), solid and
    /// drawn again as `InitMover` leaves a mover; else a look again a frame later.
    fn return_solid(&mut self, level_time: i32, clear: bool) -> Used {
        if self.waits_for_clear() && !clear {
            self.think = Some((Think::ReturnSolid, level_time + FRAMETIME));
            return Used::Waiting;
        }
        self.contents = CONTENTS_SOLID;
        self.init_mover();
        self.eflags &= !EF_NODRAW;
        self.has_use = true;
        Used::Returned
    }

    /// `InitMover`'s flags: `r.svFlags` set afresh — usable by players with
    /// `PLAYER_USE` — and `FL_INACTIVE` added with `INACTIVE`.
    fn init_mover(&mut self) {
        self.svflags = if self.spawnflags & PLAYER_USE != 0 {
            SVF_PLAYER_USABLE
        } else {
            0
        };
        if self.spawnflags & INACTIVE != 0 {
            self.inactive = true;
        }
    }

    /// Gone: no contents, not sent, not drawn.
    fn go_away(&mut self) {
        self.contents = 0;
        self.svflags |= SVF_NOCLIENT;
        self.eflags |= EF_NODRAW;
        self.present = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(pairs: &[(&str, &str)]) -> Entity {
        let mut text = String::from("{\n\"classname\" \"func_usable\"\n\"model\" \"*3\"\n");
        for (key, value) in pairs {
            text.push_str(&format!("\"{key}\" \"{value}\"\n"));
        }
        text.push('}');
        jkr_entity::parse_entity_lump(text.as_bytes())
            .unwrap()
            .remove(0)
    }

    fn usable(pairs: &[(&str, &str)]) -> Usable {
        spawn(&entity(pairs), |model| {
            assert_eq!(model, 3);
            Some(([-8.0; 3], [8.0; 3]))
        })
        .unwrap()
    }

    /// A wall: used, it goes away and fires its target; used again it comes back and
    /// fires its second. Only `PLAYER_USE` lets the key reach it.
    #[test]
    fn a_wall_goes_away_and_comes_back() {
        let mut wall = usable(&[("target", "a"), ("target2", "b")]);
        assert_eq!((wall.contents, wall.sent()), (CONTENTS_SOLID, true));
        assert!(!wall.answers_use_key(0, 1, false));
        assert_eq!(wall.used(1_000, false), Used::Target);
        assert_eq!(
            (wall.contents, wall.sent(), wall.present),
            (0, false, false)
        );
        assert_eq!(wall.eflags & EF_NODRAW, EF_NODRAW);
        assert_eq!(wall.used(2_000, false), Used::Returned);
        assert_eq!((wall.contents, wall.sent()), (CONTENTS_SOLID, true));
        assert_eq!(wall.eflags & EF_NODRAW, 0);
        let keyed = usable(&[("spawnflags", "64")]);
        assert!(keyed.answers_use_key(0, 1, false));
    }

    /// `START_OFF`: away from the start, back on the first use.
    #[test]
    fn one_that_starts_off_comes_on_when_used() {
        let mut wall = usable(&[("spawnflags", "1")]);
        assert_eq!(
            (wall.contents, wall.sent(), wall.present),
            (0, false, false)
        );
        assert_eq!(wall.used(500, true), Used::Returned);
        assert_eq!(wall.contents, CONTENTS_SOLID);
    }

    /// Spawnflag 16: while a body stands in its box it waits, looking again a frame
    /// later; used meanwhile it goes away again.
    #[test]
    fn coming_back_waits_for_a_clear_box() {
        let mut wall = usable(&[("spawnflags", "17")]);
        assert_eq!(wall.used(1_000, false), Used::Waiting);
        assert!(wall.present && wall.returning());
        assert_eq!(wall.contents, 0);
        assert!(!wall.think_due(1_050));
        assert_eq!(wall.think(1_100, false), Used::Waiting);
        assert_eq!(wall.think(1_200, true), Used::Returned);
        assert_eq!(wall.contents, CONTENTS_SOLID);
        let mut again = usable(&[("spawnflags", "17")]);
        assert_eq!(again.used(1_000, false), Used::Waiting);
        assert_eq!(again.used(1_050, false), Used::Target);
        assert!(again.think.is_none() && !again.present);
    }

    /// `ALWAYS_ON`: fires and then answers nothing until its wait is over, when even the
    /// use key reaches it.
    #[test]
    fn an_always_on_one_rests_for_its_wait() {
        let mut console = usable(&[("spawnflags", "72"), ("wait", "3"), ("target", "cam")]);
        assert!(console.answers_use_key(0, 0, false));
        assert_eq!(console.used(10_000, false), Used::Target);
        assert!(!console.answers_use_key(0, 0, false));
        assert_eq!(console.used(11_000, false), Used::Nothing);
        assert_eq!(console.contents, CONTENTS_SOLID);
        assert_eq!(console.think(12_999, false), Used::Nothing);
        assert_eq!(console.think(13_000, false), Used::Nothing);
        assert!(console.answers_use_key(0, 0, false));
        let mut no_wait = usable(&[("spawnflags", "8")]);
        assert_eq!(no_wait.used(0, false), Used::Target);
        assert_eq!(no_wait.used(100_000, false), Used::Nothing);
    }

    /// `endframe`: each use steps the shader a frame, round to the first after the last.
    #[test]
    fn a_shader_steps_through_its_frames() {
        let mut panel = usable(&[("endframe", "2"), ("target", "t")]);
        assert_eq!(panel.eflags & EF_SHADER_ANIM, EF_SHADER_ANIM);
        assert_eq!(panel.shader_frames(), 3);
        let frames: Vec<i32> = (0..4)
            .map(|_| {
                assert_eq!(panel.used(0, false), Used::Target);
                panel.frame
            })
            .collect();
        assert_eq!(frames, [1, 2, 0, 1]);
        assert_eq!(panel.contents, CONTENTS_SOLID);
    }

    /// `INACTIVE`, Siege's `teamuser`, `health`, `model2`, `origin` and `light`.
    #[test]
    fn its_keys_say_who_may_use_it_and_how_it_looks() {
        let mut off = usable(&[("spawnflags", "192")]);
        assert!(off.inactive);
        assert_eq!(off.used(0, false), Used::Nothing);
        let team = usable(&[("spawnflags", "64"), ("teamuser", "1")]);
        assert!(!team.answers_use_key(GT_SIEGE, 1, false));
        assert!(team.answers_use_key(GT_SIEGE, 2, false));
        assert!(team.answers_use_key(GT_SIEGE, 1, true));
        assert!(team.answers_use_key(0, 1, false));
        let mut shot = usable(&[("health", "5")]);
        assert_eq!(shot.struck(0, false), Used::Target);
        assert_eq!(usable(&[]).struck(0, false), Used::Nothing);
        let dish = usable(&[
            ("model2", "models/map_objects/imperial/dish.md3"),
            ("origin", "2704 1248 40"),
        ]);
        assert_eq!(
            dish.model2.as_deref(),
            Some("models/map_objects/imperial/dish.md3")
        );
        assert_eq!(dish.origin, [2704.0, 1248.0, 40.0]);
        assert_eq!(usable(&[("model2", "a.glm")]).model2, None);
        assert_eq!(usable(&[]).constant_light, None);
        let lit = usable(&[("light", "200"), ("color", "1 0.5 2")]);
        assert_eq!(
            lit.constant_light,
            Some(255 | (127 << 8) | (255 << 16) | (50 << 24))
        );
    }
}
