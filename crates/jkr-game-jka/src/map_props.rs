//! `misc_model_breakable` as multiplayer has it (`SP_misc_model_breakable`,
//! `misc_model_breakable_init`, `misc_model_breakable_gravity_init`, `g_misc.c:307-470`):
//! a model the map places for every client to draw, solid to movement with spawnflag 1
//! (or, given health, to shots only), dropped to the floor when it has `gravity`.
//! Multiplayer gives it nothing more — "TODO: fix health/dying funcs", "TODO: fix using" —
//! so whatever its keys say, it is never broken, used or repaired.

use jkr_entity::Entity;

/// `CONTENTS_SOLID`, `CONTENTS_MONSTERCLIP`, `CONTENTS_BOTCLIP`, `CONTENTS_SHOTCLIP`,
/// `CONTENTS_BODY`, `CONTENTS_OPAQUE`, `CONTENTS_TERRAIN` (`surfaceflags.h`).
const CONTENTS_SOLID: u32 = 0x1;
const CONTENTS_MONSTERCLIP: u32 = 0x20;
const CONTENTS_BOTCLIP: u32 = 0x40;
const CONTENTS_SHOTCLIP: u32 = 0x80;
const CONTENTS_BODY: u32 = 0x100;
const CONTENTS_OPAQUE: u32 = 0x8000;
const CONTENTS_TERRAIN: u32 = 0x1000;
/// A solid prop's contents ("Was CONTENTS_SOLID, but only architecture should be this").
pub const SOLID_CONTENTS: u32 =
    CONTENTS_SOLID | CONTENTS_OPAQUE | CONTENTS_BODY | CONTENTS_MONSTERCLIP | CONTENTS_BOTCLIP;
/// A prop with health but no solid flag, which only shots meet.
pub const SHOTCLIP_CONTENTS: u32 = CONTENTS_SHOTCLIP;
/// `MASK_NPCSOLID`: what a falling prop lands on.
pub const MASK_NPCSOLID: u32 =
    CONTENTS_SOLID | CONTENTS_MONSTERCLIP | CONTENTS_BODY | CONTENTS_TERRAIN;
/// `MIN_WORLD_COORD`: how far down a falling prop looks for the floor.
pub const MIN_WORLD_COORD: f32 = -64.0 * 1024.0;
/// Spawnflag 128, `PLAYER_USE`: `SVF_PLAYER_USABLE`, though nothing answers the use.
const PLAYER_USE: u32 = 128;

/// One placed prop.
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    /// `model`: what clients draw (`s.modelindex`).
    pub model: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    /// `r.mins`, `r.maxs`: the map's `mins` and `maxs`, sixteen units each way unset.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `r.contents`: [`SOLID_CONTENTS`], [`SHOTCLIP_CONTENTS`] or nothing.
    pub contents: u32,
    /// `gravity`: dropped to the floor once, as it spawns.
    pub falls: bool,
    /// `SVF_PLAYER_USABLE`.
    pub player_usable: bool,
}

/// Why the reference stops the map over a prop (`ERR_DROP`); this server refuses the
/// prop alone.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// "no model set on misc_model_breakable at (x y z)".
    NoModel { origin: [f32; 3] },
}

impl Prop {
    /// `SP_misc_model_breakable` for `entity`, or `None` when it is something else.
    pub fn spawn(entity: &Entity) -> Option<Result<Self, Refused>> {
        if !entity
            .classname()?
            .eq_ignore_ascii_case("misc_model_breakable")
        {
            return None;
        }
        let integer = |key: &str| {
            entity
                .get(key)
                .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()))
        };
        let origin = crate::fx_runner::vector(entity, "origin");
        let Some(model) = entity.get("model").filter(|model| !model.is_empty()) else {
            return Some(Err(Refused::NoModel { origin }));
        };
        let spawnflags = integer("spawnflags") as u32;
        let contents = if spawnflags & 1 != 0 {
            SOLID_CONTENTS
        } else if integer("health") != 0 {
            SHOTCLIP_CONTENTS
        } else {
            0
        };
        let mut mins = crate::fx_runner::vector(entity, "mins");
        let mut maxs = crate::fx_runner::vector(entity, "maxs");
        if mins == [0.0; 3] {
            mins = [-16.0; 3];
        }
        if maxs == [0.0; 3] {
            maxs = [16.0; 3];
        }
        let gravity = entity
            .get("gravity")
            .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes()));
        Some(Ok(Self {
            model: model.to_owned(),
            origin,
            angles: crate::fx_runner::spawn_angles(entity),
            bounds: (mins, maxs),
            contents,
            falls: gravity != 0.0,
            player_usable: spawnflags & PLAYER_USE != 0,
        }))
    }

    /// `misc_model_breakable_gravity_init`'s drop: from a unit above where it stands down
    /// to [`MIN_WORLD_COORD`], the box traced against [`MASK_NPCSOLID`]. `trace` gives
    /// (all solid, start solid, fraction, end); a clear trace that hits moves the prop.
    pub fn fall(&mut self, trace: impl FnOnce([f32; 3], [f32; 3]) -> (bool, bool, f32, [f32; 3])) {
        if !self.falls {
            return;
        }
        let mut top = self.origin;
        top[2] += 1.0;
        let bottom = [self.origin[0], self.origin[1], MIN_WORLD_COORD];
        let (all_solid, start_solid, fraction, end) = trace(top, bottom);
        if !all_solid && !start_solid && fraction < 1.0 {
            self.origin = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(pairs: &[(&str, &str)]) -> Entity {
        let mut text = String::from("{\n");
        for (key, value) in pairs {
            text.push_str(&format!("\"{key}\" \"{value}\"\n"));
        }
        text.push('}');
        jkr_entity::parse_entity_lump(text.as_bytes())
            .unwrap()
            .remove(0)
    }

    /// A solid prop keeps its box; one with health but no solid flag stops shots only; a
    /// bare one is only drawn. Without `mins`/`maxs` the box is 32 units.
    #[test]
    fn a_prop_is_solid_as_its_flags_and_health_say() {
        let solid = entity(&[
            ("classname", "misc_model_breakable"),
            ("model", "models/map_objects/crate.md3"),
            ("spawnflags", "1"),
            ("origin", "8 16 24"),
        ]);
        let prop = Prop::spawn(&solid).unwrap().unwrap();
        assert_eq!(prop.contents, SOLID_CONTENTS);
        assert_eq!(prop.bounds, ([-16.0; 3], [16.0; 3]));
        assert_eq!(prop.origin, [8.0, 16.0, 24.0]);
        let shot = entity(&[
            ("classname", "misc_model_breakable"),
            ("model", "a.md3"),
            ("health", "40"),
            ("mins", "-4 -4 0"),
            ("maxs", "4 4 32"),
        ]);
        let prop = Prop::spawn(&shot).unwrap().unwrap();
        assert_eq!(prop.contents, SHOTCLIP_CONTENTS);
        assert_eq!(prop.bounds, ([-4.0, -4.0, 0.0], [4.0, 4.0, 32.0]));
        let bare = entity(&[("classname", "misc_model_breakable"), ("model", "a.md3")]);
        assert_eq!(Prop::spawn(&bare).unwrap().unwrap().contents, 0);
        let none = entity(&[("classname", "misc_model_breakable")]);
        assert!(matches!(
            Prop::spawn(&none),
            Some(Err(Refused::NoModel { .. }))
        ));
    }

    /// `gravity`: dropped to where its box lands; a trace starting in solid leaves it.
    #[test]
    fn a_falling_prop_lands() {
        let falling = entity(&[
            ("classname", "misc_model_breakable"),
            ("model", "a.md3"),
            ("gravity", "1"),
            ("origin", "0 0 100"),
        ]);
        let mut prop = Prop::spawn(&falling).unwrap().unwrap();
        prop.fall(|top, bottom| {
            assert_eq!(top, [0.0, 0.0, 101.0]);
            assert_eq!(bottom[2], MIN_WORLD_COORD);
            (false, false, 0.5, [0.0, 0.0, 16.0])
        });
        assert_eq!(prop.origin, [0.0, 0.0, 16.0]);
        prop.fall(|_, _| (false, true, 0.0, [0.0; 3]));
        assert_eq!(prop.origin, [0.0, 0.0, 16.0]);
    }
}
