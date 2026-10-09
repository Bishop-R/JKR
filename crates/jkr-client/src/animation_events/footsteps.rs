//! CG_PlayerFootsteps material banks (cg_main.c registration order).
/// Walk/run samples, four variants each, registered during world loading.
pub const PATHS: [[[&str; 4]; 2]; 11] = [
    [
        [
            "sound/player/footsteps/stone_step1.wav",
            "sound/player/footsteps/stone_step2.wav",
            "sound/player/footsteps/stone_step3.wav",
            "sound/player/footsteps/stone_step4.wav",
        ],
        [
            "sound/player/footsteps/stone_run1.wav",
            "sound/player/footsteps/stone_run2.wav",
            "sound/player/footsteps/stone_run3.wav",
            "sound/player/footsteps/stone_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/mud_walk1.wav",
            "sound/player/footsteps/mud_walk2.wav",
            "sound/player/footsteps/mud_walk3.wav",
            "sound/player/footsteps/mud_walk4.wav",
        ],
        [
            "sound/player/footsteps/mud_run1.wav",
            "sound/player/footsteps/mud_run2.wav",
            "sound/player/footsteps/mud_run3.wav",
            "sound/player/footsteps/mud_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/dirt_step1.wav",
            "sound/player/footsteps/dirt_step2.wav",
            "sound/player/footsteps/dirt_step3.wav",
            "sound/player/footsteps/dirt_step4.wav",
        ],
        [
            "sound/player/footsteps/dirt_run1.wav",
            "sound/player/footsteps/dirt_run2.wav",
            "sound/player/footsteps/dirt_run3.wav",
            "sound/player/footsteps/dirt_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/sand_walk1.wav",
            "sound/player/footsteps/sand_walk2.wav",
            "sound/player/footsteps/sand_walk3.wav",
            "sound/player/footsteps/sand_walk4.wav",
        ],
        [
            "sound/player/footsteps/sand_run1.wav",
            "sound/player/footsteps/sand_run2.wav",
            "sound/player/footsteps/sand_run3.wav",
            "sound/player/footsteps/sand_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/snow_step1.wav",
            "sound/player/footsteps/snow_step2.wav",
            "sound/player/footsteps/snow_step3.wav",
            "sound/player/footsteps/snow_step4.wav",
        ],
        [
            "sound/player/footsteps/snow_run1.wav",
            "sound/player/footsteps/snow_run2.wav",
            "sound/player/footsteps/snow_run3.wav",
            "sound/player/footsteps/snow_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/grass_step1.wav",
            "sound/player/footsteps/grass_step2.wav",
            "sound/player/footsteps/grass_step3.wav",
            "sound/player/footsteps/grass_step4.wav",
        ],
        [
            "sound/player/footsteps/grass_run1.wav",
            "sound/player/footsteps/grass_run2.wav",
            "sound/player/footsteps/grass_run3.wav",
            "sound/player/footsteps/grass_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/metal_step1.wav",
            "sound/player/footsteps/metal_step2.wav",
            "sound/player/footsteps/metal_step3.wav",
            "sound/player/footsteps/metal_step4.wav",
        ],
        [
            "sound/player/footsteps/metal_run1.wav",
            "sound/player/footsteps/metal_run2.wav",
            "sound/player/footsteps/metal_run3.wav",
            "sound/player/footsteps/metal_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/pipe_step1.wav",
            "sound/player/footsteps/pipe_step2.wav",
            "sound/player/footsteps/pipe_step3.wav",
            "sound/player/footsteps/pipe_step4.wav",
        ],
        [
            "sound/player/footsteps/pipe_run1.wav",
            "sound/player/footsteps/pipe_run2.wav",
            "sound/player/footsteps/pipe_run3.wav",
            "sound/player/footsteps/pipe_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/gravel_walk1.wav",
            "sound/player/footsteps/gravel_walk2.wav",
            "sound/player/footsteps/gravel_walk3.wav",
            "sound/player/footsteps/gravel_walk4.wav",
        ],
        [
            "sound/player/footsteps/gravel_run1.wav",
            "sound/player/footsteps/gravel_run2.wav",
            "sound/player/footsteps/gravel_run3.wav",
            "sound/player/footsteps/gravel_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/rug_step1.wav",
            "sound/player/footsteps/rug_step2.wav",
            "sound/player/footsteps/rug_step3.wav",
            "sound/player/footsteps/rug_step4.wav",
        ],
        [
            "sound/player/footsteps/rug_run1.wav",
            "sound/player/footsteps/rug_run2.wav",
            "sound/player/footsteps/rug_run3.wav",
            "sound/player/footsteps/rug_run4.wav",
        ],
    ],
    [
        [
            "sound/player/footsteps/wood_walk1.wav",
            "sound/player/footsteps/wood_walk2.wav",
            "sound/player/footsteps/wood_walk3.wav",
            "sound/player/footsteps/wood_walk4.wav",
        ],
        [
            "sound/player/footsteps/wood_run1.wav",
            "sound/player/footsteps/wood_run2.wav",
            "sound/player/footsteps/wood_run3.wav",
            "sound/player/footsteps/wood_run4.wav",
        ],
    ],
];
/// The sound family selected by codemp's ground trace surface material.
pub fn material(flags: u32) -> usize {
    match flags & 31 {
        17 => 1,
        7 => 2,
        8 => 3,
        14 => 4,
        5 | 6 => 5,
        3 => 6,
        4 => 7,
        9 => 8,
        21 | 22 | 24 | 25 | 27 => 9,
        1 | 2 => 10,
        _ => 0,
    }
}

/// The dust `_PlayerFootStep` raises from a [`material`] family's ground
/// (`cgs.effects.footstep*`): mud, sand for dirt and sand, snow, gravel.
pub fn effect(family: usize) -> Option<&'static str> {
    match family {
        1 => Some("materials/mud"),
        2 | 3 => Some("materials/sand"),
        4 => Some("materials/snow"),
        8 => Some("materials/gravel"),
        _ => None,
    }
}

/// Whether a [`material`] family's ground keeps a footprint: mud, dirt, sand and snow.
pub fn takes_print(family: usize) -> bool {
    (1..=4).contains(&family)
}

/// NPC classes whose cgame footstep routine deliberately remains silent.
pub fn allowed_class(class: u8) -> bool {
    !matches!(
        class,
        1 | 4 | 7 | 8 | 10 | 16 | 30 | 32 | 34 | 35 | 39 | 41 | 42 | 45
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_ground_raises_dust_and_keeps_prints() {
        // MATERIAL_DIRT (7) raises sand, MATERIAL_GRAVEL (9) dust without a print,
        // MATERIAL_CONCRETE (11) neither.
        assert_eq!(effect(material(7)), Some("materials/sand"));
        assert!(takes_print(material(7)) && takes_print(material(14)));
        assert_eq!(effect(material(9)), Some("materials/gravel"));
        assert!(!takes_print(material(9)));
        assert_eq!(effect(material(11)), None);
        assert!(!takes_print(material(11)));
    }
}
