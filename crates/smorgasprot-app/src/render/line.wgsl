// Instanced screen-space anti-aliased capsule segments with polyline join clipping and dash/dot support.
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
    @location(0) local_uv: vec2<f32>,
    // `x` = seg_len, `y` = half_w, `z` = border_pt.
    @location(1) seg_info: vec3<f32>,
    @location(2) color: vec4<f32>,
    // `xy` = fragment offset from `pt_a`, `zw` = `pt_b - pt_a` (logical points).
    @location(3) rel_pts: vec4<f32>,
    // `xy` = start miter bisector normal, `zw` = end miter bisector normal.
    @location(4) join_normals: vec4<f32>,
    // `x` = has_prev (0/1), `y` = has_next (0/1), `z` = dash_on_pt, `w` = dash_off_pt.
    @location(5) join_dash: vec4<f32>,
    @location(6) border_color: vec4<f32>,
};

const QUAD_U: array<f32, 6> = array<f32, 6>(0.0, 1.0, 0.0, 0.0, 1.0, 1.0);
const QUAD_V: array<f32, 6> = array<f32, 6>(-1.0, -1.0, 1.0, 1.0, -1.0, 1.0);

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @location(2) prev: vec3<f32>,
    @location(3) next: vec3<f32>,
    @location(4) color: vec4<f32>,
    // `x` = width_pt, `y` = join bitmask (1=prev, 2=next), `z` = dash_on_pt, `w` = dash_off_pt.
    @location(5) params: vec4<f32>,
    @location(6) border_color: vec4<f32>,
    @location(7) border_pt: f32,
) -> VsOut {
    var out: VsOut;
    let clip_a = camera.view_proj * vec4<f32>(a, 1.0);
    let clip_b = camera.view_proj * vec4<f32>(b, 1.0);

    let half_vp = 0.5 * camera.viewport.xy;
    let pt_a = clip_a.xy * half_vp;
    let pt_b = clip_b.xy * half_vp;
    let delta = pt_b - pt_a;
    let seg_len = length(delta);
    let dir = select(vec2<f32>(1.0, 0.0), delta / max(seg_len, 1e-4), seg_len > 1e-4);
    let perp = vec2<f32>(-dir.y, dir.x);

    let half_w = 0.5 * max(params.x, 0.0);
    let b_pt = max(border_pt, 0.0);
    let pad = half_w + 0.5 * b_pt + camera.viewport.z;
    let u = mix(-pad, seg_len + pad, QUAD_U[vid]);
    let v = QUAD_V[vid] * pad;

    let rel_a = dir * u + perp * v;
    let pt = pt_a + rel_a;
    let t_along = clamp(u / max(seg_len, 1e-4), 0.0, 1.0);
    let z = mix(clip_a.z, clip_b.z, t_along);

    let join_mask = i32(params.y + 0.5);
    var has_prev = 0.0;
    var n_start = dir;
    if (join_mask & 1) != 0 {
        let clip_prev = camera.view_proj * vec4<f32>(prev, 1.0);
        let dp = pt_a - clip_prev.xy * half_vp;
        let dp_len = length(dp);
        if dp_len > 1e-4 {
            let sum_p = dp / dp_len + dir;
            let sum_len = length(sum_p);
            n_start = select(dir, sum_p / max(sum_len, 1e-4), sum_len > 1e-3);
            has_prev = 1.0;
        }
    }

    var has_next = 0.0;
    var n_end = dir;
    if (join_mask & 2) != 0 {
        let clip_next = camera.view_proj * vec4<f32>(next, 1.0);
        let dn = clip_next.xy * half_vp - pt_b;
        let dn_len = length(dn);
        if dn_len > 1e-4 {
            let sum_n = dir + dn / dn_len;
            let sum_len = length(sum_n);
            n_end = select(dir, sum_n / max(sum_len, 1e-4), sum_len > 1e-3);
            has_next = 1.0;
        }
    }

    out.clip_position = vec4<f32>(pt / half_vp, z, 1.0);
    out.local_uv = vec2<f32>(u, v);
    out.seg_info = vec3<f32>(seg_len, half_w, b_pt);
    out.color = color;
    out.rel_pts = vec4<f32>(rel_a, delta);
    out.join_normals = vec4<f32>(n_start, n_end);
    out.join_dash = vec4<f32>(has_prev, has_next, params.z, params.w);
    out.border_color = border_color;
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

fn eval_line(in: VsOut) -> vec4<f32> {
    let px_pt = camera.viewport.z;
    let aa = 0.5 * px_pt;
    let half_w = in.seg_info.y;
    let b_pt = in.seg_info.z;
    let join_limit = half_w * 3.0 + 0.5 * b_pt + px_pt;

    let pa = in.rel_pts.xy;
    if in.join_dash.x > 0.5 && length(pa) < join_limit && dot(pa, in.join_normals.xy) < 0.0 {
        discard;
    }
    let pb = in.rel_pts.xy - in.rel_pts.zw;
    if in.join_dash.y > 0.5 && length(pb) <= join_limit && dot(pb, in.join_normals.zw) >= 0.0 {
        discard;
    }

    let clamped_u = clamp(in.local_uv.x, 0.0, in.seg_info.x);
    var sdf = length(in.local_uv - vec2<f32>(clamped_u, 0.0));

    if in.join_dash.z > 0.0 && in.join_dash.w > 0.0 {
        let period = in.join_dash.z + in.join_dash.w;
        let u = max(in.local_uv.x, 0.0);
        let phase = u - floor(u / period) * period;
        let d_dash = min(max(phase - in.join_dash.z, 0.0), period - phase);
        let dist_dash = length(vec2<f32>(d_dash, in.local_uv.y));
        sdf = max(sdf, dist_dash);
    }

    let fill_cov = 1.0 - smoothstep(max(half_w - aa, 0.0), half_w + aa, sdf);
    let fill_a = in.color.a * fill_cov;
    let fill_pre = srgb_decode(in.color.rgb) * fill_a;

    var out_pre = fill_pre;
    var out_a = fill_a;
    if b_pt > 0.0 && in.border_color.a > 0.0 {
        let hw_b = 0.5 * b_pt;
        let outer = 1.0 - smoothstep(max(half_w + hw_b - aa, 0.0), half_w + hw_b + aa, sdf);
        let inner = smoothstep(max(half_w - hw_b - aa, 0.0), half_w - hw_b + aa, sdf);
        let b_a = in.border_color.a * (outer * inner);
        let b_pre = srgb_decode(in.border_color.rgb) * b_a;
        out_pre = b_pre + fill_pre * (1.0 - b_a);
        out_a = b_a + fill_a * (1.0 - b_a);
    }
    return vec4<f32>(out_pre, out_a);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let frag = eval_line(in);
    if frag.a <= 0.004 {
        discard;
    }
    return frag;
}

@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    let frag = eval_line(in);
    // Avoid low-coverage AA fringes writing depth and punching halos into geometry behind lines.
    if frag.a < 0.35 {
        discard;
    }
    return frag;
}

struct WboitOut {
    @location(0) accum: vec4<f32>,
    @location(1) reveal: vec4<f32>,
};

@fragment
fn fs_wboit(in: VsOut) -> WboitOut {
    let frag = eval_line(in);
    if frag.a <= 0.004 {
        discard;
    }
    let z_norm = clamp((in.clip_position.z - camera.depth_info.x) * camera.depth_info.z, 0.0, 1.0);
    let one_minus_z = 1.0 - z_norm;
    let w = clamp(300.0 * one_minus_z * one_minus_z * one_minus_z, 1e-2, 300.0);

    var out: WboitOut;
    out.accum = vec4<f32>(frag.rgb * w, frag.a * w);
    out.reveal = vec4<f32>(frag.a, 0.0, 0.0, frag.a);
    return out;
}

@fragment
fn fs_peel(in: VsOut) -> @location(0) vec4<f32> {
    let frag = eval_line(in);
    let px = vec2<i32>(in.clip_position.xy);
    let prev_z = textureLoad(prev_peel_depth, px, 0);
    let op_z = textureLoad(opaque_depth, px, 0);
    let z = in.clip_position.z;
    let eps = camera.depth_info.w;
    if z <= prev_z + eps || z >= op_z || frag.a <= 0.004 {
        discard;
    }
    return frag;
}
