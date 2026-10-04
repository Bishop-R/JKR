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

/// NPC classes whose cgame footstep routine deliberately remains silent.
pub fn allowed_class(class: u8) -> bool {
    !matches!(
        class,
        1 | 4 | 7 | 8 | 10 | 16 | 30 | 32 | 34 | 35 | 39 | 41 | 42 | 45
    )
}
