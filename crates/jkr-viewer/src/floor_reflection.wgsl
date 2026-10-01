@group(1) @binding(0) var reflected_scene: texture_2d<f32>;
// Inverse scene dimensions, packed raster scale, and finish roughness.
@group(1) @binding(1) var<uniform> finish: vec4<f32>;
@group(1) @binding(2) var reflected_sampler: sampler;
struct FloorOutput {
    @builtin(position) @invariant position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};
@vertex fn floor_vertex(input: VertexInput) -> FloorOutput {
    return FloorOutput(camera.view_projection*vec4(input.position,1.0),input.position,input.normal);
}
@fragment fn floor_fragment(input: FloorOutput) -> @location(0) vec4<f32> {
    let uv = vec2(1.0-input.position.x*finish.x,input.position.y*finish.y)*finish.z;
    let half_texel = finish.xy*0.5;
    let high = max(half_texel,vec2(finish.z)-half_texel);
    // A fixed angular footprint: soft at every resolution, without noise or LOD switches.
    // Normalized weights keep a constant reflection's radiance unchanged.
    let reach = finish.w*12.0/1080.0*finish.z*vec2(finish.x/finish.y,1.0);
    var reflection = vec3(0.0);
    for (var y = -1; y <= 1; y++) { for (var x = -1; x <= 1; x++) {
        let weight = select(1.0,2.0,x==0)*select(1.0,2.0,y==0)/16.0;
        let at = clamp(uv+vec2<f32>(f32(x),f32(y))*reach,half_texel,high);
        reflection += textureSampleLevel(reflected_scene,reflected_sampler,at,0.0).rgb*weight;
    }}
    let facing = clamp(dot(normalize(input.normal),normalize(camera.camera_position-input.world)),0.0,1.0);
    // A polished finish retains its diffuse pattern and gains reflection at grazing angles.
    let polish = 0.12+0.58*pow(1.0-facing,5.0);
    let strength = polish*(1.0-0.6*finish.w);
    return vec4(reflection,strength);
}
