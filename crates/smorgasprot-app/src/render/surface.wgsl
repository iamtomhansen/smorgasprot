// Molecular surface shading + anti-aliased barycentric wireframe, matching `complex_scene::add_mesh`:
// `shade = 0.55 + 0.45 * abs(dot(face_normal, LIGHT_DIR))`, with `face_normal` the unnormalized
// average of a triangle's 3 vertex normals, baked into all 3 vertices at upload time so flat per-facet
// shading falls out of ordinary interpolation across backends.
const LIGHT_DIR: vec3<f32> = vec3<f32>(0.3, 0.5, 0.8);

struct Camera {
    // Camera-only orthographic view-projection (`CameraState::view_projection_matrix`).
    view_proj: mat4x4<f32>,
    // World -> plane-local `(u, v, n)` frame (`PlaneFrame::to_local_matrix`).
    frame: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct ChainStyle {
    // Plane-local explode offset (xyz); `w` > 0.5 enables face fill rendering.
    offset_and_faces: vec4<f32>,
    // Nonlinear (Color32-style) base RGB in `[0, 1]` and alpha.
    color: vec4<f32>,
    // Nonlinear wireframe edge RGB in `[0, 1]`; `w` is wireframe width in pixels (<= 0 disables).
    wireframe: vec4<f32>,
};
@group(1) @binding(0) var<uniform> style: ChainStyle;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) face_normal: vec3<f32>,
    @location(1) bary: vec3<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @location(0) position: vec3<f32>,
    @location(1) face_normal: vec3<f32>,
) -> VsOut {
    var out: VsOut;
    let local = camera.frame * vec4<f32>(position, 1.0) + vec4<f32>(style.offset_and_faces.xyz, 0.0);
    out.clip_position = camera.view_proj * local;
    out.face_normal = face_normal;
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Derivatives must be evaluated in uniform control flow before any discard/return.
    let fw = max(fwidth(in.bary), vec3<f32>(1e-5));
    let dist_px = in.bary / fw;
    let min_dist_px = min(dist_px.x, min(dist_px.y, dist_px.z));

    let show_faces = style.offset_and_faces.w > 0.5;
    let wf_width = style.wireframe.w;
    var edge_cov = 0.0;
    if wf_width > 0.0 {
        edge_cov = 1.0 - smoothstep(max(wf_width - 0.5, 0.0), wf_width + 0.5, min_dist_px);
    }

    let edge_a = 0.72 * style.color.a * edge_cov;
    let edge_linear = srgb_decode(style.wireframe.rgb);
    let edge_pre = edge_linear * edge_a;

    if !show_faces {
        if edge_a <= 0.004 {
            discard;
        }
        return vec4<f32>(edge_pre, edge_a);
    }

    let shade = 0.55 + 0.45 * abs(dot(in.face_normal, LIGHT_DIR));
    let face_linear = srgb_decode(style.color.rgb * shade);
    let face_a = style.color.a;
    let face_pre = face_linear * face_a;

    let out_pre = edge_pre + face_pre * (1.0 - edge_a);
    let out_a = edge_a + face_a * (1.0 - edge_a);
    return vec4<f32>(out_pre, out_a);
}
