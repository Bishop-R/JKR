// Identity below peak 0.9; a C1 rational shoulder above it. RGB shares ONE multiplier:
// unlike independent channel curves, hue and linear RGB ratios cannot drift toward grey.
fn hdr_map(c: vec3<f32>) -> vec3<f32> {
    let peak = max(c.r, max(c.g, c.b));
    if peak <= 0.9 { return c; }
    let excess = peak - 0.9;
    let mapped = 0.9 + 0.1 * excess / (excess + 0.1);
    return c * (mapped / peak);
}
fn hdr_encoded(c: vec3<f32>) -> vec3<f32> {
    let mapped = hdr_map(max(c * HDR_EXPOSURE, vec3(0.0)));
    return select(mapped * 12.92, 1.055 * pow(mapped, vec3(1.0 / 2.4)) - 0.055,
        mapped > vec3(0.0031308));
}
