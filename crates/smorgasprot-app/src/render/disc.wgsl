// Instanced screen-space anti-aliased outlined discs in plane-local coordinates.
struct Camera {
    view_proj: mat4x4<f32>,
    frame: mat4x4<f32>,
    depth_info: vec4<f32>,
    viewport: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

@group(1) @binding(0) var prev_peel_depth: texture_depth_2d;
@group(1) @binding(1) var opaque_depth: texture_depth_2d;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pt: vec2<f32>,
    // `x` = radius_pt, `y` = outline_pt, `z` = shaded_3d (0/1), `w` = r_clip_z.
    @location(1) radii: vec4<f32>,
    @location(2) fill_color: vec4<f32>,
    @location(3) outline_color: vec4<f32>,
};

const QUAD_X: array<f32, 6> = array<f32, 6>(-1.0, 1.0, -1.0, -1.0, 1.0, 1.0);
const QUAD_Y: array<f32, 6> = array<f32, 6>(-1.0, -1.0, 1.0, 1.0, -1.0, 1.0);

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @location(0) center: vec3<f32>,
    @location(1) radius_pt: f32,
    @location(2) outline_pt: f32,
    @location(3) fill_color: vec4<f32>,
    @location(4) outline_color: vec4<f32>,
    @location(5) shaded_3d: f32,
) -> VsOut {
    var out: VsOut;
    let clip_c = camera.view_proj * vec4<f32>(center, 1.0);
    let half_vp = 0.5 * camera.viewport.xy;
    let r = max(radius_pt, 0.0);
    let pad = r + 0.5 * max(outline_pt, 0.0) + camera.viewport.z;
    let local_pt = vec2<f32>(QUAD_X[vid], QUAD_Y[vid]) * pad;

    out.clip_position = vec4<f32>(clip_c.xy + local_pt / half_vp, clip_c.z, 1.0);
    out.local_pt = local_pt;
    out.radii = vec4<f32>(r, max(outline_pt, 0.0), shaded_3d, r * camera.viewport.w);
    out.fill_color = fill_color;
    out.outline_color = outline_color;
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

struct DiscEval {
    color: vec4<f32>,
    depth: f32,
};

fn eval_disc(in: VsOut) -> DiscEval {
    let aa = 0.5 * camera.viewport.z;
    let r = in.radii.x;
    let hw = 0.5 * in.radii.y;
    let d = length(in.local_pt);

    var shade = 1.0;
    var depth = in.clip_position.z;
    if in.radii.z > 0.5 && r > 1e-4 {
        let norm_xy = in.local_pt / r;
        let r2 = dot(norm_xy, norm_xy);
        let nz = sqrt(max(0.0, 1.0 - r2));
        let n = vec3<f32>(norm_xy, nz);
        let light_dir = normalize(vec3<f32>(0.3, 0.5, 0.8));
        shade = 0.35 + 0.65 * max(dot(n, light_dir), 0.0);
        depth = clamp(in.clip_position.z - nz * in.radii.w, 0.0, 1.0);
    }

    let fill_cov = 1.0 - smoothstep(max(r - aa, 0.0), r + aa, d);
    let fill_a = in.fill_color.a * fill_cov;
    let fill_pre = srgb_decode(in.fill_color.rgb * shade) * fill_a;

    var out_pre = fill_pre;
    var out_a = fill_a;
    if in.radii.y > 0.0 && in.outline_color.a > 0.0 {
        let outer_cov = 1.0 - smoothstep(max(r + hw - aa, 0.0), r + hw + aa, d);
        let inner_cov = smoothstep(max(r - hw - aa, 0.0), r - hw + aa, d);
        let outl_a = in.outline_color.a * (outer_cov * inner_cov);
        let outl_pre = srgb_decode(in.outline_color.rgb) * outl_a;
        out_pre = outl_pre + fill_pre * (1.0 - outl_a);
        out_a = outl_a + fill_a * (1.0 - outl_a);
    }
    var res: DiscEval;
    res.color = vec4<f32>(out_pre, out_a);
    res.depth = depth;
    return res;
}

struct FragOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: VsOut) -> FragOut {
    let res = eval_disc(in);
    if res.color.a <= 0.004 {
        discard;
    }
    var out: FragOut;
    out.color = res.color;
    out.depth = res.depth;
    return out;
}

@fragment
fn fs_opaque(in: VsOut) -> FragOut {
    let res = eval_disc(in);
    if res.color.a < 0.35 {
        discard;
    }
    var out: FragOut;
    out.color = res.color;
    out.depth = res.depth;
    return out;
}

struct WboitOut {
    @location(0) accum: vec4<f32>,
    @location(1) reveal: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_wboit(in: VsOut) -> WboitOut {
    let res = eval_disc(in);
    if res.color.a <= 0.004 {
        discard;
    }
    let z_norm = clamp((res.depth - camera.depth_info.x) * camera.depth_info.z, 0.0, 1.0);
    let one_minus_z = 1.0 - z_norm;
    let w = clamp(300.0 * one_minus_z * one_minus_z * one_minus_z, 1e-2, 300.0);

    var out: WboitOut;
    out.accum = vec4<f32>(res.color.rgb * w, res.color.a * w);
    out.reveal = vec4<f32>(res.color.a, 0.0, 0.0, res.color.a);
    out.depth = res.depth;
    return out;
}

@fragment
fn fs_peel(in: VsOut) -> FragOut {
    let res = eval_disc(in);
    let px = vec2<i32>(in.clip_position.xy);
    let prev_z = textureLoad(prev_peel_depth, px, 0);
    let op_z = textureLoad(opaque_depth, px, 0);
    let z = res.depth;
    let eps = camera.depth_info.w;
    if z <= prev_z + eps || z >= op_z || res.color.a <= 0.004 {
        discard;
    }
    var out: FragOut;
    out.color = res.color;
    out.depth = z;
    return out;
}
