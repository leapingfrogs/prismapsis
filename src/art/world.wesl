// Theme art with each hue held back until it has been restored. Mirrors
// `art::filtered`: a pixel's hue decides how much colour it may show, and
// the rest is pulled towards grey. Vertex colours carry a tint in rgb and
// the tileset layer in alpha.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> wash: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var tiles: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var tiles_sampler: sampler;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

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
    return vec4<f32>(shown, texel.a);
}
