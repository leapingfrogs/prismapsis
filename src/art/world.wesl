// Theme art with each hue held back until it has been restored. Mirrors
// `art::filtered`: a pixel's hue decides how much colour it may show, and
// the rest is pulled towards grey. Vertex colours carry a tint in rgb and
// the tileset layer in alpha.
//
// Lights then brighten what's near them in their own colour, after the
// filter, so a red beam tints a wall that is still grey. Each light is a
// segment (a point when both ends meet) with a colour and a radius; how
// strongly this art takes light is in `wash.w`.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> wash: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var tiles: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var tiles_sampler: sampler;

struct Lights {
    // Each light's ends: xy and zw.
    segments: array<vec4<f32>, 8>,
    // Each light's colour times its strength, and its radius in w.
    colours: array<vec4<f32>, 8>,
    count: u32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(3) var<uniform> lights: Lights;

// The light reaching `p`: each light falls off smoothly to nothing at its
// radius from the nearest point of its segment.
fn light_at(p: vec2<f32>) -> vec3<f32> {
    var total = vec3<f32>(0.0);
    for (var i = 0u; i < min(lights.count, 8u); i = i + 1u) {
        let segment = lights.segments[i];
        let a = segment.xy;
        let ab = segment.zw - a;
        let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 0.0001), 0.0, 1.0);
        let d = distance(p, a + ab * t);
        let colour = lights.colours[i];
        let falloff = clamp(1.0 - d / max(colour.w, 1.0), 0.0, 1.0);
        total = total + colour.rgb * falloff * falloff;
    }
    return total;
}

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

// How much light lifts a surface regardless of how bright it is.
const LIFT: f32 = 0.05;

fn hue_degrees(c: vec3<f32>) -> f32 {
    let top = max(c.r, max(c.g, c.b));
    let range = top - min(c.r, min(c.g, c.b));
    if range <= 1e-6 {
        return 0.0;
    }
    var sector: f32;
    if top == c.r {
        sector = (c.g - c.b) / range;
        sector = sector - 6.0 * floor(sector / 6.0);
    } else if top == c.g {
        sector = (c.b - c.r) / range + 2.0;
    } else {
        sector = (c.r - c.g) / range + 4.0;
    }
    return sector * 60.0;
}

fn allowance(hue: f32) -> f32 {
    let r = wash.r;
    let g = wash.g;
    let b = wash.b;
    var anchors = array<f32, 7>(r, min(r, g), g, min(g, b), b, min(b, r), r);
    let x = (hue - 360.0 * floor(hue / 360.0)) / 60.0;
    let i = min(u32(floor(x)), 5u);
    let t = x - f32(i);
    return mix(anchors[i], anchors[i + 1u], t);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let layer = i32(round(in.color.a));
    let texel = textureSample(tiles, tiles_sampler, in.uv, layer);
    let colour = texel.rgb * in.color.rgb;
    let grey = vec3<f32>(dot(colour, LUMA));
    let shown = mix(grey, colour, allowance(hue_degrees(colour)));
    // Light brightens what's there and lifts even dark surfaces a little,
    // so a beam shows on shadowed rock.
    let light = light_at(in.world_position.xy) * wash.w;
    // Never past white: light brightens walls but doesn't make them glow.
    let lit = min(shown + light * (shown + vec3<f32>(LIFT)), vec3<f32>(1.0));
    return vec4<f32>(lit, texel.a);
}
