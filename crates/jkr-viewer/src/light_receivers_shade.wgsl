@group(2) @binding(0) var receiver_world: texture_2d<f32>;
@group(2) @binding(1) var receiver_normal: texture_2d<f32>;
@vertex fn receiver_vertex(@builtin(vertex_index) id: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0));
    return vec4(p[id], 0.0, 1.0);
}
@fragment fn receiver_light(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let world = textureLoad(receiver_world, pixel, 0);
    let normal = textureLoad(receiver_normal, pixel, 0);
    // Cleared texels have a zero normal; every valid receiver stores a unit normal.
    if dot(normal.xyz, normal.xyz) == 0.0 { return vec4(0.0, 0.0, 0.0, 1.0); }
    return realtime_light_from_visibility(world.xyz, normal.xyz, vec2(world.w, normal.w));
}
// The same light with lamps taken from the static cache wherever it is valid.
@group(3) @binding(0) var receiver_cache: texture_2d<f32>;
@group(3) @binding(1) var lamp_cache: texture_2d_array<f32>;
@group(3) @binding(2) var lamp_cache_sampler: sampler;
@fragment fn receiver_light_cached(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let world = textureLoad(receiver_world, pixel, 0);
    let normal = textureLoad(receiver_normal, pixel, 0);
    if dot(normal.xyz, normal.xyz) == 0.0 { return vec4(0.0, 0.0, 0.0, 1.0); }
    let at = textureLoad(receiver_cache, pixel, 0);
    // All four texels of the bilinear footprint must be valid; otherwise evaluate.
    let cached = textureSampleLevel(lamp_cache, lamp_cache_sampler, at.xy,
        i32(max(at.z, 1.0)) - 1, 0.0);
    var lamps: vec3<f32>;
    if at.z >= 1.0 && cached.a >= 0.999 {
        lamps = lamp_light_cached(cached.rgb, world.xyz, normal.xyz);
    } else {
        lamps = lamp_light(world.xyz, normal.xyz);
    }
    return realtime_light_with_lamps(world.xyz, normal.xyz, vec2(world.w, normal.w), lamps);
}
