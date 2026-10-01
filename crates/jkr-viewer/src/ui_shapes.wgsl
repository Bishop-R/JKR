struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) start_color: vec4<f32>,
    @location(4) end_color: vec4<f32>,
    @location(5) parameters: vec2<f32>,
    @location(6) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) start_color: vec4<f32>,
    @location(3) end_color: vec4<f32>,
    @location(4) parameters: vec2<f32>,
    @location(5) uv: vec2<f32>,
}

@group(0) @binding(0) var icon_texture: texture_2d<f32>;
@group(0) @binding(1) var icon_sampler: sampler;

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4(input.position, 0.0, 1.0);
    output.local = input.local;
    output.size = input.size;
    output.start_color = input.start_color;
    output.end_color = input.end_color;
    output.parameters = input.parameters;
    output.uv = input.uv;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.parameters.y > 1.5 {
        return textureSample(icon_texture, icon_sampler, input.uv) * input.start_color;
    }
    let radius = input.parameters.x;
    // A negative parameters.y is a rounded border: its magnitude is the ring width.
    let ring = -input.parameters.y;
    var coverage = 1.0;
    if radius > 0.0 || ring > 0.0 {
        // Signed distance to the rounded outline in pixels; the edge is
        // anti-aliased by fading alpha across one pixel of it (and across
        // the inner edge of a ring) instead of a hard discard.
        let pixel = input.local * input.size;
        let half_size = input.size * 0.5;
        let q = abs(pixel - half_size) - (half_size - vec2(radius));
        let distance = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
        coverage = clamp(0.5 - distance, 0.0, 1.0);
        if ring > 0.0 {
            coverage *= clamp(distance + ring + 0.5, 0.0, 1.0);
        }
        if coverage <= 0.0 {
            discard;
        }
    }
    let amount = select(input.local.x, input.local.y, input.parameters.y > 0.5);
    let color = mix(input.start_color, input.end_color, amount);
    return vec4(color.rgb, color.a * coverage);
}
