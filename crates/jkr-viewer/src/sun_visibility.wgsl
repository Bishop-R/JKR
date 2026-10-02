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
// Bilinearly reconstruct separation and blocked coverage, correcting the receiver
// plane at each texel centre. Interpolating raw depths would invent occluders at edges.
fn blockers(map: texture_depth_2d, p: Projection, depth: f32,
    dimensions: vec2<f32>, at: vec2<f32>) -> vec2<f32> {
    let base = vec2<i32>(floor(at));
    let blend = fract(at);
    let lo = clamp(base, vec2(0), vec2<i32>(dimensions)-1);
    let hi = clamp(base + vec2(1), vec2(0), vec2<i32>(dimensions)-1);
    let samples = vec4(textureLoad(map, lo, 0),
        textureLoad(map, vec2(hi.x, lo.y), 0),
        textureLoad(map, vec2(lo.x, hi.y), 0), textureLoad(map, hi, 0));
    let plane = dot(p.gradient, (vec2<f32>(lo) + 0.5)/dimensions - p.uv);
    let step = p.gradient * vec2<f32>(hi-lo)/dimensions;
    let gaps = max(vec4(depth + plane) + vec4(0.0, step.x, step.y, step.x+step.y)
        - samples, vec4(0.0));
    let weights = vec4((1.0-blend.x)*(1.0-blend.y), blend.x*(1.0-blend.y),
        (1.0-blend.x)*blend.y, blend.x*blend.y);
    return vec2(dot(gaps, weights), dot(select(vec4(0.0), weights, gaps > vec4(0.0)), vec4(1.0)));
}
// Same full-texel receiver-plane allowance for both layers; no added depth bias.
fn receiver_depth(p: Projection, dimensions: vec2<f32>, range: f32) -> f32 {
    return p.depth - dot(abs(p.gradient), 1.0/dimensions) - 0.05 / range;
}
// Sample a truncated Gaussian disk rather than an equal-weight disk with a hard rim.
// Fixed angles need no temporal history. A fractional final tap avoids count jumps.
fn reconstruct(map: texture_depth_2d, p: Projection, depth: f32,
    dimensions: vec2<f32>, radius: f32) -> f32 {
    let base_taps = shadow.quality.z;
    let count = clamp(base_taps * radius / 1.5, base_taps, base_taps * 4.0);
    var visibility = 0.0;
    for (var i = 0u; i < u32(ceil(count)); i++) {
        let angle = f32(i) * 2.39996323;
        let quantile = min((f32(i)+0.5)/count, 1.0);
        // Inverse radial CDF of exp(-4*r*r), truncated at radius 1.
        let radial = sqrt(-log(1.0-quantile*0.98168436)*0.25);
        let offset = radial * vec2(cos(angle), sin(angle)) * radius/dimensions;
        visibility += min(count-f32(i), 1.0) * textureSampleCompareLevel(map,
            comparison, p.uv + offset, depth+dot(p.gradient, offset));
    }
    return visibility/count;
}
// Static-world penumbra: separation from the receiver, not camera distance. Search
// in world units so the close cascade cannot clip a tall caster's broad shadow at
// twelve tiny texels. The 24-unit bound limits receiver-plane extrapolation.
fn filtered(map: texture_depth_2d, p: Projection, texel: f32, range: f32,
    footprint: f32) -> f32 {
    let dimensions = vec2<f32>(textureDimensions(map));
    let depth = receiver_depth(p, dimensions, range);
    let reach = max(24.0, footprint*1.41421356);
    var blocked = vec2(0.0);
    for (var i = 0u; i < 16u; i++) {
        let offset = select(disk(i, 16u)*reach/texel, vec2(0.0), i == 0u);
        blocked += blockers(map, p, depth, dimensions, p.uv*dimensions+offset-0.5);
    }
    let gap = blocked.x/max(blocked.y, 1e-6)*range;
    // Gaussian reconstruction retains approximately the disk's contact width.
    let radius = min(max(footprint, gap*0.0093)*1.41421356, reach)/texel;
    return reconstruct(map, p, depth, dimensions, radius);
}
// Moving casters use their own contact reconstruction. They cannot change the
// world's blocker estimate or kernel. Multiplying the separately filtered sun
// visibility approximates their union without brightening/reshaping the broad edge.
fn layered(moving: texture_depth_2d, world: texture_depth_2d, p: Projection,
    texel: f32, range: f32, footprint: f32) -> f32 {
    if shadow.quality.w <= 0.0 { return filtered(moving, p, texel, range, footprint); }
    let static_visibility = filtered(world, p, texel, range, footprint);
    if static_visibility <= 0.0 { return 0.0; }
    let dimensions = vec2<f32>(textureDimensions(moving));
    let dynamic_visibility = reconstruct(moving, p, receiver_depth(p, dimensions, range),
        dimensions, footprint*1.41421356/texel);
    return static_visibility*dynamic_visibility;
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
        // Refine gradually as the camera approaches. The old final-ten-percent
        // band changed close-map sharpness over only 25.6 units at default settings.
        coverage = 1.0-smoothstep(shadow.quality.w*0.75, shadow.quality.w, distance);
        if shadow.close_quality.w > 0.0 {
            close_coverage = 1.0-smoothstep(shadow.close_quality.z*0.5,
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
    // Adjacent maps reconstruct the same world-space width while blending. The
    // width itself changes continuously toward the finer map's footprint, instead
    // of cross-fading an independently sharp edge with an independently soft one.
    let far_texel = select(shadow.quality.x, shadow.far_quality.x, shadow.far_quality.w > 0.0);
    let view_footprint = mix(far_texel, shadow.quality.x, coverage);
    let footprint = 1.5 * mix(view_footprint, shadow.close_quality.x, close_coverage);
    if shadow.far_quality.w > 0.0 && far.inside && coverage < 1.0 &&
        (u32(shadow.realtime.w) & 8u) == 0u {
        visibility = filtered(far_map, far, shadow.far_quality.x, shadow.far_quality.y, footprint);
        known = 1.0;
    }
    if near.inside && coverage > 0.0 && close_coverage < 1.0 {
        let sharp = layered(depth_map, world_map, near, shadow.quality.x, shadow.quality.y, footprint);
        visibility = mix(visibility, sharp, coverage);
        known = max(known, coverage);
    }
    // Bit 64: no close cascade (the near cascade serves the first 256 units too).
    if close_coverage > 0.0 && (debug & 64u) == 0u {
        let finest = layered(close_map, close_world_map, close, shadow.close_quality.x, shadow.close_quality.y, footprint);
        visibility = mix(visibility, finest, close_coverage);
        known = max(known, close_coverage);
    }
    return vec2(visibility, known);
}
