@group(1) @binding(0) var portal_image: texture_2d<f32>;
@group(1) @binding(1) var<uniform> portal_control: vec4<f32>;
@vertex fn portal_main(vertex: VertexInput,
    instance: InstanceInput) -> @builtin(position) vec4<f32> {
    return camera.view_projection * vec4(instance_position(vertex, instance), 1.0);
}
@fragment fn sample_view(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = textureDimensions(portal_image);
    var pixel = vec2<i32>(position.xy);
    if portal_control.x != 0.0 { pixel.x = i32(size.x) - pixel.x - 1; }
    return textureLoad(portal_image, pixel, 0);
}
