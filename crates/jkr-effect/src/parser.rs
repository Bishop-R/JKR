//! Token parser for the generic Raven EFX grammar.

use crate::{
    Component, ComponentKind, Curve, CurveFlags, CurveModifier, EffectDefinition, EffectError,
    Range, VectorRange,
};

pub(crate) fn parse(source: &str) -> Result<EffectDefinition, EffectError> {
    let tokens = tokenize(source);
    let mut cursor = Cursor::new(&tokens);
    let mut effect = EffectDefinition::default();
    while let Some(token) = cursor.next() {
        if token.eq_ignore_ascii_case("repeatdelay") {
            effect.repeat_delay = Some(parse_range(&mut cursor)?);
        } else if let Some(kind) = component_kind(token) {
            cursor.expect("{")?;
            effect.components.push(parse_component(&mut cursor, kind)?);
        } else {
            skip_value(&mut cursor);
        }
    }
    Ok(effect)
}

fn parse_component(cursor: &mut Cursor<'_>, kind: ComponentKind) -> Result<Component, EffectError> {
    let mut component = Component::new(kind);
    while let Some(key) = cursor.next() {
        if key == "}" {
            return Ok(component);
        }
        match key.to_ascii_lowercase().as_str() {
            "count" => component.count = parse_range(cursor)?,
            "life" => component.life = parse_range(cursor)?,
            "delay" => component.delay = parse_range(cursor)?,
            "origin" => component.origin = parse_vector_range(cursor)?,
            "origin2" => component.origin2 = parse_vector_range(cursor)?,
            "velocity" | "vel" => component.velocity = parse_vector_range(cursor)?,
            "acceleration" | "accel" => component.acceleration = parse_vector_range(cursor)?,
            "gravity" => component.gravity = parse_range(cursor)?,
            "rotation" => component.rotation = parse_range(cursor)?,
            "rotationdelta" => component.rotation_delta = parse_range(cursor)?,
            "angle" | "angles" => component.angles = parse_vector_range(cursor)?,
            "angledelta" => component.angle_delta = parse_vector_range(cursor)?,
            "density" => component.density = parse_range(cursor)?,
            "variance" => component.variance = parse_range(cursor)?,
            "cullrange" => component.cull_range = Some(parse_range(cursor)?),
            "size" | "width" => component.size = parse_curve(cursor, component.size)?,
            "size2" | "width2" => {
                component.size2 = parse_curve(cursor, component.size2)?;
                component.size2_authored = true;
            }
            "length" => {
                component.length = parse_curve(cursor, component.length)?;
                component.length_authored = true;
            }
            "alpha" => component.alpha = parse_curve(cursor, component.alpha)?,
            "rgb" => parse_rgb(cursor, &mut component)?,
            "intensity" => {
                let elasticity = parse_range(cursor)?;
                component.elasticity = elasticity;
                component.chaos = elasticity;
                component.chaos_authored = true;
                component.intensity_authored = true;
                if kind == ComponentKind::CameraShake {
                    component.intensity = elasticity;
                }
            }
            "chaos" => {
                component.chaos = parse_range(cursor)?;
                component.chaos_authored = true;
            }
            "bounce" | "elasticity" => {
                let elasticity = parse_range(cursor)?;
                component.elasticity = elasticity;
                component.flags.apply_physics = true;
                component.bounce_authored = true;
                if kind == ComponentKind::CameraShake {
                    component.intensity = elasticity;
                }
            }
            "radius" => component.radius = parse_range(cursor)?,
            "height" if cursor.peek() == Some("{") => {
                component.length = parse_curve(cursor, component.length)?;
                component.length_authored = true;
            }
            "height" => component.height = parse_range(cursor)?,
            "shader" | "shaders" => component.shaders = parse_list(cursor)?,
            "model" | "models" => {
                component.models = parse_list(cursor)?;
                component.flags.use_model = true;
            }
            "playfx" => component.effects = parse_list(cursor)?,
            "emitfx" => {
                component.emit_effects = parse_list(cursor)?;
                component.flags.emit_effect = true;
            }
            "impactfx" => component.impact_effects = parse_list(cursor)?,
            "deathfx" => component.death_effects = parse_list(cursor)?,
            "sounds" => component.sounds = parse_list(cursor)?,
            "flags" => {
                let mut flags = crate::parser_flags::primitive(cursor);
                flags.apply_physics |= component.flags.apply_physics;
                component.flags = flags;
            }
            "spawnflags" => component.spawn_flags = crate::parser_flags::spawn(cursor),
            "name" => skip_name(cursor),
            _ => skip_value(cursor),
        }
    }
    Err(EffectError::Parse("unterminated effect component".into()))
}

fn parse_curve(cursor: &mut Cursor<'_>, mut curve: Curve) -> Result<Curve, EffectError> {
    cursor.expect("{")?;
    while let Some(key) = cursor.next() {
        if key == "}" {
            return Ok(curve);
        }
        match key.to_ascii_lowercase().as_str() {
            "start" => curve.start = parse_range(cursor)?,
            "end" => curve.end = parse_range(cursor)?,
            "parm" | "parms" => curve.parameter = parse_range(cursor)?,
            "flags" => curve.flags = parse_curve_flags(cursor),
            _ => skip_value(cursor),
        }
    }
    Err(EffectError::Parse("unterminated effect curve".into()))
}

fn parse_rgb(cursor: &mut Cursor<'_>, component: &mut Component) -> Result<(), EffectError> {
    cursor.expect("{")?;
    while let Some(key) = cursor.next() {
        if key == "}" {
            return Ok(());
        }
        match key.to_ascii_lowercase().as_str() {
            "start" => component.rgb_start = parse_rgb_ranges(cursor)?,
            "end" => component.rgb_end = parse_rgb_ranges(cursor)?,
            "parm" | "parms" => component.rgb_parameter = parse_range(cursor)?,
            "flags" => component.rgb_flags = parse_curve_flags(cursor),
            _ => skip_value(cursor),
        }
    }
    Err(EffectError::Parse("unterminated rgb curve".into()))
}

fn parse_range(cursor: &mut Cursor<'_>) -> Result<Range, EffectError> {
    let minimum = cursor.number()?;
    let maximum = cursor
        .peek()
        .and_then(|value| value.parse::<f32>().ok())
        .map_or(minimum, |_| cursor.number().unwrap_or(minimum));
    Ok(Range { minimum, maximum })
}

fn parse_vector_range(cursor: &mut Cursor<'_>) -> Result<VectorRange, EffectError> {
    let values = parse_numbers(cursor, 6)?;
    match values.len() {
        3 => Ok(VectorRange {
            minimum: [values[0], values[1], values[2]],
            maximum: [values[0], values[1], values[2]],
        }),
        6 => Ok(VectorRange {
            minimum: [values[0], values[1], values[2]],
            maximum: [values[3], values[4], values[5]],
        }),
        _ => Err(EffectError::Parse(
            "vector property requires 3 or 6 numbers".into(),
        )),
    }
}

fn parse_rgb_ranges(cursor: &mut Cursor<'_>) -> Result<[Range; 3], EffectError> {
    let values = parse_numbers(cursor, 6)?;
    match values.len() {
        3 => Ok(std::array::from_fn(|index| Range {
            minimum: values[index],
            maximum: values[index],
        })),
        6 => Ok(std::array::from_fn(|index| Range {
            minimum: values[index],
            maximum: values[index + 3],
        })),
        _ => Err(EffectError::Parse(
            "rgb property requires 3 or 6 numbers".into(),
        )),
    }
}

fn parse_numbers(cursor: &mut Cursor<'_>, limit: usize) -> Result<Vec<f32>, EffectError> {
    let mut values = Vec::new();
    while values.len() < limit {
        let Some(value) = cursor.peek().and_then(|token| token.parse::<f32>().ok()) else {
            break;
        };
        cursor.next();
        values.push(value);
    }
    if values.is_empty() {
        Err(EffectError::Parse("expected a number".into()))
    } else {
        Ok(values)
    }
}

fn parse_list(cursor: &mut Cursor<'_>) -> Result<Vec<String>, EffectError> {
    cursor.expect("[")?;
    let mut values = Vec::new();
    while let Some(value) = cursor.next() {
        if value == "]" {
            return Ok(values);
        }
        values.push(value.to_owned());
    }
    Err(EffectError::Parse("unterminated effect value list".into()))
}

fn parse_curve_flags(cursor: &mut Cursor<'_>) -> CurveFlags {
    let mut flags = CurveFlags::default();
    while let Some(token) = cursor.peek() {
        match token.to_ascii_lowercase().as_str() {
            "linear" => flags.linear = true,
            "random" => flags.random = true,
            "nonlinear" => flags.modifier = CurveModifier::NonLinear,
            "wave" => flags.modifier = CurveModifier::Wave,
            "clamp" => flags.modifier = CurveModifier::Clamp,
            _ => break,
        }
        cursor.next();
    }
    flags
}

fn component_kind(token: &str) -> Option<ComponentKind> {
    Some(match token.to_ascii_lowercase().as_str() {
        "particle" => ComponentKind::Particle,
        "orientedparticle" => ComponentKind::OrientedParticle,
        "line" => ComponentKind::Line,
        "tail" => ComponentKind::Tail,
        "cylinder" => ComponentKind::Cylinder,
        "electricity" => ComponentKind::Electricity,
        "fxrunner" => ComponentKind::FxRunner,
        "decal" => ComponentKind::Decal,
        "sound" => ComponentKind::Sound,
        "light" => ComponentKind::Light,
        "camerashake" => ComponentKind::CameraShake,
        "flash" => ComponentKind::Flash,
        "emitter" => ComponentKind::Emitter,
        _ if token
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase()) =>
        {
            ComponentKind::Other
        }
        _ => return None,
    })
}

fn skip_name(cursor: &mut Cursor<'_>) {
    while cursor
        .peek()
        .is_some_and(|token| token != "}" && !is_property(token.to_ascii_lowercase().as_str()))
    {
        cursor.next();
    }
}

fn is_property(token: &str) -> bool {
    matches!(
        token,
        "count"
            | "life"
            | "delay"
            | "origin"
            | "origin2"
            | "velocity"
            | "vel"
            | "acceleration"
            | "accel"
            | "gravity"
            | "rotation"
            | "rotationdelta"
            | "cullrange"
            | "size"
            | "width"
            | "size2"
            | "width2"
            | "length"
            | "alpha"
            | "rgb"
            | "intensity"
            | "chaos"
            | "angle"
            | "angles"
            | "angledelta"
            | "density"
            | "variance"
            | "model"
            | "models"
            | "emitfx"
            | "bounce"
            | "elasticity"
            | "radius"
            | "height"
            | "shader"
            | "shaders"
            | "playfx"
            | "impactfx"
            | "deathfx"
            | "sounds"
            | "flags"
            | "spawnflags"
    )
}

fn skip_value(cursor: &mut Cursor<'_>) {
    match cursor.peek() {
        Some("{") => skip_balanced(cursor, "{", "}"),
        Some("[") => skip_balanced(cursor, "[", "]"),
        Some(_) => {
            cursor.next();
            while cursor
                .peek()
                .is_some_and(|token| token.parse::<f32>().is_ok())
            {
                cursor.next();
            }
        }
        None => {}
    }
}

fn skip_balanced(cursor: &mut Cursor<'_>, open: &str, close: &str) {
    let mut depth = 0_u32;
    while let Some(token) = cursor.next() {
        if token == open {
            depth += 1;
        } else if token == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                break;
            }
        }
    }
}

fn tokenize(source: &str) -> Vec<String> {
    let mut clean = String::with_capacity(source.len());
    for line in source.lines() {
        clean.push_str(line.split_once("//").map_or(line, |(before, _)| before));
        clean.push('\n');
    }
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in clean.chars() {
        if character.is_whitespace() || matches!(character, '{' | '}' | '[' | ']') {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            if matches!(character, '{' | '}' | '[' | ']') {
                tokens.push(character.to_string());
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub(crate) struct Cursor<'a> {
    tokens: &'a [String],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(tokens: &'a [String]) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    pub(crate) fn next(&mut self) -> Option<&'a str> {
        let value = self.tokens.get(self.position)?;
        self.position += 1;
        Some(value)
    }

    pub(crate) fn peek(&self) -> Option<&str> {
        self.tokens.get(self.position).map(String::as_str)
    }

    pub(crate) fn peek_after(&self) -> Option<&str> {
        self.tokens.get(self.position + 1).map(String::as_str)
    }

    fn expect(&mut self, expected: &str) -> Result<(), EffectError> {
        let actual = self.next();
        if actual == Some(expected) {
            Ok(())
        } else {
            Err(EffectError::Parse(format!(
                "expected {expected:?}, found {actual:?}"
            )))
        }
    }

    fn number(&mut self) -> Result<f32, EffectError> {
        let token = self
            .next()
            .ok_or_else(|| EffectError::Parse("expected number at end of effect".into()))?;
        token
            .parse()
            .map_err(|_| EffectError::Parse(format!("expected number, found {token:?}")))
    }
}
