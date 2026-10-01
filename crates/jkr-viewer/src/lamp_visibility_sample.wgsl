@group(3) @binding(19) var lamp_visibility_map: texture_2d<f32>;
// A compact, smooth filter of binary visibility, never an average of blocker depths.
// Its footprint is fixed in the lamp's map, so walking toward a shadow cannot
// change its shape or switch its softness. Endpoint value and slope are zero.
fn lamp_visibility_weight(distance: f32) -> f32 {
    let t = max(1.0-distance*distance/6.25,0.0);
    return t*t;
}
// Octahedral edges join mirrored tiles of the same sphere, not neighboring lamps.
// Atlas resolutions are powers of two, including the one-texel fallback.
fn lamp_visibility_pixel(pixel: vec2<i32>, resolution: i32) -> vec2<i32> {
    let local = pixel & vec2(resolution-1);
    let reflected = ((pixel.x^pixel.y)&resolution)!=0;
    return select(local,vec2(resolution-1)-local,reflected);
}
// Static geometry visibility remains available even when a lamp has no actor-shadow slot.
fn lamp_static_visibility(lamp: u32, world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let resolution = lamp_grid.offsets.z;
    if resolution == 0u { return 1.0; }
    let source = bitcast<vec4<f32>>(lamp_data[lamp*5u]).xyz;
    let emitter_normal = bitcast<vec4<f32>>(lamp_data[lamp*5u+1u]).xyz;
    let delta = world-(source+emitter_normal*0.5);
    if dot(delta,delta)<1e-6 { return 1.0; }
    let uv = lamp_octa_coordinates(normalize(delta))*0.5+0.5;
    let at = uv*f32(resolution)-0.5;
    let center = vec2<i32>(floor(at+0.5));
    let pitch = resolution+2u;
    let tile = vec2(lamp%lamp_grid.offsets.w,lamp/lamp_grid.offsets.w)*pitch;
    let plane = dot(delta,normal);
    let plane_distance = abs(plane);
    let oriented_normal = normal*select(-1.0,1.0,plane>=0.0);
    var visible = 0.0;
    var total = 0.0;
    for (var y=-2; y<=2; y++) {
        let vertical = lamp_visibility_weight(f32(center.y+y)-at.y);
        for (var x=-2; x<=2; x++) {
        let tap = center+vec2(x,y);
        let weight = lamp_visibility_weight(f32(tap.x)-at.x)*vertical;
        let pixel = lamp_visibility_pixel(tap,i32(resolution));
        let ray = lamp_octa_vector((vec2<f32>(pixel)+0.5)/f32(resolution)*2.0-1.0);
        // Each comparison uses the receiving plane at its own ray. Filtering raw
        // depth first, or sharing one receiver depth, would reintroduce wall stripes.
        let denominator = dot(ray,oriented_normal);
        // Rays outside the receiving hemisphere have no intersection to compare.
        if denominator<1e-4 { continue; }
        let depth = textureLoad(lamp_visibility_map,vec2<i32>(tile)+pixel+1,0).x;
        // The cache includes the half-unit radial bias and the octahedral
        // ray-length conversion. No division or square root is needed per tap.
        visible += weight*select(0.0,1.0,depth*denominator>=plane_distance);
        total += weight;
    } }
    if total<1e-6 { return 1.0; }
    return visible/total;
}
