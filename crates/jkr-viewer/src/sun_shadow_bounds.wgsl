// One 64x64 tile per workgroup. Adjacent lanes load adjacent texels; each
// invocation reduces sixteen depths before the shared reduction. No depth is
// approximated, and clamping repeats edge texels in a partial final tile.
@group(0) @binding(0) var source_depth: texture_depth_2d;
@group(0) @binding(1) var bounds: texture_storage_2d_array<rg32float, write>;
@group(0) @binding(2) var<uniform> parameters: vec4<u32>;
var<workgroup> extrema: array<vec2<f32>, 256>;
@compute @workgroup_size(16, 16, 1)
fn build(@builtin(workgroup_id) block: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(local_invocation_index) index: u32) {
    let pixel = vec2<i32>(block.xy*64u + local.xy);
    let last = vec2<i32>(textureDimensions(source_depth)) - 1;
    var low = 1.0;
    var high = 0.0;
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let depth = textureLoad(source_depth, min(pixel + vec2(x,y)*16, last), 0);
            low = min(low, depth);
            high = max(high, depth);
        }
    }
    extrema[index] = vec2(low, high);
    for (var stride = 128u; stride > 0u; stride /= 2u) {
        workgroupBarrier();
        if index < stride {
            extrema[index] = vec2(min(extrema[index].x, extrema[index+stride].x),
                max(extrema[index].y, extrema[index+stride].y));
        }
    }
    if index == 0u {
        textureStore(bounds, vec2<i32>(block.xy), i32(parameters.x), vec4(extrema[0],0.0,0.0));
    }
}
