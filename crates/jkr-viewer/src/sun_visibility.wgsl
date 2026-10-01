// Shared by the existing world receiver and main-view model diffuse lighting.
// Three cascades: the close fit (`close_map`, finest texels) nearest the camera, the view
// fit (`depth_map`) ahead of it, and a map-wide world-only far cascade (`far_map`) behind
// it, so no surface is ever assumed sunlit merely because it lies past a fit. The `.w`
// of each extra quality vector flags that cascade's presence.
struct Projection { uv: vec2<f32>, depth: f32, gradient: vec2<f32>, inside: bool };
fn disk(i: u32, count: u32) -> vec2<f32> {
    let angle = f32(i) * 2.39996323;
    return sqrt((f32(i)+0.5)/f32(count)) * vec2(cos(angle), sin(angle));
}
// Sub-texel normal offset plus receiver-plane derivatives; no world-space multi-unit lift.
// Derivatives are evaluated here, before any divergent rejection.
fn project(vp: mat4x4<f32>, texel: f32, world: vec3<f32>, normal: vec3<f32>,
    slope: bool) -> Projection {
    // A tenth of a texel along the normal. Every unit of bias lets sun through at a
    // crease where wall and floor meet: the blocker's and the receiver's depths agree at
    // the base, so the sunlit line along the base is as wide as the total bias over the
    // tangent of the sun's elevation. The receiver-plane term below covers the surface's
    // own slope; this only keeps the compare off the surface itself.
    let clip = vp * vec4(world + normal * texel * 0.1, 1.0);
    let uv = clip.xy * vec2(0.5, -0.5) + 0.5;
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let dz = vec2(dpdx(clip.z), dpdy(clip.z));
    let determinant = dx.x*dy.y-dx.y*dy.x;
    var gradient = vec2(0.0);
    if slope && abs(determinant) > 1e-12 {
        gradient = vec2(dy.y*dz.x-dx.y*dz.y, dx.x*dz.y-dy.x*dz.x)/determinant;
    }
    let inside = all(uv > vec2(0.0)) && all(uv < vec2(1.0)) && clip.z > 0.0 && clip.z < 1.0;
    return Projection(uv, clip.z, gradient, inside);
}
// Blocker search then contact-hardening PCF on one cascade; `range` is its depth extent.
fn filtered(map: texture_depth_2d, p: Projection, texel: f32, range: f32) -> f32 {
    let dimensions = vec2<f32>(textureDimensions(map));
    // A linear comparison samples the four surrounding texel centers. At an
    // arbitrary sub-texel position, a center can be almost one texel away on
    // either axis. Half a texel only covers the midpoint and lets planar
    // receivers shadow themselves as the sample moves across the footprint.
    let depth = p.depth - dot(abs(p.gradient), 1.0/dimensions) - 0.05 / range;
    // The blocker search and the penumbra reach at most 24 world units: the receiver
    // plane is extrapolated that far, and on the far cascade's 4 to 8 unit texels a
    // twelve texel reach was 96 units, enough for curved patches and steps to read as
    // blockers of their own neighbours (false penumbrae, dotted, at a distance).
    let reach = clamp(24.0/texel, 1.0, 12.0);
    var sum = 0.0;
    var blockers = 0.0;
    for (var i = 0u; i < 16u; i++) {
        // Cover the full possible penumbra, with a center sample for thin/contact blockers.
        let offset = select(disk(i, 16u)*reach, vec2(0.0), i == 0u);
        let pixel = vec2<i32>((p.uv + offset/dimensions)*dimensions);
        let sample = textureLoad(map, clamp(pixel, vec2(0), vec2<i32>(dimensions)-1), 0);
        let adjusted = sample-dot(p.gradient, offset/dimensions);
        if adjusted < depth { sum += adjusted; blockers += 1.0; }
    }
    // Parallel sunlight: penumbra grows with receiver/blocker separation, no extra pass.
    // The hardware comparison already filters one texel. An extra fixed disk
    // radius turns contact into a world-space penumbra that grows with cascade
    // texel size, even when blocker and receiver touch. Only their separation
    // should widen the sun's penumbra.
    var radius = 0.0;
    if blockers > 0.0 {
        let gap = max(depth - sum/blockers, 0.0) * range;
        radius = clamp(gap * 0.0093 / texel, 0.0, reach);
    }
    var visibility = 0.0;
    let taps = u32(shadow.quality.z);
    for (var i = 0u; i < taps; i++) {
        visibility += textureSampleCompareLevel(map, comparison,
            p.uv + disk(i, taps)*radius/dimensions,
            depth+dot(p.gradient, disk(i, taps)*radius/dimensions));
    }
    return visibility/f32(taps);
}
// Visibility and how much of it is known, respectively. The finest cascade covering the
// point answers; across each fit's axial fade band neighbouring cascades cross-blend;
// beyond every fit the far cascade alone. Without a far cascade, unknown air stays sunlit.
fn sun_visibility(world: vec3<f32>, normal: vec3<f32>, eye: vec3<f32>,
    forward: vec3<f32>) -> vec2<f32> {
    return sun_visibility_masked(world, normal, eye, forward, false);
}
// Baked receivers can already be completely occluded. Keep projection derivatives
// in uniform control flow, then omit filters whose result is multiplied by zero.
fn sun_visibility_masked(world: vec3<f32>, normal: vec3<f32>, eye: vec3<f32>,
    forward: vec3<f32>, fully_occluded: bool) -> vec2<f32> {
    // `jkr_dayDebug` bit 32: no sun shadow maps at all (everything the sun faces is lit).
    let debug = u32(shadow.realtime.w);
    if (debug & 32u) != 0u { return vec2(1.0, 1.0); }
    var coverage = 1.0;
    var close_coverage = 0.0;
    let slope = shadow.quality.w > 0.0;
    if slope {
        let distance = dot(world-eye, forward);
        coverage = 1.0-smoothstep(shadow.quality.w*0.9, shadow.quality.w, distance);
        if shadow.close_quality.w > 0.0 {
            close_coverage = 1.0-smoothstep(shadow.close_quality.z*0.9,
                shadow.close_quality.z, distance);
        }
    }
    let close = project(shadow.close_vp, shadow.close_quality.x, world, normal, slope);
    let near = project(shadow.vp, shadow.quality.x, world, normal, slope);
    let far = project(shadow.far_vp, shadow.far_quality.x, world, normal, slope);
    if fully_occluded && (debug & 1024u) == 0u { return vec2(0.0, 1.0); }
    var visibility = 1.0;
    var known = 0.0;
    if !close.inside { close_coverage = 0.0; }
    if shadow.far_quality.w > 0.0 && far.inside && coverage < 1.0 &&
        (u32(shadow.realtime.w) & 8u) == 0u {
        visibility = filtered(far_map, far, shadow.far_quality.x, shadow.far_quality.y);
        known = 1.0;
    }
    if near.inside && coverage > 0.0 && close_coverage < 1.0 {
        let sharp = filtered(depth_map, near, shadow.quality.x, shadow.quality.y);
        visibility = mix(visibility, sharp, coverage);
        known = max(known, coverage);
    }
    // Bit 64: no close cascade (the near cascade serves the first 256 units too).
    if close_coverage > 0.0 && (debug & 64u) == 0u {
        let finest = filtered(close_map, close, shadow.close_quality.x, shadow.close_quality.y);
        visibility = mix(visibility, finest, close_coverage);
        known = max(known, close_coverage);
    }
    return vec2(visibility, known);
}
