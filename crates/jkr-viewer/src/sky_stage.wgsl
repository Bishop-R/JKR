// Camera-relative Q3 outer sky box and sky-surface depth mask.
struct Camera {
    view_projection: mat4x4<f32>,
    camera_position: vec3<f32>,
    shader_time: f32,
};

// What one unit of the sky's image is in the scene's light units: one for a scene shown as it
// is lit, less for a scene whose final pass exposes it (the day program applies it).
override sky_radiance: f32 = 1.0;
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var sky_images: texture_2d_array<f32>;
@group(1) @binding(1) var sky_sampler: sampler;

struct BoxInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: f32,
};

struct BoxOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    // Camera-relative box position: the view direction of the texel.
    @location(2) direction: vec3<f32>,
};

@vertex
fn box_vertex(input: BoxInput) -> BoxOutput {
    var output: BoxOutput;
    let world = camera.camera_position + input.position;
    var clip = camera.view_projection * vec4<f32>(world, 1.0);
    clip.z = clip.w;
    output.position = clip;
    output.uv = input.uv;
    output.layer = u32(input.layer);
    output.direction = input.position;
    return output;
}

@fragment
fn box_fragment(input: BoxOutput) -> @location(0) vec4<f32> {
    return textureSample(sky_images, sky_sampler, input.uv, input.layer);
}

struct MaskInput {
    @location(0) position: vec3<f32>,
};

@vertex
fn mask_vertex(input: MaskInput) -> @builtin(position) vec4<f32> {
    return camera.view_projection * vec4<f32>(input.position, 1.0);
}

@fragment
fn mask_fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}

// The box on the sky faces themselves (`draw_faces_shaded`): the texel the box would show
// along the fragment's view direction. Axis and (s, t) invert `make_sky_vec`.
struct FaceOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) direction: vec3<f32>,
};

@vertex
fn face_vertex(input: MaskInput) -> FaceOutput {
    return FaceOutput(camera.view_projection * vec4<f32>(input.position, 1.0),
        input.position - camera.camera_position);
}

// (u, v, layer) of the box texel seen along `d`.
fn box_coordinates(d: vec3<f32>) -> vec3<f32> {
    let a = abs(d);
    var st = vec2(0.0);
    var layer = 0.0;
    if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 { st = vec2(-d.y, d.z)/a.x; layer = 0.0; } else { st = vec2(d.y, d.z)/a.x; layer = 1.0; }
    } else if a.y >= a.z {
        if d.y > 0.0 { st = vec2(d.x, d.z)/a.y; layer = 2.0; } else { st = vec2(-d.x, d.z)/a.y; layer = 3.0; }
    } else {
        if d.z > 0.0 { st = vec2(-d.y, -d.x)/a.z; layer = 4.0; } else { st = vec2(-d.y, d.x)/a.z; layer = 5.0; }
    }
    return vec3((st.x + 1.0)*0.5, 1.0 - (st.y + 1.0)*0.5, layer);
}
// BOX_COORDINATES_END
fn box_texel(d: vec3<f32>) -> vec4<f32> {
    let at = box_coordinates(d);
    // The box magnifies its faces; an explicit level also keeps the axis seams clean.
    return textureSampleLevel(sky_images, sky_sampler, at.xy, i32(at.z), 0.0);
}

@fragment
fn face_fragment(input: FaceOutput) -> @location(0) vec4<f32> {
    let texel = box_texel(input.direction);
    // A missing box face has no geometry: whatever was there stays.
    if texel.a <= 0.0 { discard; }
    return texel;
}
