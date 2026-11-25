// Fullscreen composite shaders for WBOIT resolve and depth-peeling layer blending.

@vertex
fn vs_fullscreen(@builtin(vertex_index) vid: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(vid & 1u) * 4 - 1);
    let y = f32(i32(vid & 2u) * 2 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@group(0) @binding(0) var accum_tex: texture_2d<f32>;
@group(0) @binding(1) var reveal_tex: texture_2d<f32>;

@fragment
fn fs_wboit(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(pos.xy);
    let accum = textureLoad(accum_tex, px, 0);
    let reveal = clamp(textureLoad(reveal_tex, px, 0).r, 0.0, 1.0);
    if reveal > 0.9999 || accum.a <= 1e-6 {
        discard;
    }
    let avg_color = clamp(accum.rgb, vec3<f32>(0.0), vec3<f32>(65000.0)) / max(accum.a, 1e-6);
    let out_a = 1.0 - reveal;
    return vec4<f32>(avg_color * out_a, out_a);
}

@group(0) @binding(0) var layer_tex: texture_2d<f32>;

@fragment
fn fs_copy(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(layer_tex, vec2<i32>(pos.xy), 0);
}
