// Point lights on surfaces: the block's layout, the grid that finds a fragment's candidates,
// and the two ways a light reaches a surface. The including program binds `point_lights`
// (a `PointLightBlock`), and defines `realtime_active()` and a `VertexOutput` with
// `world_position` and `world_normal`.

struct PointLight {
    origin_radius: vec4<f32>,
    color: vec4<f32>,
};

// CPU layout and dimensions: point_light_grid.rs. One bit per original light index.
struct PointLightGrid {
    low: vec4<f32>,
    high: vec4<f32>,
    scale: vec4<f32>,
    control: vec4<u32>,
    masks: array<vec4<u32>,432>,
};
struct PointLightBlock {
    lights: array<PointLight, 32>,
    metadata: vec4<u32>,
    grid: PointLightGrid,
};

// Bounds are shared by all cameras. Only finite surface laws use this mask;
// the legacy model inverse-square light has infinite support and keeps its loop.
fn finite_point_mask(world: vec3<f32>) -> u32 {
    let control = point_lights.grid.control;
    if control.y != 0u || control.x == 0u { return control.x; }
    let low = point_lights.grid.low.xyz;
    let high = point_lights.grid.high.xyz;
    if any(world < low) || any(world > high) { return 0u; }
    let cell = vec3<u32>(clamp((world-low)*point_lights.grid.scale.xyz,vec3(0.0),vec3(11.0)));
    let index = cell.x+(cell.y+cell.z*12u)*12u;
    return point_lights.grid.masks[index>>2u][index&3u];
}
// The fragment's candidate point lights (`stage_fragment` sets it): emitted light and
// dynamic-light modulation share one grid lookup instead of making one each.
var<private> fragment_point_mask: u32;
// Emitted light illuminates albedo even where the existing environment is dark.
// This mask keeps the old point-light behavior for every unmarked source.
fn emitted_light(input: VertexOutput) -> vec3<f32> {
    if point_lights.metadata.w == 0u || !realtime_active() { return vec3(0.0); }
    var result = vec3(0.0);
    let normal = normalize(input.world_normal);
    var remaining = fragment_point_mask & point_lights.metadata.w;
    while remaining != 0u {
        let index = firstTrailingBit(remaining);
        remaining &= remaining - 1u;
        let light = point_lights.lights[index];
        if light.color.w == 0.0 { continue; }
        let delta = light.origin_radius.xyz-input.world_position;
        // Outside this finite light's box, its radial contribution is exactly zero.
        // Reject before square roots/normalization; no screen- or distance-quality LOD.
        if any(abs(delta) > vec3(max(light.origin_radius.w,0.001))) { continue; }
        let distance = length(delta);
        let falloff = max(1.0-distance/max(light.origin_radius.w,0.001),0.0);
        let facing = max(dot(normal,delta/max(distance,0.001)),0.0);
        result += light.color.rgb*falloff*falloff*facing;
    }
    return result;
}

// rd-vanilla marks affected surfaces in R_DlightBmodel/R_MarkLights and adds
// a projected GL_DST_COLOR,GL_ONE pass in RB_ProjectDlightTexture
// (tr_light.cpp:44-111; tr_world.cpp:253-360; tr_shade.cpp:761-1091).
// We retain its additive modulation in the existing opaque pass: radial
// attenuation and facing replace the legacy 2-D projection texture.
fn dynamic_light_modulation(input: VertexOutput) -> vec3<f32> {
    var result = vec3(0.0);
    let normal = normalize(input.world_normal);
    var remaining = fragment_point_mask;
    if realtime_active() { remaining &= ~point_lights.metadata.w; }
    while remaining != 0u {
        let index = firstTrailingBit(remaining);
        remaining &= remaining - 1u;
        let light = point_lights.lights[index];
        if light.color.w > 0.0 && realtime_active() { continue; }
        let delta = light.origin_radius.xyz - input.world_position;
        if any(abs(delta) > vec3(light.origin_radius.w)) { continue; }
        let distance = length(delta);
        if distance < light.origin_radius.w && distance > 0.0001 {
            let attenuation = 1.0 - distance / light.origin_radius.w;
            let facing = max(dot(normal, delta / distance), 0.0);
            result += light.color.rgb * attenuation * facing;
        }
    }
    return result;
}
