// Fullscreen ChimeraX-style silhouette border composite over mesh ID + depth textures.
struct OutlineData {
    // `rgb` = sRGB border color, `a` = border alpha.
    colors: array<vec4<f32>, 256>,
    // `x` = border width (px); `widths[0].y` = max active width across all IDs.
    widths: array<vec4<f32>, 256>,
};

@group(0) @binding(0) var outline_id_tex: texture_2d<f32>;
@group(0) @binding(1) var outline_depth_tex: texture_depth_2d;
@group(0) @binding(2) var<uniform> outline: OutlineData;
@group(0) @binding(3) var opaque_depth_tex: texture_depth_2d;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    let x = f32((vid << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vid & 2u) * 2.0 - 1.0;
    var out: VsOut;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

fn srgb_decode_channel(c: f32) -> f32 {
    let x = clamp(c, 0.0, 1.0);
    if x <= 0.04045 {
        return x / 12.92;
    }
    return pow((x + 0.055) / 1.055, 2.4);
}

fn srgb_decode(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        srgb_decode_channel(rgb.x),
        srgb_decode_channel(rgb.y),
        srgb_decode_channel(rgb.z),
    );
}

@fragment
fn fs_outline(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let dims = vec2<i32>(textureDimensions(outline_id_tex));
    let id_c = i32(round(textureLoad(outline_id_tex, px, 0).r * 255.0));
    let z_c = textureLoad(outline_depth_tex, px, 0);
    let op_z = textureLoad(opaque_depth_tex, px, 0);

    let max_w = clamp(outline.widths[0].y, 0.0, 6.0);
    let r = i32(ceil(max_w + 0.5));
    if r <= 0 {
        discard;
    }

    var best_cov = 0.0;
    var best_z = 2.0;
    var best_id = 0;

    for (var dy = -r; dy <= r; dy = dy + 1) {
        for (var dx = -r; dx <= r; dx = dx + 1) {
            if dx == 0 && dy == 0 {
                continue;
            }
            let dist = length(vec2<f32>(f32(dx), f32(dy)));
            if dist > max_w + 0.5 {
                continue;
            }
            let n_px = clamp(px + vec2<i32>(dx, dy), vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
            let id_n = i32(round(textureLoad(outline_id_tex, n_px, 0).r * 255.0));
            if id_n <= 0 || id_n == id_c {
                continue;
            }
            let z_n = textureLoad(outline_depth_tex, n_px, 0);
            if id_c > 0 && z_n >= z_c {
                continue;
            }
            if id_c == 0 && z_n >= op_z {
                continue;
            }
            let w_n = outline.widths[id_n].x;
            if w_n <= 0.0 || dist > w_n + 0.5 {
                continue;
            }
            let cov = clamp(w_n + 0.5 - dist, 0.0, 1.0);
            // Prefer highest edge coverage first so slanted surfaces do not hatch; tie-break by depth.
            if cov > best_cov + 1e-4 || (abs(cov - best_cov) <= 1e-4 && z_n < best_z) {
                best_cov = cov;
                best_z = z_n;
                best_id = id_n;
            }
        }
    }

    if best_id <= 0 || best_cov <= 0.004 {
        discard;
    }
    let col = outline.colors[best_id];
    let out_a = col.a * best_cov;
    return vec4<f32>(srgb_decode(col.rgb) * out_a, out_a);
}
