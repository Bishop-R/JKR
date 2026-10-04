// Real-time lighting for the no-bake mode: sun through the cascades plus probe bounce, in
// display units where a sunlit white surface is about one, scaled by jkr_lightScale. `shadow.radiance` is the sun radiance,
// `shadow.ambient` the sky radiance, `shadow.realtime` = (scale, buffer scale x, y, unused).
// Direct sun, lamps and bounce at a point (rgb) and the sun's visibility there (a), for
// the highlight the caller adds with its material's gloss.
fn realtime_light(world: vec3<f32>, normal: vec3<f32>, eye: vec3<f32>,
    forward: vec3<f32>) -> vec4<f32> {
    return realtime_light_from_visibility(world, normal, sun_visibility(world, normal, eye, forward));
}
// Geometry computes derivative-dependent sun visibility before storing the winning
// receiver. Lamp, probe and contact evaluation can then run once per final pixel.
fn realtime_light_from_visibility(world: vec3<f32>, normal: vec3<f32>, visibility: vec2<f32>) -> vec4<f32> {
    return realtime_light_with_lamps(world, normal, visibility, lamp_light(world, normal));
}
// `lamps` is the receiver's lamp irradiance, evaluated directly or taken from the cache.
fn realtime_light_with_lamps(world: vec3<f32>, normal: vec3<f32>, visibility: vec2<f32>,
    lamps: vec3<f32>) -> vec4<f32> {
    // `jkr_dayDebug` bits leave terms out, live, to name the one behind an artefact.
    let debug = u32(shadow.realtime.w);
    var sample = visibility;

    // Screen-space contact shadows (`sun_contact.wgsl`) only where the cascades say lit.
    if sample.x > 0.02 && (debug & 2u) == 0u { sample.x *= contact_shadow(world, normal); }
    let facing = max(dot(normal, shadow.sun.xyz), 0.0);
    let direct = shadow.radiance.rgb*shadow.radiance.w*facing*mix(1.0, sample.x, sample.y);
    // The map's lamps, direct and per pixel; their bounce rides the probes.
    var local = lamp_response(lamps);

    if (debug & 4u) != 0u { local = vec3(0.0); }
    // Bounce is gathered from the probes at the point itself (per light-buffer texel for
    // the world, per pixel for models).
    var indirect = shadow.ambient.rgb*0.5;
    if probes.counts.w != 0u { indirect = probe_irradiance(world, normal)/3.1415927; }
    if (debug & 16u) != 0u { indirect = shadow.ambient.rgb*0.5; }
    // Gain is applied when shading, never fed back into the probe bounce solver.
    // Screen-space obscurance affects sky/bounce; fill can retain a bounded floor.
    var occlusion = 1.0;
    if (debug & 1u) == 0u { occlusion = ambient_occlusion(world, normal); }
    indirect = readable_indirect(direct + local, indirect, occlusion, shadow.fill,
        shadow.readability.xy);
    // Bit 128: the sun's visibility itself as the light (grey where the maps say shadow).
    if (debug & 128u) != 0u { return vec4(vec3(0.15 + 0.85*sample.x), 1.0); }
    return vec4(shadow.realtime.x*(direct + local + indirect), mix(1.0, sample.x, sample.y));
}
