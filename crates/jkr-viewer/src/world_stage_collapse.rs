//! Load-time multitexture collapse.
use super::*;

/// Apply rd-vanilla `CollapseMultitexture` at map load.
///
/// The eight blend pairs are copied from `tr_shader.cpp:2404-2432`; compatible
/// state and generator/wave guards mirror `tr_shader.cpp:2458-2522`. As in the
/// original mask, depth-write is excluded from the compatibility comparison;
/// the collapsed pass retains the first stage's depth-write state.
pub(crate) fn collapse_multitexture(stages: &[ShaderStage]) -> Vec<CompiledStage> {
    let mut result = Vec::with_capacity(stages.len());
    let mut index = 0;
    while index < stages.len() {
        if let Some((operator, output_blend)) = stages
            .get(index + 1)
            .and_then(|second| collapse_pair(&stages[index], second))
        {
            let mut primary = stages[index].clone();
            let mut secondary = stages[index + 1].clone();
            if primary.texture_generator == TextureGenerator::Lightmap {
                std::mem::swap(&mut primary, &mut secondary);
            }
            result.push(CompiledStage {
                primary,
                secondary: Some(secondary),
                combine: operator,
                output_blend,
                output_depth_write: stages[index].depth_write,
                output_depth_function: stages[index].depth_function,
            });
            index += 2;
        } else {
            result.push(CompiledStage {
                primary: stages[index].clone(),
                secondary: None,
                combine: CollapseOperator::None,
                output_blend: stages[index].blend.clone(),
                output_depth_write: stages[index].depth_write,
                output_depth_function: stages[index].depth_function,
            });
            index += 1;
        }
    }
    result
}

fn collapse_pair(
    first: &ShaderStage,
    second: &ShaderStage,
) -> Option<(CollapseOperator, StageBlend)> {
    if first.depth_function != second.depth_function
        || first.alpha_function != second.alpha_function
        || first.rgb_generator != second.rgb_generator
        || first.alpha_generator != second.alpha_generator
        || first.rgb_wave != second.rgb_wave
        || first.alpha_wave != second.alpha_wave
        || first.rgb_constant != second.rgb_constant
        || first.alpha_constant != second.alpha_constant
    {
        return None;
    }
    let (operator, output) = collapse_rule(&first.blend, &second.blend)?;
    if operator == CollapseOperator::Add
        && first
            .rgb_generator
            .as_deref()
            .is_some_and(|generator| !generator.eq_ignore_ascii_case("identity"))
    {
        return None;
    }
    Some((operator, output))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlendBits {
    Replace,
    ModulateSource,
    ModulateDestination,
    Add,
    Other,
}

fn collapse_rule(
    first: &StageBlend,
    second: &StageBlend,
) -> Option<(CollapseOperator, StageBlend)> {
    use BlendBits::{Add, ModulateDestination, ModulateSource, Replace};
    use CollapseOperator::{Add as AddTextures, Modulate};
    let first = blend_bits(first);
    let second = blend_bits(second);
    match (first, second) {
        (Replace, ModulateSource) | (Replace, ModulateDestination) => {
            Some((Modulate, StageBlend::Replace))
        }
        (ModulateSource, ModulateSource)
        | (ModulateDestination, ModulateSource)
        | (ModulateSource, ModulateDestination)
        | (ModulateDestination, ModulateDestination) => Some((Modulate, StageBlend::Filter)),
        (Replace, Add) => Some((AddTextures, StageBlend::Replace)),
        (Add, Add) => Some((AddTextures, StageBlend::Add)),
        _ => None,
    }
}

fn blend_bits(blend: &StageBlend) -> BlendBits {
    match blend {
        StageBlend::Replace => BlendBits::Replace,
        StageBlend::Filter => BlendBits::ModulateSource,
        StageBlend::Add => BlendBits::Add,
        StageBlend::Custom {
            source,
            destination,
        } if source.eq_ignore_ascii_case("gl_zero")
            && destination.eq_ignore_ascii_case("gl_src_color") =>
        {
            BlendBits::ModulateDestination
        }
        _ => BlendBits::Other,
    }
}
