//! The material program: the ordinary stage program of the lighting mode with
//! hooks for material maps.
//!
//! Like the stage table's program, it is the unchanged stage source with a few
//! exact replacements, each asserted to match once, so the ordinary programs
//! stay byte for byte what they were. The hooks:
//! - the vertex stage reads the vertex's frame (`material_map_frames`) and passes
//!   the tangent and light direction on two more varyings;
//! - the fragment stage first resolves the mapped normal, the specular sample and
//!   the parallax offset (`material_map_prepare`), and samples the diffuse image
//!   at the offset texture coordinate;
//! - the lightmap (or the real-time light buffer) response is replaced by
//!   `material_map_lightmap`, point lights use the mapped normal, and highlights
//!   are added after the albedo product and dynamic-light modulation
//!   (`material_map_finish`, which also draws the `r_materialMapsDebug` views).

use super::super::{STAGE_SHADER, world_sun_shader};

/// The material program's WGSL for the authored (`realtime == false`) or the
/// real-time (light buffer) lighting mode.
pub(in crate::world_materials) fn source(realtime: bool) -> String {
    let base = if realtime {
        world_sun_shader()
    } else {
        STAGE_SHADER
    };
    // Checkouts may carry CRLF line ends (`core.autocrlf`); the patterns use LF.
    let mut source = base.replace("\r\n", "\n");
    for (from, to) in [
        (
            "    @location(11) @interpolate(flat) animation_index: i32,\n};",
            "    @location(11) @interpolate(flat) animation_index: i32,\n    \
             @location(12) material_tangent: vec4<f32>,\n    \
             @location(13) material_light: vec3<f32>,\n};",
        ),
        (
            "    if (instance.view_flags & 8u) != 0u { output.animation_index",
            "    let material_map_vertex = material_map_frame(index, instance.rotation);\n    \
             output.material_tangent = material_map_vertex.tangent;\n    \
             output.material_light = material_map_vertex.light;\n    \
             if (instance.view_flags & 8u) != 0u { output.animation_index",
        ),
        (
            "    fragment_point_mask = finite_point_mask(input.world_position);\n",
            "    fragment_point_mask = finite_point_mask(input.world_position);\n    \
             material_map_prepare(input);\n",
        ),
        (
            "textureSample(stage_images, stage_sampler, uv, frame)",
            "textureSample(stage_images, stage_sampler, uv + material_map_surface.uv_offset, frame)",
        ),
        (
            "input.secondary_uv, secondary_frame)",
            "input.secondary_uv + material_map_surface.uv_offset, secondary_frame)",
        ),
        (
            "    var output = apply_lighting_mode(input, texel, secondary_texel);",
            "    material_map_remember_albedo(texel, secondary_texel);\n    \
             var output = apply_lighting_mode(input, texel, secondary_texel);",
        ),
        (
            "        output = vec4(output.rgb * (vec3(1.0) + dynamic_light_modulation(input)), output.a);\n    }\n    return output;\n}",
            "        output = vec4(output.rgb * (vec3(1.0) + dynamic_light_modulation(input)), output.a);\n        \
             material_map_point_highlights(input);\n    }\n    \
             return material_map_finish(output);\n}",
        ),
        // Point lights shine on the mapped normal.
        (
            "    var result = vec3(0.0);\n    let normal = normalize(input.world_normal);\n    var remaining = fragment_point_mask & point_lights.metadata.w;",
            "    var result = vec3(0.0);\n    let normal = material_map_surface.normal;\n    var remaining = fragment_point_mask & point_lights.metadata.w;",
        ),
        (
            "    let normal = normalize(input.world_normal);\n    var remaining = fragment_point_mask;\n",
            "    let normal = material_map_surface.normal;\n    var remaining = fragment_point_mask;\n",
        ),
        (
            "base = realtime_lightmap(input, base);",
            "base = material_map_lightmap(input, base);",
        ),
        (
            "extra = realtime_lightmap(input, extra);",
            "extra = material_map_lightmap(input, extra);",
        ),
    ] {
        assert_eq!(
            source.matches(from).count(),
            1,
            "stage program changed: {from}"
        );
        source = source.replace(from, to);
    }
    source.push_str(include_str!("material_maps.wgsl"));
    source.push_str(if realtime {
        include_str!("material_maps_realtime.wgsl")
    } else {
        include_str!("material_maps_authored.wgsl")
    });
    source
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::naga;

    fn validate(source: &str) -> naga::valid::ModuleInfo {
        let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|error| {
            panic!("{}", error.emit_to_string(source));
        });
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)))
    }

    #[test]
    fn material_programs_validate_in_both_lighting_modes() {
        for realtime in [false, true] {
            let source = source(realtime);
            validate(&source);
            for entry in ["entity_vertex_main", "fragment_main"] {
                assert!(source.contains(&format!("fn {entry}(")), "{entry}");
            }
        }
    }

    #[test]
    fn debug_views_read_the_lighting_mode_bits_the_cvar_writes() {
        let shift = format!(
            "(point_lights.metadata.z >> {}u) & 3u",
            super::super::DEBUG_SHIFT
        );
        for realtime in [false, true] {
            let source = source(realtime);
            assert_eq!(source.matches(&shift).count(), 1, "{shift}");
            // The views replace the final colour after every other hook ran.
            assert!(source.contains("return material_map_finish(output);"));
        }
    }

    #[test]
    fn ordinary_programs_still_validate_unchanged() {
        validate(STAGE_SHADER);
        validate(world_sun_shader());
        assert!(!STAGE_SHADER.contains("material_map"));
        assert!(!world_sun_shader().contains("material_map"));
    }
}
