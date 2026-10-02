//! Optional material maps for world stages in the convention of OpenJK's rend2
//! renderer (`codemp/rd-rend2`): normal maps (with height in alpha for parallax)
//! and specular or packed roughness/metalness/occlusion maps, named by stage
//! keywords or found next to the diffuse image (`_nh`, `_n`, `_specGloss`,
//! `_rmo`, `_orm`), so existing rend2 texture packs apply unchanged.
//!
//! Everything is opt-in (`r_normalMapping`, `r_specularMapping`,
//! `r_parallaxMapping`, sampled at startup like rend2's latched cvars). Off, no
//! image is looked up, no layout, buffer or program exists and every stage
//! compiles exactly as before. On, a stage with maps compiles to its own
//! pipeline key ([`PIPELINE_BIT`]) whose program is the ordinary stage program
//! plus the material hooks (`material_map_program.rs`); stages without maps keep
//! their pipelines, bind groups and stage-table records.
//!
//! The first step covers lightmapped world surfaces (static and inline movers)
//! whose lightmap and diffuse stages collapse into one pass. Models, vertex-lit
//! surfaces and uncollapsed stacks keep their authored shading.

#[path = "material_map_frames.rs"]
pub(crate) mod frames;
#[path = "material_map_gpu.rs"]
pub(super) mod gpu;
#[path = "material_map_images.rs"]
mod images;
#[path = "material_map_program.rs"]
pub(super) mod program;

use crate::world_stage::{CollapseOperator, CompiledStage};
use image::RgbaImage;
use jkr_shader::{ShaderCatalog, StageBlend, TextureGenerator};
use jkr_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use jkr_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// [`crate::world_stage::PipelineKey::geometry`] bit of a material-mapped stage.
pub(crate) const PIPELINE_BIT: u8 = 16;

/// Geometry bits without [`PIPELINE_BIT`]: material maps change shading only, so a
/// mapped stage still casts sun shadows, fills the light buffer and receives SSAO.
pub(crate) const fn without_maps(geometry: u8) -> u8 {
    geometry & !PIPELINE_BIT
}

/// First bit of `r_materialMapsDebug` in the scene lighting-mode word
/// (`world_lighting_mode.rs`); the material program reads two bits from here.
pub(crate) const DEBUG_SHIFT: u32 = 8;

/// The controls in the order of their [`LATCH`] bits.
const CONTROLS: [&str; 3] = ["r_normalMapping", "r_specularMapping", "r_parallaxMapping"];
/// [`LATCH`] has been written: the viewer sampled its controls.
const LATCHED: u8 = 0x80;
/// The control values the running viewer sampled (bit `i` for `CONTROLS[i]`, plus
/// [`LATCHED`]). Before the sample, changes are the startup configuration being
/// applied, not changes that need a restart.
static LATCH: AtomicU8 = AtomicU8::new(0);

/// Startup policy, rend2's names and default-off.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Settings {
    /// `r_normalMapping`: normal maps (and their height, for parallax).
    pub(crate) normal: bool,
    /// `r_specularMapping`: specular and packed material maps.
    pub(crate) specular: bool,
    /// `r_parallaxMapping`: parallax from the height in a normal map's alpha.
    pub(crate) parallax: bool,
}

impl Settings {
    /// Read the registered values once, at context creation, and remember them so a
    /// later change can tell whether it needs a restart.
    pub(crate) fn sample(console: Option<&crate::console::ViewerConsole>) -> Self {
        let on = |name| console.and_then(|c| c.integer_cvar(name)).unwrap_or(0) != 0;
        let [normal, specular, parallax] = CONTROLS.map(on);
        if console.is_some() {
            let bits = [normal, specular, parallax]
                .iter()
                .enumerate()
                .fold(LATCHED, |bits, (index, on)| bits | (u8::from(*on) << index));
            LATCH.store(bits, Ordering::Relaxed);
        }
        Self {
            normal,
            specular,
            // Parallax reads the normal map's height: nothing to do without normal maps.
            parallax: normal && parallax,
        }
    }

    /// Whether any material map may be looked up.
    pub(crate) fn enabled(self) -> bool {
        self.normal || self.specular
    }
}

/// Whether setting control `index` to `on` differs from what the viewer runs with
/// (`latch`, the [`LATCH`] word). Nothing differs before the startup sample: the
/// configuration file setting a control is what the sample then reads.
fn restart_needed(latch: u8, index: usize, on: bool) -> bool {
    latch & LATCHED != 0 && (latch >> index & 1 != 0) != on
}

/// The enabled kinds for the load log, as `normal+specular+parallax` (or `off`).
impl std::fmt::Display for Settings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kinds = [
            (self.normal, "normal"),
            (self.specular, "specular"),
            (self.parallax, "parallax"),
        ];
        let mut first = true;
        for (_, name) in kinds.iter().filter(|(on, _)| *on) {
            formatter.write_str(if first { "" } else { "+" })?;
            formatter.write_str(name)?;
            first = false;
        }
        if first {
            formatter.write_str("off")?;
        }
        Ok(())
    }
}

/// Register the rend2-named controls; a change asks for a restart, like rend2's latch.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    for (index, (name, help)) in [
        (
            "r_normalMapping",
            "Normal maps on world surfaces (rend2 _n/_nh images and normalMap keywords); \
             restart required",
        ),
        (
            "r_specularMapping",
            "Specular/roughness maps on world surfaces (rend2 _specGloss/_rmo/_orm images and \
             keywords); restart required",
        ),
        (
            "r_parallaxMapping",
            "Parallax from the height in a normal map's alpha (_nh images, normalHeightMap); \
             needs r_normalMapping; restart required",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        debug_assert_eq!(name, CONTROLS[index]);
        cvars.register(CvarDefinition::new(name, 0_i64, CvarFlags::ARCHIVE, help))?;
        cvars.on_change(name, move |change| {
            let on = matches!(change.current, CvarValue::Integer(value) if value != 0);
            if restart_needed(LATCH.load(Ordering::Relaxed), index, on) {
                crate::log::progress(format_args!(
                    "{name} changed: restart the viewer to apply material maps"
                ))
            }
        })?;
    }
    Ok(())
}

/// Which bundle of a collapsed hardware stage holds the diffuse texture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Bundle {
    Primary,
    Secondary,
}

/// The diffuse bundle of a stage that can take material maps: a lightmapped
/// world surface's lightmap and diffuse texture collapsed into one opaque pass
/// with plain colour generators. The maps then replace the lightmap's response
/// (rend2's `CollapseStagesToLightall` makes the same pairing). Everything else,
/// including deforming and sprite stages, keeps its authored shading.
pub(super) fn diffuse_bundle(stage: &CompiledStage, lightmap: i32) -> Option<Bundle> {
    let secondary = stage.secondary.as_ref()?;
    if lightmap < 0
        || stage.combine != CollapseOperator::Modulate
        || stage.output_blend != StageBlend::Replace
    {
        return None;
    }
    let bundle = match (stage.primary.texture_generator, secondary.texture_generator) {
        (TextureGenerator::Base, TextureGenerator::Lightmap) => Bundle::Primary,
        (TextureGenerator::Lightmap, TextureGenerator::Base) => Bundle::Secondary,
        _ => return None,
    };
    let diffuse = match bundle {
        Bundle::Primary => &stage.primary,
        Bundle::Secondary => secondary,
    };
    let plain = |generator: Option<&str>| {
        generator.is_none_or(|g| {
            g.eq_ignore_ascii_case("identity") || g.eq_ignore_ascii_case("identityLighting")
        })
    };
    // Collapsed bundles share their generators (`collapse_pair`), so one check covers both.
    let plain_alpha = diffuse.alpha_generator.as_deref().is_none_or(|g| {
        !g.eq_ignore_ascii_case("lightingSpecular") && !g.eq_ignore_ascii_case("portal")
    });
    (plain(diffuse.rgb_generator.as_deref())
        && diffuse.rgb_wave.is_none()
        && diffuse.rgb_constant.is_none()
        && plain_alpha
        && diffuse.surface_sprites.is_none())
    .then_some(bundle)
}

/// One decoded map in the layout the material program reads, with its cache key.
#[derive(Clone, Debug)]
pub(super) struct MapImage {
    pub(super) key: String,
    pub(super) pixels: Arc<RgbaImage>,
}

/// Uniform parameters of one material-mapped stage (`MaterialMapParams` in WGSL).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Params {
    /// rend2 `normalScale`: x/y strength, z unused, w parallax depth.
    pub(super) normal_scale: [f32; 4],
    /// rend2 `specularScale`.
    pub(super) specular_scale: [f32; 4],
    /// x: [`FLAG_NORMAL`] and friends; y: specular layout (0 none, 1 spec/gloss,
    /// 2 occlusion-roughness-metalness-specular); z: parallax bias; w unused.
    pub(super) control: [f32; 4],
}

/// The stage has a normal map.
pub(super) const FLAG_NORMAL: u32 = 1;
/// The normal map's alpha is a depth map and parallax is enabled.
pub(super) const FLAG_PARALLAX: u32 = 2;
/// The diffuse texture is the secondary bundle.
pub(super) const FLAG_SECONDARY: u32 = 4;
/// Two-sided material: shade the side facing the viewer.
pub(super) const FLAG_TWO_SIDED: u32 = 8;

/// The maps of one stage, decoded on a load worker and uploaded with its bind group.
#[derive(Clone, Debug)]
pub(super) struct StageMaps {
    pub(super) normal: Option<MapImage>,
    pub(super) specular: Option<MapImage>,
    pub(super) params: Params,
    /// Clamp the maps like the diffuse texture.
    pub(super) clamp: bool,
}

/// How many stages of a map found which maps, for the load log.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Counts {
    pub(super) stages: usize,
    pub(super) normal: usize,
    pub(super) parallax: usize,
    pub(super) specular: usize,
}

impl Counts {
    pub(super) fn of<'a>(maps: impl Iterator<Item = &'a StageMaps>) -> Self {
        maps.fold(Self::default(), |found, maps| {
            let flags = maps.params.control[0] as u32;
            Self {
                stages: found.stages + 1,
                normal: found.normal + usize::from(maps.normal.is_some()),
                parallax: found.parallax + usize::from(flags & FLAG_PARALLAX != 0),
                specular: found.specular + usize::from(maps.specular.is_some()),
            }
        })
    }
}

/// Find, decode and convert the maps of one hardware stage. `None` when the stage
/// cannot take maps or none exist; the stage then compiles exactly as without maps.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    settings: Settings,
    stage: &CompiledStage,
    lightmap: i32,
    two_sided: bool,
    implicit_name: &str,
    cache: &mut HashMap<String, Arc<RgbaImage>>,
) -> Result<Option<StageMaps>, Box<dyn Error>> {
    if !settings.enabled() {
        return Ok(None);
    }
    let Some(bundle) = diffuse_bundle(stage, lightmap) else {
        return Ok(None);
    };
    let diffuse = match bundle {
        Bundle::Primary => &stage.primary,
        Bundle::Secondary => stage.secondary.as_ref().expect("collapsed stage"),
    };
    let found = images::find(vfs, shaders, settings, diffuse, implicit_name, cache)?;
    if found.normal.is_none() && found.specular.is_none() {
        return Ok(None);
    }
    let mut flags = 0;
    if found.normal.is_some() {
        flags |= FLAG_NORMAL;
        if settings.parallax && found.height {
            flags |= FLAG_PARALLAX;
        }
    }
    if bundle == Bundle::Secondary {
        flags |= FLAG_SECONDARY;
    }
    if two_sided {
        flags |= FLAG_TWO_SIDED;
    }
    Ok(Some(StageMaps {
        params: Params {
            normal_scale: found.normal_scale,
            specular_scale: found.specular_scale,
            control: [
                flags as f32,
                found
                    .specular
                    .as_ref()
                    .map_or(0.0, |(_, kind)| *kind as f32),
                found.parallax_bias,
                0.0,
            ],
        },
        normal: found.normal,
        specular: found.specular.map(|(image, _)| image),
        clamp: diffuse.clamp,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_stage::collapse_multitexture;

    fn stages(script: &str) -> Vec<CompiledStage> {
        let definitions = jkr_shader::parse_shader_script(script.as_bytes(), "scripts/t.shader")
            .expect("script parses");
        collapse_multitexture(&definitions[0].stages)
    }

    #[test]
    fn lightmapped_diffuse_pairs_take_maps() {
        let compiled =
            stages("textures/a {\n{ map $lightmap }\n{ map textures/a/floor blendFunc filter }\n}");
        assert_eq!(compiled.len(), 1);
        assert_eq!(diffuse_bundle(&compiled[0], 0), Some(Bundle::Primary));
        // Vertex-lit and model surfaces have no lightmap to redistribute.
        assert_eq!(diffuse_bundle(&compiled[0], -3), None);
        assert_eq!(diffuse_bundle(&compiled[0], -1), None);
        let reversed =
            stages("textures/a {\n{ map textures/a/floor }\n{ map $lightmap blendFunc filter }\n}");
        assert_eq!(diffuse_bundle(&reversed[0], 2), Some(Bundle::Primary));
    }

    #[test]
    fn unscripted_textures_take_maps() {
        // A texture without a shader script draws as an implicit lightmap + texture pair
        // whose diffuse stage names no image: the material name is the image.
        let (stages, _, _) = crate::world_stage::material_stages(None, 3);
        let compiled = collapse_multitexture(&stages);
        assert_eq!(compiled.len(), 1);
        assert!(compiled[0].primary.images.is_empty());
        assert_eq!(diffuse_bundle(&compiled[0], 3), Some(Bundle::Primary));
    }

    #[test]
    fn effect_stages_keep_authored_shading() {
        for script in [
            // A single texture without a lightmap.
            "textures/a { { map textures/a/floor } }",
            // Additive glow over the lightmap.
            "textures/a {\n{ map $lightmap }\n{ map textures/a/glow blendFunc add }\n}",
            // Environment mapping.
            "textures/a {\n{ map $lightmap }\n{ map textures/a/env tcGen environment \
             blendFunc filter }\n}",
            // Animated colour.
            "textures/a {\n{ map $lightmap rgbGen wave sin 0 1 0 1 }\n\
             { map textures/a/floor blendFunc filter rgbGen wave sin 0 1 0 1 }\n}",
        ] {
            let compiled = stages(script);
            assert!(
                compiled
                    .iter()
                    .all(|stage| diffuse_bundle(stage, 0).is_none()),
                "{script}"
            );
        }
    }

    #[test]
    fn settings_need_normal_maps_for_parallax() {
        let settings = Settings::sample(None);
        assert_eq!(settings, Settings::default());
        assert!(!settings.enabled());
        assert!(
            Settings {
                specular: true,
                ..Default::default()
            }
            .enabled()
        );
    }

    #[test]
    fn counts_name_the_maps_each_stage_found() {
        let image = MapImage {
            key: "material:normal:t".into(),
            pixels: Arc::new(RgbaImage::new(1, 1)),
        };
        let stage = |normal: bool, specular: bool, flags: u32| StageMaps {
            normal: normal.then(|| image.clone()),
            specular: specular.then(|| image.clone()),
            params: Params {
                control: [flags as f32, 0.0, 0.0, 0.0],
                ..Default::default()
            },
            clamp: false,
        };
        let stages = [
            stage(true, true, FLAG_NORMAL | FLAG_PARALLAX),
            stage(true, false, FLAG_NORMAL),
            stage(false, true, 0),
        ];
        assert_eq!(
            Counts::of(stages.iter()),
            Counts {
                stages: 3,
                normal: 2,
                parallax: 1,
                specular: 2,
            }
        );
    }

    #[test]
    fn settings_name_their_kinds_for_the_log() {
        assert_eq!(Settings::default().to_string(), "off");
        let all = Settings {
            normal: true,
            specular: true,
            parallax: true,
        };
        assert_eq!(all.to_string(), "normal+specular+parallax");
        let specular = Settings {
            specular: true,
            ..Default::default()
        };
        assert_eq!(specular.to_string(), "specular");
    }

    #[test]
    fn pipeline_bit_only_separates_shading() {
        // Distinct from deforms (1), sprites (2), live emission (4) and polygon offset (8).
        assert_eq!(
            PIPELINE_BIT & (1 | 2 | 4 | crate::world_stage::POLYGON_OFFSET),
            0
        );
        let stage = &stages(
            "textures/a {
{ map $lightmap }
{ map textures/a/b blendFunc filter }
}",
        )[0];
        let plain = crate::world_stage::hardware_pipeline_key(stage, jkr_shader::ShaderCull::Front);
        let mut mapped = plain;
        mapped.geometry |= PIPELINE_BIT;
        // Mapped and plain stages never share a pipeline, yet a mapped stage still casts
        // shadows, fills the light buffer and receives SSAO like the plain one.
        assert_ne!(plain, mapped);
        assert_eq!(without_maps(mapped.geometry), plain.geometry);
        assert!(crate::world_materials::ssao::eligible(
            mapped,
            &crate::world_stage::compile_hardware_stage(stage)
        ));
    }

    #[test]
    fn only_changes_after_the_startup_sample_ask_for_a_restart() {
        // The configuration file sets the controls before the viewer samples them.
        assert!(!restart_needed(0, 0, true));
        assert!(!restart_needed(0, 2, false));
        // Sampled with normal maps on and the others off.
        let latch = LATCHED | 1;
        assert!(!restart_needed(latch, 0, true));
        assert!(restart_needed(latch, 0, false));
        assert!(restart_needed(latch, 1, true));
        assert!(!restart_needed(latch, 1, false));
        // Parallax latches its own value, not the effective one: a restart with it
        // on is what the change asks for, even while normal maps are off.
        assert!(restart_needed(LATCHED, 2, true));
    }

    #[test]
    fn params_match_the_wgsl_uniform_size() {
        assert_eq!(std::mem::size_of::<Params>(), 48);
    }
}
