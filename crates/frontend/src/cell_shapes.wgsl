// Type markings in cell-local coordinates, with a membrane radius of one.
// The shapes follow the silhouettes and markings of Android Cell Lab v103.
fn segment_distance(p: vec2f, a: vec2f, b: vec2f) -> f32 {
    let v = b - a;
    let t = clamp(dot(p - a, v) / max(dot(v, v), 0.0001), 0., 1.);
    return length(p - a - v * t);
}

fn radial_sector(p: vec2f, count: f32) -> vec2f {
    let step = 6.2831853 / count;
    let heading = round(atan2(p.y, p.x) / step) * step;
    let axis = vec2f(cos(heading), sin(heading));
    return vec2f(dot(p, axis), dot(p, vec2f(-axis.y, axis.x)));
}

fn flagellum_distance(p: vec2f) -> f32 {
    let along = clamp((-p.x - 0.89) / 1.11, 0., 1.);
    let wave = sin(along * 8.5) * along * 0.22;
    let width = mix(0.074, 0.023, along);
    return max(abs(p.y - wave) - width, max(p.x + 0.89, -2. - p.x));
}

fn nitro_tuft_distance(p: vec2f) -> f32 {
    let q = vec2f(p.x, abs(p.y));
    let fork = vec2f(1.15, 0.24);
    let stem = segment_distance(q, vec2f(0.91, 0.16), fork) - 0.04;
    let inner = segment_distance(q, fork, vec2f(1.39, 0.19)) - 0.026;
    let outer = segment_distance(q, fork, vec2f(1.30, 0.39)) - 0.026;
    return min(stem, min(inner, outer));
}

fn appendage_distance(kind: u32, p: vec2f) -> f32 {
    // No appendages enter the interior of the membrane.
    if dot(p, p) < 0.75 {
        return 1.;
    }
    switch kind {
        case 2u: { return flagellum_distance(p); }
        case 3u: {
            let q = radial_sector(p, 17.);
            return segment_distance(q, vec2f(0.93, 0.), vec2f(1.18, 0.)) - 0.048;
        }
        case 5u: {
            let q = radial_sector(p, 19.);
            let tip = 1.27 - q.x;
            return max(0.93 - q.x, max(-tip, abs(q.y) - tip * 0.35));
        }
        case 6u: { return nitro_tuft_distance(radial_sector(p, 5.)); }
        default: { return 1.; }
    }
}

fn chloroplast_distance(p: vec2f) -> f32 {
    // Vary placement, elongation, and size to avoid a regular dotted ring.
    let patches = array<vec4f, 8>(
        vec4f(-0.57, -0.36, 0.10, 0.22),
        vec4f(-0.23, -0.64, 0.17, 0.09),
        vec4f( 0.35, -0.56, 0.20, 0.11),
        vec4f( 0.69, -0.19, 0.10, 0.16),
        vec4f( 0.54,  0.46, 0.13, 0.18),
        vec4f( 0.09,  0.65, 0.17, 0.10),
        vec4f(-0.42,  0.47, 0.16, 0.21),
        vec4f(-0.67,  0.16, 0.085, 0.075),
    );
    var distance = 1.;
    for (var i = 0u; i < 8u; i += 1u) {
        let spot = patches[i];
        let offset = p - spot.xy;
        let shear = select(-0.36, 0.36, i % 2u == 0u);
        let q = vec2f(offset.x + offset.y * shear, offset.y) / spot.zw;
        distance = min(distance, (length(q) - 1.) * min(spot.z, spot.w));
    }
    return distance;
}

fn marking_coverage(distance: f32, aa: f32) -> f32 {
    return 1. - smoothstep(-aa, aa, distance);
}
