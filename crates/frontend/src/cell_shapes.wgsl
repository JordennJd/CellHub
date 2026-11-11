// Type markings in cell-local coordinates, with a membrane radius of one.
// The shapes follow the silhouettes and markings of Android Cell Lab v103.
fn segment_distance(p: vec2f, a: vec2f, b: vec2f) -> f32 {
    let v = b - a;
    let t = clamp(dot(p - a, v) / max(dot(v, v), 0.0001), 0., 1.);
    return length(p - a - v * t);
}

fn radial_sector(p: vec2f, count: f32) -> vec2f {
    let step = 6.2831853 / count;
    let safe_x = select(p.x, 0.0001, dot(p, p) < 0.00000001);
    let heading = round(atan2(p.y, safe_x) / step) * step;
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
        case 9u: {
            let q = radial_sector(p, 33.);
            return max(0.98 - q.x, max(q.x - 1.14, abs(q.y) - (1.14 - q.x) * 0.28));
        }
        case 10u: {
            let q = vec2f(abs(p.x), p.y);
            var distance = 1.;
            for (var i = 0u; i < 7u; i += 1u) {
                let angle = (f32(i) - 3.) * 0.105;
                let tip = vec2f(cos(angle), sin(angle)) * 1.42;
                distance = min(distance, segment_distance(q, vec2f(0.96, 0.), tip) - 0.021);
            }
            return distance;
        }
        case 11u: {
            let q = radial_sector(p, 9.);
            let stem = segment_distance(q, vec2f(0.96, 0.), vec2f(1.10, 0.)) - 0.022;
            let branch = segment_distance(vec2f(q.x, abs(q.y)), vec2f(1.10, 0.), vec2f(1.25, 0.105)) - 0.018;
            return min(stem, branch);
        }
        case 17u: {
            let q = radial_sector(p, 28.);
            let t = clamp((q.x - 0.97) / 0.30, 0., 1.);
            let sweep = select(-0.11, 0.11, p.y >= 0.) * t * t;
            return max(abs(q.y - sweep) - 0.018, max(0.97 - q.x, q.x - 1.27));
        }
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

fn neural_compartments_distance(p: vec2f) -> f32 {
    let q = (radial_sector(p, 6.) - vec2f(0.58, 0.)) / vec2f(0.20, 0.115);
    return abs(length(q) - 1.) * 0.115 - 0.024;
}

fn interior_distance(kind: u32, p: vec2f) -> f32 {
    switch kind {
        case 8u: { return abs(length(p - vec2f(-0.18, 0.)) - 0.61) - 0.045; }
        case 10u, 11u, 13u: { return neural_compartments_distance(p); }
        case 12u: {
            var distance = 1.;
            for (var i = 0u; i < 4u; i += 1u) {
                let y = (f32(i) - 1.5) * 0.36;
                distance = min(distance, abs(p.y - y - 0.12 * p.x * p.x) - 0.045);
            }
            return max(distance, length(p) - 0.84);
        }
        case 14u: {
            let outer = abs(length(radial_sector(p, 9.) - vec2f(0.70, 0.)) - 0.075) - 0.025;
            let inner = abs(length(radial_sector(p, 5.) - vec2f(0.38, 0.)) - 0.045) - 0.022;
            return min(outer, inner);
        }
        case 16u: {
            let safe_x = select(p.x, 0.0001, dot(p, p) < 0.00000001);
            let angle = atan2(p.y, safe_x);
            return abs(length(p) - (0.75 + 0.075 * cos(angle * 7.))) - 0.035;
        }
        default: { return 1.; }
    }
}

fn nucleus_distance(kind: u32, p: vec2f) -> f32 {
    if kind == 8u {
        return length(p - vec2f(0.55, 0.)) - 0.16;
    }
    if kind == 16u {
        return max(length(p) - 0.24, p.x + 0.015);
    }
    return length(p) - 0.21;
}

fn stem_lobe_distance(p: vec2f) -> f32 {
    return max(length(p - vec2f(0.10, 0.)) - 0.66, 0.45 - length(p - vec2f(-0.10, 0.)));
}

fn marking_coverage(distance: f32, aa: f32) -> f32 {
    return 1. - smoothstep(-aa, aa, distance);
}
