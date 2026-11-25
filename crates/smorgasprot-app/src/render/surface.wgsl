// Molecular & boundary surface shader: Lambert flat/smooth/unlit shading + anti-aliased
// barycentric wireframe, with entry points for opaque/sorted, WBOIT, and depth-peeling passes.
const LIGHT_DIR: vec3<f32> = vec3<f32>(0.3, 0.5, 0.8);

struct Camera {
    // Camera-only orthographic view-projection (`CameraState::view_projection_matrix`).
    view_proj: mat4x4<f32>,
    // World -> plane-local `(u, v, n)` frame (`PlaneFrame::to_local_matrix`).
    frame: mat4x4<f32>,
    // `(clip_z_near, clip_z_far, inv_clip_z_span, peel_z_eps)` for WBOIT & depth peeling.
    depth_info: vec4<f32>,
    // `(width_pt, height_pt, px_per_pt_inv, 0.0)` for screen-space sizing in sibling shaders.
    viewport: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct MeshStyle {
    // Plane-local explode offset (`xyz`); `w` unused.
    offset: vec4<f32>,
    // Nonlinear sRGB base color multiplier (`rgb`) and face alpha (`a`).
    color: vec4<f32>,
    // Nonlinear sRGB wireframe edge color multiplier (`rgb`); `w` is pixel width (<= 0 disables).
    wireframe: vec4<f32>,
    // `x` = show_faces (0/1), `y` = shading (0=Flat, 1=Smooth, 2=Unlit), `z` = edge_alpha, `w` = plane_local (0/1).
    params: vec4<f32>,
};
@group(1) @binding(0) var<uniform> style: MeshStyle;

@group(2) @binding(0) var prev_peel_depth: texture_depth_2d;
@group(2) @binding(1) var opaque_depth: texture_depth_2d;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) face_normal: vec3<f32>,
    @location(1) vertex_normal: vec3<f32>,
    @location(2) vert_color: vec4<f32>,
    @location(3) edge_color: vec3<f32>,
    @location(4) bary: vec3<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @location(0) position: vec3<f32>,
    @location(1) face_normal: vec3<f32>,
    @location(2) vertex_normal: vec3<f32>,
    @location(3) vert_color: vec4<f32>,
    @location(4) edge_color: vec3<f32>,
) -> VsOut {
    var out: VsOut;
    let frame_pos = select(camera.frame * vec4<f32>(position, 1.0), vec4<f32>(position, 1.0), style.params.w > 0.5);
    let local = frame_pos + vec4<f32>(style.offset.xyz, 0.0);
    out.clip_position = camera.view_proj * local;
    out.face_normal = face_normal;
    out.vertex_normal = vertex_normal;
    out.vert_color = vert_color;
    out.edge_color = edge_color;
    let corner = vid % 3u;
    out.bary = vec3<f32>(
        f32(corner == 0u),
        f32(corner == 1u),
        f32(corner == 2u),
    );
    return out;
}

// Standard sRGB EOTF used to pre-compensate for the hardware OETF applied by the `*Srgb` target,
// keeping opaque colors bit-consistent with CPU `Color32` math while keeping linear-alpha blends accurate.
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

fn eval_surface(in: VsOut) -> vec4<f32> {
    // Derivatives must be evaluated in uniform control flow before any discard/return.
    let fw = max(fwidth(in.bary), vec3<f32>(1e-5));
    let dist_px = in.bary / fw;
    let min_dist_px = min(dist_px.x, min(dist_px.y, dist_px.z));

    let show_faces = style.params.x > 0.5;
    let shading_mode = i32(style.params.y + 0.5);
    let edge_alpha = style.params.z;

    let wf_width = style.wireframe.w;
    var edge_cov = 0.0;
    if wf_width > 0.0 {
        edge_cov = 1.0 - smoothstep(max(wf_width - 0.5, 0.0), wf_width + 0.5, min_dist_px);
    }

    let edge_a = edge_alpha * edge_cov;
    let edge_linear = srgb_decode(style.wireframe.rgb * in.edge_color);
    let edge_pre = edge_linear * edge_a;

    if !show_faces {
        return vec4<f32>(edge_pre, edge_a);
    }

    var shade = 1.0;
    if shading_mode == 0 {
        shade = 0.55 + 0.45 * abs(dot(in.face_normal, LIGHT_DIR));
    } else if shading_mode == 1 {
        let vn_len = length(in.vertex_normal);
        let vn = select(in.face_normal, in.vertex_normal / max(vn_len, 1e-5), vn_len > 1e-5);
        shade = 0.55 + 0.45 * abs(dot(vn, LIGHT_DIR));
    }

    let face_linear = srgb_decode(style.color.rgb * in.vert_color.rgb * shade);
    let face_a = style.color.a * in.vert_color.a;
    let face_pre = face_linear * face_a;

    let out_pre = edge_pre + face_pre * (1.0 - edge_a);
    let out_a = edge_a + face_a * (1.0 - edge_a);
    return vec4<f32>(out_pre, out_a);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let frag = eval_surface(in);
    if frag.a <= 0.004 {
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
    let frag = eval_surface(in);
    if frag.a <= 0.004 {
        discard;
    }
    // Normalized scene depth in `[0, 1]` where `0` is closest to camera, `1` is farthest.
    let z_norm = clamp((in.clip_position.z - camera.depth_info.x) * camera.depth_info.z, 0.0, 1.0);
    let one_minus_z = 1.0 - z_norm;
    // Cap peak weight so high overlap sums stay safely within `Rgba16Float` finite range.
    let w = clamp(300.0 * one_minus_z * one_minus_z * one_minus_z, 1e-2, 300.0);

    var out: WboitOut;
    out.accum = vec4<f32>(frag.rgb * w, frag.a * w);
    out.reveal = vec4<f32>(frag.a, 0.0, 0.0, frag.a);
    return out;
}

@fragment
fn fs_peel(in: VsOut) -> @location(0) vec4<f32> {
    let frag = eval_surface(in);
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

@fragment
fn fs_outline_id(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(style.offset.w / 255.0, 0.0, 0.0, 1.0);
}
