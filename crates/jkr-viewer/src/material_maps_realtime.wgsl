// Real-time lighting with material maps. The light buffer holds the light the light pass
// evaluated with the geometric normal at half resolution (`sun_realtime_buffer.wgsl`):
// the sun share is moved to the mapped normal per pixel, with the sun visibility the
// buffer kept in alpha. Lamps and probe bounce stay as evaluated for the geometric normal.
fn material_map_lightmap(input: VertexOutput, texel: vec4<f32>) -> vec4<f32> {
    if !realtime_active() { return material_map_baked(input, texel); }
    let surface = material_map_surface;
    let light = buffered_light(input.position, normalize(input.world_normal));
    let sun = shadow.sun.xyz;
    let received = dot(surface.geometric, sun);
    let direct = shadow.realtime.x*shadow.radiance.rgb*shadow.radiance.w*light.a;
    let baked = direct*max(received, 0.0);
    let rest = max(light.rgb - baked, vec3(0.0));
    // The mapped normal takes over toward the terminator's lit side only: nothing reaches a
    // face turned from the sun, and a flat map gives back exactly the buffered light.
    let facing = mix(max(received, 0.0), max(dot(surface.normal, sun), 0.0),
        clamp(4.0*received, 0.0, 1.0));
    var lit = material_map_shade(input.world_position, direct, sun, facing, rest);
    // `jkr_dayDebug` bit 512: no sun highlight or sky rim.
    let highlights = (u32(shadow.realtime.w) & 512u) == 0u;
    if material_map_layout() == 0u {
        // No specular map: the material's existing gloss, on the mapped normal.
        let gloss = select(0.0, stage.emission.w, highlights);
        lit += shadow.realtime.x*(sun_specular(input.world_position, surface.normal, light.a,
            gloss) + sky_reflection(input.world_position, surface.normal, gloss));
    } else if highlights {
        let response = material_map_response();
        material_map_highlight += shadow.realtime.x*response.occlusion
            *mix(vec3(1.0), response.specular, response.metalness)
            *sky_reflection(input.world_position, surface.normal, 1.0 - response.roughness);
    } else {
        material_map_highlight = vec3(0.0);
    }
    return vec4(lit, texel.a);
}
