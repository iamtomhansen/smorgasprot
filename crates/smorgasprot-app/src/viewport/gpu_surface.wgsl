// Molecular surface shading, reproducing `complex_scene::add_mesh`'s CPU formula exactly:
// `shade = 0.55 + 0.45 * abs(dot(face_normal, LIGHT_DIR))`, with `face_normal` the unnormalized
// average of a triangle's 3 vertex normals, baked into all 3 of that triangle's vertices at
// upload time so flat per-facet shading falls out of ordinary interpolation without relying on
// a `@interpolate(flat)` provoking-vertex convention, which varies by backend. Like the CPU
// path, shading uses the mesh's own (world-space) normals directly, never rotated into
// plane-local space — `LIGHT_DIR` is fixed in that same world space.
const LIGHT_DIR: vec3<f32> = vec3<f32>(0.3, 0.5, 0.8);

struct Camera {
    // Camera-only orthographic view-projection (`CameraState::view_projection_matrix`).
    view_proj: mat4x4<f32>,
    // World → plane-local `(u, v, n)` frame (`PlaneFrame::to_local_matrix`); applied before
    // the per-chain offset below, matching `complex_scene::View::project`'s
    // `camera.project(frame.to_local(world) + offset, rect)` pipeline exactly.
    frame: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct ChainStyle {
    // Plane-local explode offset (xyz), added after `camera.frame`.
    offset: vec4<f32>,
    // Nonlinear (Color32-style) base RGB in `[0, 1]` and alpha, matching `style::palette::shaded`.
    color: vec4<f32>,
};
@group(1) @binding(0) var<uniform> style: ChainStyle;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) face_normal: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) face_normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    let local = camera.frame * vec4<f32>(position, 1.0) + vec4<f32>(style.offset.xyz, 0.0);
    out.clip_position = camera.view_proj * local;
    out.face_normal = face_normal;
    return out;
}

// Standard sRGB EOTF, used to pre-compensate for the hardware OETF the Srgb render target
// applies on store, so the bytes that land in the texture are bit-identical to what the CPU
// path's `Color32` arithmetic (plain nonlinear byte math, no gamma awareness) would produce.
fn srgb_decode_channel(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let shade = 0.55 + 0.45 * abs(dot(in.face_normal, LIGHT_DIR));
    // egui's image blend state (`src: One, dst: OneMinusSrcAlpha`) expects premultiplied rgb.
    let nonlinear_premultiplied = style.color.rgb * shade * style.color.a;
    let linear = vec3<f32>(
        srgb_decode_channel(nonlinear_premultiplied.x),
        srgb_decode_channel(nonlinear_premultiplied.y),
        srgb_decode_channel(nonlinear_premultiplied.z),
    );
    return vec4<f32>(linear, style.color.a);
}
