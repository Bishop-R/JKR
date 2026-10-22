//! Material classes: the one table that decides how strong a texture's normals
//! are, whether it gets parallax height, and its roughness, metalness and
//! occlusion. Tune generation here.
//!
//! A texture's class comes from, in order:
//!
//! 1. the map compiler's material id in the BSP shader lump (`surfaceFlags &
//!    MATERIAL_MASK`, written from the shader's `material`/`q3map_material`
//!    keyword; ids from OpenJK `codemp/game/surfaceflags.h`);
//! 2. the first class, in table order, with a keyword contained in the
//!    texture's path below `textures/` (directories and file name, lower case);
//! 3. `surfaceparm metalsteps` (`SURF_METALSTEPS`) for metal;
//! 4. [`GENERIC`].

/// One row of [`CLASSES`]. All values are in the units the generator writes:
/// normal strength multiplies the height slope, the rest are 0–1 map values.
#[derive(Debug, PartialEq)]
pub struct MaterialClass {
    /// Short name used in the manifest and the dry-run listing.
    pub name: &'static str,
    /// BSP material ids (`MATERIAL_*`) that select this class.
    pub bsp_materials: &'static [u32],
    /// Lower-case substrings of the texture path that select this class.
    pub keywords: &'static [&'static str],
    /// Slope of the normal map per unit of normalised height at 256 texels.
    pub normal_strength: f32,
    /// Write height for parallax (`_nh`); otherwise a plain normal map (`_n`).
    pub parallax: bool,
    /// Base roughness (rend2 packed roughness: 0 mirror, 1 matte).
    pub roughness: f32,
    /// How far local variation, brightness and cavities move the roughness.
    pub roughness_variation: f32,
    /// Metalness of bright, unsaturated texels; painted and dark texels get less.
    pub metalness: f32,
    /// Ambient darkening of cavities (occlusion channel), 0 for none.
    pub occlusion: f32,
    /// Alpha-tested stages of this class may get maps (grates, not foliage).
    pub alpha_test_safe: bool,
}

/// BSP material ids of OpenJK `surfaceflags.h`.
pub mod bsp {
    pub const NONE: u32 = 0;
    pub const SOLID_WOOD: u32 = 1;
    pub const HOLLOW_WOOD: u32 = 2;
    pub const SOLID_METAL: u32 = 3;
    pub const HOLLOW_METAL: u32 = 4;
    pub const SHORT_GRASS: u32 = 5;
    pub const LONG_GRASS: u32 = 6;
    pub const DIRT: u32 = 7;
    pub const SAND: u32 = 8;
    pub const GRAVEL: u32 = 9;
    pub const GLASS: u32 = 10;
    pub const CONCRETE: u32 = 11;
    pub const MARBLE: u32 = 12;
    pub const WATER: u32 = 13;
    pub const SNOW: u32 = 14;
    pub const ICE: u32 = 15;
    pub const FLESH: u32 = 16;
    pub const MUD: u32 = 17;
    pub const BP_GLASS: u32 = 18;
    pub const DRY_LEAVES: u32 = 19;
    pub const GREEN_LEAVES: u32 = 20;
    pub const FABRIC: u32 = 21;
    pub const CANVAS: u32 = 22;
    pub const ROCK: u32 = 23;
    pub const RUBBER: u32 = 24;
    pub const PLASTIC: u32 = 25;
    pub const TILES: u32 = 26;
    pub const CARPET: u32 = 27;
    pub const PLASTER: u32 = 28;
    pub const SHATTER_GLASS: u32 = 29;
    pub const ARMOR: u32 = 30;
    pub const COMPUTER: u32 = 31;
    /// `MATERIAL_MASK`: the material id's bits in `surfaceFlags`.
    pub const MASK: u32 = 0x1f;
    /// `SURF_METALSTEPS`.
    pub const SURF_METALSTEPS: u32 = 0x0000_8000;
}

/// The classes, in keyword-matching order: more specific rows first.
pub const CLASSES: &[MaterialClass] = &[
    MaterialClass {
        name: "foliage",
        bsp_materials: &[bsp::DRY_LEAVES, bsp::GREEN_LEAVES, bsp::LONG_GRASS],
        keywords: &[
            "leaf", "leaves", "foliage", "plant", "bush", "vine", "ivy", "fern", "hedge", "branch",
            "frond",
        ],
        normal_strength: 1.5,
        parallax: false,
        roughness: 0.8,
        roughness_variation: 0.15,
        metalness: 0.0,
        occlusion: 0.3,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "glass",
        bsp_materials: &[bsp::GLASS, bsp::BP_GLASS, bsp::SHATTER_GLASS, bsp::ICE],
        keywords: &["glass", "window"],
        normal_strength: 0.5,
        parallax: false,
        roughness: 0.1,
        roughness_variation: 0.1,
        metalness: 0.0,
        occlusion: 0.0,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "electronics",
        bsp_materials: &[bsp::COMPUTER, bsp::PLASTIC, bsp::RUBBER],
        keywords: &[
            "computer",
            "console",
            "monitor",
            "screen",
            "display",
            "panel_light",
            "button",
            "keypad",
            "plastic",
        ],
        normal_strength: 1.5,
        parallax: false,
        roughness: 0.45,
        roughness_variation: 0.25,
        metalness: 0.1,
        occlusion: 0.3,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "lights",
        bsp_materials: &[],
        keywords: &["light", "lamp", "neon", "glow"],
        normal_strength: 1.0,
        parallax: false,
        roughness: 0.4,
        roughness_variation: 0.2,
        metalness: 0.0,
        occlusion: 0.2,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "metal",
        bsp_materials: &[bsp::SOLID_METAL, bsp::HOLLOW_METAL, bsp::ARMOR],
        keywords: &[
            "metal", "steel", "grate", "grating", "grill", "pipe", "girder", "rust", "chrome",
            "mtl", "vent", "rivet", "hatch", "duct", "catwalk", "railing",
        ],
        normal_strength: 2.0,
        parallax: false,
        roughness: 0.45,
        roughness_variation: 0.35,
        // Kept low on purpose: rend2's packed path takes the diffuse share away
        // from metal, and nothing reflects the surroundings back into it.
        metalness: 0.3,
        occlusion: 0.4,
        alpha_test_safe: true,
    },
    MaterialClass {
        name: "tiles",
        bsp_materials: &[bsp::TILES, bsp::MARBLE],
        keywords: &["tile", "marble", "mosaic", "polished"],
        normal_strength: 2.5,
        parallax: true,
        roughness: 0.35,
        roughness_variation: 0.35,
        metalness: 0.0,
        occlusion: 0.5,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "stone",
        bsp_materials: &[bsp::ROCK, bsp::CONCRETE, bsp::GRAVEL],
        keywords: &[
            "rock", "stone", "brick", "cobble", "cliff", "boulder", "slate", "granite", "cave",
            "canyon", "concrete", "cement", "gravel", "pillar", "column", "masonry", "ruin",
        ],
        normal_strength: 3.0,
        parallax: true,
        roughness: 0.85,
        roughness_variation: 0.2,
        metalness: 0.0,
        occlusion: 0.6,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "wood",
        bsp_materials: &[bsp::SOLID_WOOD, bsp::HOLLOW_WOOD],
        keywords: &["wood", "plank", "crate", "bark", "timber", "logs"],
        normal_strength: 1.8,
        parallax: false,
        roughness: 0.7,
        roughness_variation: 0.25,
        metalness: 0.0,
        occlusion: 0.4,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "ground",
        bsp_materials: &[bsp::DIRT, bsp::SAND, bsp::MUD, bsp::SNOW, bsp::SHORT_GRASS],
        keywords: &[
            "dirt", "sand", "mud", "snow", "ground", "terrain", "grass", "soil",
        ],
        normal_strength: 2.0,
        parallax: true,
        roughness: 0.9,
        roughness_variation: 0.1,
        metalness: 0.0,
        occlusion: 0.4,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "fabric",
        bsp_materials: &[bsp::FABRIC, bsp::CANVAS, bsp::CARPET, bsp::FLESH],
        keywords: &[
            "cloth", "fabric", "carpet", "banner", "rug", "canvas", "tapestry", "curtain", "flag",
        ],
        normal_strength: 1.0,
        parallax: false,
        roughness: 0.95,
        roughness_variation: 0.05,
        metalness: 0.0,
        occlusion: 0.3,
        alpha_test_safe: false,
    },
    MaterialClass {
        name: "plaster",
        bsp_materials: &[bsp::PLASTER],
        keywords: &["plaster", "stucco", "drywall", "adobe", "clay"],
        normal_strength: 1.2,
        parallax: false,
        roughness: 0.9,
        roughness_variation: 0.1,
        metalness: 0.0,
        occlusion: 0.3,
        alpha_test_safe: false,
    },
];

/// The class of textures nothing else describes: moderate bumps, matte.
pub const GENERIC: MaterialClass = MaterialClass {
    name: "generic",
    bsp_materials: &[bsp::NONE, bsp::WATER],
    keywords: &[],
    normal_strength: 1.5,
    parallax: false,
    roughness: 0.75,
    roughness_variation: 0.25,
    metalness: 0.0,
    occlusion: 0.35,
    alpha_test_safe: false,
};

/// Which rule chose a class, for the manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassSource {
    /// The BSP material id.
    BspMaterial(u32),
    /// A path keyword.
    Keyword(&'static str),
    /// `surfaceparm metalsteps`.
    MetalSteps,
    /// Nothing matched.
    Default,
}

impl ClassSource {
    /// Human-readable description used in listings and the manifest.
    pub fn describe(self) -> String {
        match self {
            Self::BspMaterial(id) => format!("bsp material {id}"),
            Self::Keyword(word) => format!("keyword \"{word}\""),
            Self::MetalSteps => "surfaceparm metalsteps".to_owned(),
            Self::Default => "default".to_owned(),
        }
    }
}

/// Choose the class of a texture. `image_path` is the diffuse image's VFS path;
/// `surface_flags` the BSP shader lump's flags of the shader that uses it.
pub fn classify(image_path: &str, surface_flags: u32) -> (&'static MaterialClass, ClassSource) {
    let material = surface_flags & bsp::MASK;
    if material != bsp::NONE
        && let Some(class) = CLASSES
            .iter()
            .find(|class| class.bsp_materials.contains(&material))
    {
        return (class, ClassSource::BspMaterial(material));
    }
    let lower = image_path.to_ascii_lowercase();
    let searched = lower.strip_prefix("textures/").unwrap_or(&lower);
    for class in CLASSES {
        if let Some(word) = class.keywords.iter().find(|word| searched.contains(*word)) {
            return (class, ClassSource::Keyword(word));
        }
    }
    if surface_flags & bsp::SURF_METALSTEPS != 0 {
        let metal = by_name("metal").expect("the table has a metal class");
        return (metal, ClassSource::MetalSteps);
    }
    (&GENERIC, ClassSource::Default)
}

/// Look a class up by its name.
pub fn by_name(name: &str) -> Option<&'static MaterialClass> {
    CLASSES
        .iter()
        .chain(std::iter::once(&GENERIC))
        .find(|class| class.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bsp_material_wins_over_keywords() {
        let (class, source) = classify("textures/a/metal_wall", bsp::ROCK);
        assert_eq!(class.name, "stone");
        assert_eq!(source, ClassSource::BspMaterial(bsp::ROCK));
    }

    #[test]
    fn keywords_follow_table_order() {
        assert_eq!(classify("textures/x/rockwall", 0).0.name, "stone");
        assert_eq!(classify("textures/x/metal_floor", 0).0.name, "metal");
        // "glass" comes before "metal" in the table.
        assert_eq!(classify("textures/x/metal_glass", 0).0.name, "glass");
        // Only the part below textures/ is searched.
        assert_eq!(classify("textures/x/plain", 0).0.name, "generic");
    }

    #[test]
    fn metal_steps_and_default() {
        let (class, source) = classify("textures/x/plain", bsp::SURF_METALSTEPS);
        assert_eq!((class.name, source), ("metal", ClassSource::MetalSteps));
        let (class, source) = classify("textures/x/plain", 0);
        assert_eq!((class.name, source), ("generic", ClassSource::Default));
    }

    #[test]
    fn every_bsp_material_has_one_class() {
        for id in 0..32 {
            let classes = CLASSES
                .iter()
                .chain(std::iter::once(&GENERIC))
                .filter(|class| class.bsp_materials.contains(&id))
                .count();
            assert_eq!(classes, 1, "material id {id}");
        }
    }

    #[test]
    fn table_values_are_in_range() {
        for class in CLASSES.iter().chain(std::iter::once(&GENERIC)) {
            assert!(class.normal_strength > 0.0, "{}", class.name);
            for value in [
                class.roughness,
                class.roughness_variation,
                class.metalness,
                class.occlusion,
            ] {
                assert!((0.0..=1.0).contains(&value), "{}", class.name);
            }
        }
        assert!(by_name("generic").is_some());
    }
}
