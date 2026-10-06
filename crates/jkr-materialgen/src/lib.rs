//! `jkr-materialgen`: a local generator of material maps for the world
//! textures of an installed Jedi Academy, in the convention of OpenJK's rend2
//! renderer that JKR's optional material maps read (`r_normalMapping`,
//! `r_specularMapping`, `r_parallaxMapping`).
//!
//! # What it does
//!
//! 1. Mounts the game data like the client ([`mount`]): `GameData/base`, an
//!    optional `fs_game` directory and `JKR_CONTENT`, loose files and pk3s in
//!    Quake 3 order, case-insensitive. Its own earlier output is left out.
//! 2. Reads the shader lumps and surfaces of the installed maps (or `--maps`)
//!    and the shader scripts, and picks the diffuse images of shaders drawn on
//!    lightmapped surfaces whose lightmap and diffuse stages collapse into one
//!    opaque pass: the stages the renderer gives maps to ([`select`]). Skies,
//!    fog, liquids, nodraw/clip/system shaders, interface images, lightmaps,
//!    blend-only effects, deforms, glowing, animated or environment-mapped
//!    stages, alpha-tested foliage and textures that already have rend2 maps
//!    are skipped, each with a recorded reason.
//! 3. Generates, per texture and deterministically ([`generate`]), a
//!    tangent-space normal map from a height estimated from the albedo
//!    (multi-scale high-pass that drops baked lighting gradients, Scharr
//!    gradients, wrap-around filtering so tiling textures stay seamless), and a
//!    packed roughness/metalness/occlusion map from the material class
//!    ([`classes`], the one table to tune) and local image statistics.
//! 4. Writes one pk3 ([`package`]) of PNGs at the names rend2's automatic
//!    lookup tries next to the diffuse image: `<texture>_nh.png` (normal RGB,
//!    height in alpha) for parallax-worthy classes, `<texture>_n.png`
//!    otherwise, and `<texture>_rmo.png`, plus `jkr-materialgen/manifest.json`.
//!
//! # Why `_rmo`
//!
//! rend2 reads two specular conventions: `_specGloss` (specular colour and
//! gloss, which JKR passes through rend2's SDR colour-ratio conversion) and the
//! packed `_rmo`/`_orm` (roughness, metalness, occlusion). The heuristics
//! produce roughness and metalness directly, and the packed path uses the
//! albedo as the metal colour and a 0.04 dielectric reflectance on its own, so
//! `_rmo` needs no colour guesswork and no conversion to undo. Its red,
//! green and blue are roughness, metalness and occlusion; JKR reorders them to
//! occlusion, roughness, metalness at load, as rend2's swizzle does.
//! Metalness stays low (at most 0.3 for the metal class): in that path metal
//! loses its diffuse light and nothing reflects the surroundings back into
//! it, so fully metallic retail textures would turn dark.
//!
//! # Using the output
//!
//! The default output is `<JKR user data>/generated/zzz_jkr_materials.pk3`
//! (`%APPDATA%\jkr\generated` on Windows), never the game folder. Either set
//! `JKR_CONTENT` to that directory, which the client mounts above the game
//! data, or copy the pk3 into `GameData/base` (the `zzz_` name sorts after the
//! retail `assets*.pk3`). Then set `r_normalMapping 1`, `r_specularMapping 1`
//! and optionally `r_parallaxMapping 1` and restart.
//!
//! # Retail data
//!
//! The generated images are derived from retail textures. They stay on the
//! player's machine and must never be shared or committed; the tests use
//! synthetic images only.

pub mod classes;
pub mod cli;
pub mod filters;
pub mod generate;
pub mod mount;
pub mod package;
pub mod run;
pub mod select;
