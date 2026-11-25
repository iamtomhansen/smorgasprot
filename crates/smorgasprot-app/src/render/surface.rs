//! GPU-rendered 3D scene (`meshes`, `lines`, `discs`) on a shared depth-tested `wgpu` pipeline.
//!
//! Renders opaque geometry first into `color + opaque_depth`, then resolves translucent geometry
//! across all primitive types using [`TransparencyMode`] (`Wboit`, `DepthPeeling`, or `CentroidSorted`).

use crate::camera::CameraState;
use crate::state::{InterfaceDisplayMode, MeshShading, SurfaceCulling, TransparencyMode};
use crate::style::palette;
use egui::{Color32, Rect};
use glam::Vec3;
use smorgasprot_core::SurfaceMesh;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

const SURFACE_SHADER_SRC: &str = include_str!("surface.wgsl");
const LINE_SHADER_SRC: &str = include_str!("line.wgsl");
const DISC_SHADER_SRC: &str = include_str!("disc.wgsl");
const COMPOSITE_SHADER_SRC: &str = include_str!("composite.wgsl");
const OUTLINE_SHADER_SRC: &str = include_str!("outline.wgsl");

/// Offscreen color format; must be `*Srgb` per `egui_wgpu::Renderer::register_native_texture`.
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const WBOIT_ACCUM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const WBOIT_REVEAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// Alpha at or above which a primitive is treated as opaque (depth-writing).
const OPAQUE_ALPHA_THRESHOLD: f32 = 0.999;

/// Stable per-mesh cache identifier within a dataset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuMeshKey {
    Chain(usize),
    Boundary(usize, InterfaceDisplayMode),
    Ribbon(usize),
    Cartoon(usize),
    BallStickBonds(usize),
    BallStickSpheres(usize),
    Spheres(usize),
    InterfaceSpheres(usize),
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    face_normal: [f32; 3],
    vertex_normal: [f32; 3],
    color: [f32; 4],
    edge_color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLineInstance {
    a: [f32; 3],
    b: [f32; 3],
    prev: [f32; 3],
    next: [f32; 3],
    color: [f32; 4],
    // `width_pt`, `join_mask`, `dash_on_pt`, `dash_off_pt`.
    params: [f32; 4],
    border_color: [f32; 4],
    border_pt: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuDiscInstance {
    center: [f32; 3],
    radius_pt: f32,
    outline_pt: f32,
    fill_color: [f32; 4],
    outline_color: [f32; 4],
    shaded_3d: f32,
}

struct InstanceBuffer {
    buf: Option<wgpu::Buffer>,
    cap_bytes: usize,
}

impl InstanceBuffer {
    fn new() -> Self {
        Self { buf: None, cap_bytes: 0 }
    }

    fn upload<T: bytemuck::Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        data: &[T],
    ) -> u32 {
        let bytes = bytemuck::cast_slice(data);
        if bytes.is_empty() {
            return 0;
        }
        if self.buf.is_none() || self.cap_bytes < bytes.len() {
            let cap_bytes = bytes.len().next_power_of_two().max(4096);
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: cap_bytes as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.buf = Some(buf);
            self.cap_bytes = cap_bytes;
        }
        if let Some(buf) = &self.buf {
            queue.write_buffer(buf, 0, bytes);
        }
        data.len() as u32
    }
}

struct InstancedPipelines {
    opaque: wgpu::RenderPipeline,
    sorted: wgpu::RenderPipeline,
    wboit: Option<wgpu::RenderPipeline>,
    peel: wgpu::RenderPipeline,
}

struct MeshEntry {
    vertex_buf: Option<wgpu::Buffer>,
    vertex_count: u32,
    center: Vec3,
    radius: f32,
    style_buf: wgpu::Buffer,
    style_bind_group: wgpu::BindGroup,
    draw_gen: u64,
    color_sig: u64,
}

struct WboitTargets {
    accum_view: wgpu::TextureView,
    reveal_view: wgpu::TextureView,
    composite_bind_group: wgpu::BindGroup,
}

struct PeelTargets {
    depth_views: [wgpu::TextureView; 2],
    depth_bind_groups: [wgpu::BindGroup; 2],
    peel_color_view: wgpu::TextureView,
    peel_color_bg: wgpu::BindGroup,
    peel_accum_view: wgpu::TextureView,
    peel_accum_bg: wgpu::BindGroup,
}

struct OutlineTargets {
    id_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    composite_bg: wgpu::BindGroup,
}

/// Offscreen render target, resized to match the viewport rect.
struct Target {
    color_view: wgpu::TextureView,
    opaque_depth_view: wgpu::TextureView,
    texture_id: egui::TextureId,
    size: (u32, u32),
    wboit: Option<WboitTargets>,
    peel: Option<PeelTargets>,
    outline: Option<OutlineTargets>,
}

/// One 3D mesh to draw this frame (chain surface, interface boundary, or 3D cartoon prism).
pub struct GpuMeshDraw<'a> {
    pub key: GpuMeshKey,
    pub mesh: &'a SurfaceMesh,
    /// Optional per-vertex base colors (`ContactEnvelope`); falls back to `Color32::WHITE` when `None`.
    pub vertex_colors: Option<&'a [Color32]>,
    /// Cache key for `vertex_colors` or procedural mesh parameters.
    pub color_sig: u64,
    pub offset: Vec3,
    pub color: Color32,
    pub edge_color: Color32,
    pub alpha: f32,
    pub edge_alpha: f32,
    pub show_faces: bool,
    pub wireframe_width_px: f32,
    pub culling: SurfaceCulling,
    pub shading: MeshShading,
    pub border_width_px: f32,
    pub border_color: Color32,
    pub border_alpha: f32,
}

/// One 3D screen-space-width capsule segment in plane-local coordinates.
#[derive(Clone, Copy, Debug)]
pub struct GpuLineDraw {
    pub a: Vec3,
    pub b: Vec3,
    pub prev: Option<Vec3>,
    pub next: Option<Vec3>,
    pub color: Color32,
    pub alpha: f32,
    pub width_pt: f32,
    /// Optional `[dash_on_pt, dash_off_pt]` pattern along screen-space length.
    pub dash: Option<[f32; 2]>,
    pub border_pt: f32,
    pub border_color: Color32,
    pub border_alpha: f32,
}

/// One 3D flat-filled unlit triangle in plane-local coordinates.
#[derive(Clone, Copy, Debug)]
pub struct GpuPolyTri {
    pub pts: [Vec3; 3],
    pub color: Color32,
    pub alpha: f32,
}

/// One 3D screen-space-radius outlined disc/sphere in plane-local coordinates.
#[derive(Clone, Copy, Debug)]
pub struct GpuDiscDraw {
    pub center: Vec3,
    pub radius_pt: f32,
    pub outline_pt: f32,
    pub fill_color: Color32,
    pub fill_alpha: f32,
    pub outline_color: Color32,
    pub outline_alpha: f32,
    pub shaded_3d: bool,
}

pub struct GpuSurfaceRenderer {
    render_state: egui_wgpu::RenderState,
    // Indexed by `culling_index(culling)`: None, Back, Front.
    opaque_pipelines: [wgpu::RenderPipeline; 3],
    sorted_pipelines: [wgpu::RenderPipeline; 3],
    wboit_pipelines: Option<[wgpu::RenderPipeline; 3]>,
    peel_pipelines: [wgpu::RenderPipeline; 3],
    outline_id_pipelines: [wgpu::RenderPipeline; 3],
    line_pipelines: InstancedPipelines,
    disc_pipelines: InstancedPipelines,
    wboit_composite_pipeline: wgpu::RenderPipeline,
    peel_under_pipeline: wgpu::RenderPipeline,
    peel_over_pipeline: wgpu::RenderPipeline,
    outline_composite_pipeline: wgpu::RenderPipeline,
    mesh_bind_group_layout: wgpu::BindGroupLayout,
    peel_depth_bgl: wgpu::BindGroupLayout,
    wboit_composite_bgl: wgpu::BindGroupLayout,
    copy_bgl: wgpu::BindGroupLayout,
    outline_bgl: wgpu::BindGroupLayout,
    camera_buf: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    poly_style_bind_group: wgpu::BindGroup,
    outline_buf: wgpu::Buffer,
    poly_buf: InstanceBuffer,
    line_buf: InstanceBuffer,
    disc_buf: InstanceBuffer,
    target: Option<Target>,
    meshes: HashMap<(u64, GpuMeshKey), MeshEntry>,
}

impl Drop for GpuSurfaceRenderer {
    fn drop(&mut self) {
        if let Some(target) = self.target.take() {
            self.render_state.renderer.write().free_texture(&target.texture_id);
        }
    }
}

fn culling_index(culling: SurfaceCulling) -> usize {
    match culling {
        SurfaceCulling::None => 0,
        SurfaceCulling::Backfaces => 1,
        SurfaceCulling::Frontfaces => 2,
    }
}

fn cull_face(idx: usize) -> Option<wgpu::Face> {
    match idx {
        1 => Some(wgpu::Face::Back),
        2 => Some(wgpu::Face::Front),
        _ => None,
    }
}

fn is_opaque_mesh(draw: &GpuMeshDraw<'_>) -> bool {
    draw.show_faces && draw.alpha >= OPAQUE_ALPHA_THRESHOLD
}

fn shading_code(shading: MeshShading) -> f32 {
    match shading {
        MeshShading::Flat => 0.0,
        MeshShading::Smooth => 1.0,
        MeshShading::Unlit => 2.0,
    }
}

fn unmultiplied_rgba(c: Color32, extra_alpha: f32) -> [f32; 4] {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        ((a as f32 / 255.0) * extra_alpha).clamp(0.0, 1.0),
    ]
}

fn srgb_u8_to_linear(u: u8) -> f64 {
    let x = u as f64 / 255.0;
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

fn srgb_color_to_linear_wgpu(c: Color32) -> wgpu::Color {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    wgpu::Color {
        r: srgb_u8_to_linear(r),
        g: srgb_u8_to_linear(g),
        b: srgb_u8_to_linear(b),
        a: 1.0,
    }
}

fn premul_over_blend() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

fn under_blend() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

fn wboit_color_targets() -> [Option<wgpu::ColorTargetState>; 2] {
    [
        Some(wgpu::ColorTargetState {
            format: WBOIT_ACCUM_FORMAT,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format: WBOIT_REVEAL_FORMAT,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ]
}

fn pack_camera_uniform(
    vp: glam::Mat4,
    frame: glam::Mat4,
    depth_info: [f32; 4],
    viewport: [f32; 4],
    light_view: [f32; 4],
) -> [f32; 44] {
    let mut out = [0.0_f32; 44];
    out[..16].copy_from_slice(&vp.to_cols_array());
    out[16..32].copy_from_slice(&frame.to_cols_array());
    out[32..36].copy_from_slice(&depth_info);
    out[36..40].copy_from_slice(&viewport);
    out[40..44].copy_from_slice(&light_view);
    out
}

fn uniform_bgl_entry(visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_bgl_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn make_instanced_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    instance_layout: &wgpu::VertexBufferLayout<'_>,
    base_layout: &wgpu::PipelineLayout,
    peel_layout: &wgpu::PipelineLayout,
    wboit_supported: bool,
    label: &str,
) -> InstancedPipelines {
    let color_target_premul = [Some(wgpu::ColorTargetState {
        format: COLOR_FORMAT,
        blend: Some(premul_over_blend()),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let color_target_replace = [Some(wgpu::ColorTargetState {
        format: COLOR_FORMAT,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let wboit_targets = wboit_color_targets();

    let build = |layout: &wgpu::PipelineLayout,
                 depth_write_enabled: bool,
                 fs_entry: &str,
                 targets: &[Option<wgpu::ColorTargetState>]| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: std::slice::from_ref(instance_layout),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs_entry),
                targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        })
    };

    InstancedPipelines {
        opaque: build(base_layout, true, "fs_opaque", &color_target_premul),
        sorted: build(base_layout, false, "fs_main", &color_target_premul),
        wboit: wboit_supported.then(|| build(base_layout, false, "fs_wboit", &wboit_targets)),
        peel: build(peel_layout, true, "fs_peel", &color_target_replace),
    }
}

impl GpuSurfaceRenderer {
    /// Builds mesh, line, disc, and transparency compositing pipelines from `cc.wgpu_render_state`.
    pub fn new(render_state: &egui_wgpu::RenderState) -> Self {
        let device = &render_state.device;
        let surface_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu_surface"),
            source: wgpu::ShaderSource::Wgsl(SURFACE_SHADER_SRC.into()),
        });
        let line_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu_line"),
            source: wgpu::ShaderSource::Wgsl(LINE_SHADER_SRC.into()),
        });
        let disc_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu_disc"),
            source: wgpu::ShaderSource::Wgsl(DISC_SHADER_SRC.into()),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu_composite"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER_SRC.into()),
        });

        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_surface_camera_bgl"),
            entries: &[uniform_bgl_entry(wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let mesh_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_surface_mesh_bgl"),
            entries: &[uniform_bgl_entry(wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let peel_depth_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_surface_peel_depth_bgl"),
            entries: &[
                texture_bgl_entry(0, wgpu::TextureSampleType::Depth),
                texture_bgl_entry(1, wgpu::TextureSampleType::Depth),
            ],
        });
        let wboit_composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_wboit_composite_bgl"),
            entries: &[
                texture_bgl_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
                texture_bgl_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let copy_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_copy_bgl"),
            entries: &[texture_bgl_entry(0, wgpu::TextureSampleType::Float { filterable: false })],
        });

        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu_surface_mesh_layout"),
            bind_group_layouts: &[&camera_bgl, &mesh_bind_group_layout],
            push_constant_ranges: &[],
        });
        let mesh_peel_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu_surface_peel_layout"),
            bind_group_layouts: &[&camera_bgl, &mesh_bind_group_layout, &peel_depth_bgl],
            push_constant_ranges: &[],
        });
        let overlay_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu_overlay_layout"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });
        let overlay_peel_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu_overlay_peel_layout"),
            bind_group_layouts: &[&camera_bgl, &peel_depth_bgl],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32x3,
                2 => Float32x3,
                3 => Float32x4,
                4 => Float32x3,
            ],
        };

        let make_mesh_pipeline = |layout: &wgpu::PipelineLayout,
                                  cull_idx: usize,
                                  depth_write_enabled: bool,
                                  fs_entry: &str,
                                  targets: &[Option<wgpu::ColorTargetState>],
                                  label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &surface_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[vertex_layout.clone()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: cull_face(cull_idx),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &surface_shader,
                    entry_point: Some(fs_entry),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                multiview: None,
                cache: None,
            })
        };

        let color_target_premul = [Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: Some(premul_over_blend()),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let color_target_replace = [Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let opaque_pipelines = std::array::from_fn(|c| {
            make_mesh_pipeline(&mesh_layout, c, true, "fs_main", &color_target_premul, "gpu_surface_opaque")
        });
        let sorted_pipelines = std::array::from_fn(|c| {
            make_mesh_pipeline(&mesh_layout, c, false, "fs_main", &color_target_premul, "gpu_surface_sorted")
        });
        let peel_pipelines = std::array::from_fn(|c| {
            make_mesh_pipeline(&mesh_peel_layout, c, true, "fs_peel", &color_target_replace, "gpu_surface_peel")
        });

        let wboit_features = render_state.adapter.get_texture_format_features(WBOIT_ACCUM_FORMAT);
        let wboit_supported = wboit_features.allowed_usages.contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            && wboit_features.flags.contains(wgpu::TextureFormatFeatureFlags::BLENDABLE);
        let wboit_targets = wboit_color_targets();
        let wboit_pipelines = wboit_supported.then(|| {
            std::array::from_fn(|c| {
                make_mesh_pipeline(&mesh_layout, c, false, "fs_wboit", &wboit_targets, "gpu_surface_wboit")
            })
        });

        let outline_id_target = [Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8Unorm,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let outline_id_pipelines = std::array::from_fn(|c| {
            make_mesh_pipeline(&mesh_layout, c, true, "fs_outline_id", &outline_id_target, "gpu_surface_outline_id")
        });

        let line_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuLineInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32x3,
                2 => Float32x3,
                3 => Float32x3,
                4 => Float32x4,
                5 => Float32x4,
                6 => Float32x4,
                7 => Float32,
            ],
        };
        let disc_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuDiscInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32,
                2 => Float32,
                3 => Float32x4,
                4 => Float32x4,
                5 => Float32,
            ],
        };
        let line_pipelines = make_instanced_pipelines(
            device,
            &line_shader,
            &line_instance_layout,
            &overlay_layout,
            &overlay_peel_layout,
            wboit_supported,
            "gpu_line",
        );
        let disc_pipelines = make_instanced_pipelines(
            device,
            &disc_shader,
            &disc_instance_layout,
            &overlay_layout,
            &overlay_peel_layout,
            wboit_supported,
            "gpu_disc",
        );

        let make_fullscreen_pipeline = |bgl: &wgpu::BindGroupLayout, fs_entry: &str, blend: wgpu::BlendState, label: &str| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[bgl],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &composite_shader,
                    entry_point: Some("vs_fullscreen"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &composite_shader,
                    entry_point: Some(fs_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                multiview: None,
                cache: None,
            })
        };

        let wboit_composite_pipeline =
            make_fullscreen_pipeline(&wboit_composite_bgl, "fs_wboit", premul_over_blend(), "gpu_wboit_composite");
        let peel_under_pipeline =
            make_fullscreen_pipeline(&copy_bgl, "fs_copy", under_blend(), "gpu_peel_under");
        let peel_over_pipeline =
            make_fullscreen_pipeline(&copy_bgl, "fs_copy", premul_over_blend(), "gpu_peel_over");

        let outline_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu_outline_shader"),
            source: wgpu::ShaderSource::Wgsl(OUTLINE_SHADER_SRC.into()),
        });
        let outline_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_outline_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let outline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu_outline_composite_layout"),
            bind_group_layouts: &[&outline_bgl],
            push_constant_ranges: &[],
        });
        let outline_composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gpu_outline_composite"),
            layout: Some(&outline_layout),
            vertex: wgpu::VertexState {
                module: &outline_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &outline_shader,
                entry_point: Some("fs_outline"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(premul_over_blend()),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });
        let outline_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_outline_buf"),
            contents: &vec![0u8; 256 * 32],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_surface_camera_buf"),
            contents: bytemuck::cast_slice(&pack_camera_uniform(
                glam::Mat4::IDENTITY,
                glam::Mat4::IDENTITY,
                [0.0, 1.0, 1.0, 1e-5],
                [1.0, 1.0, 1.0, 0.0],
                [0.3, 0.5, 0.8, 0.0],
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_surface_camera_bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        let poly_style: [f32; 16] = [
            0.0, 0.0, 0.0, 0.0,
            1.0, 1.0, 1.0, 1.0,
            1.0, 1.0, 1.0, 0.0,
            1.0, shading_code(MeshShading::Unlit), 0.0, 1.0,
        ];
        let poly_style_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_surface_poly_style"),
            contents: bytemuck::cast_slice(&poly_style),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let poly_style_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_surface_poly_style_bg"),
            layout: &mesh_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: poly_style_buf.as_entire_binding(),
            }],
        });

        Self {
            render_state: render_state.clone(),
            opaque_pipelines,
            sorted_pipelines,
            wboit_pipelines,
            peel_pipelines,
            outline_id_pipelines,
            line_pipelines,
            disc_pipelines,
            wboit_composite_pipeline,
            peel_under_pipeline,
            peel_over_pipeline,
            outline_composite_pipeline,
            mesh_bind_group_layout,
            peel_depth_bgl,
            wboit_composite_bgl,
            copy_bgl,
            outline_bgl,
            camera_buf,
            camera_bind_group,
            poly_style_bind_group,
            outline_buf,
            poly_buf: InstanceBuffer::new(),
            line_buf: InstanceBuffer::new(),
            disc_buf: InstanceBuffer::new(),
            target: None,
            meshes: HashMap::new(),
        }
    }

    /// True when the active GPU adapter supports `Rgba16Float` blending required by WBOIT.
    pub fn wboit_supported(&self) -> bool {
        self.wboit_pipelines.is_some()
    }

    /// (Re)creates the offscreen base color + opaque depth target if `size` changed.
    fn ensure_target(&mut self, size: (u32, u32)) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let device = &self.render_state.device;
        let extent = wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 };
        let color_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu_surface_color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu_surface_opaque_depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let opaque_depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let mut renderer = self.render_state.renderer.write();
        let texture_id = match &mut self.target {
            Some(prev) => {
                renderer.update_egui_texture_from_wgpu_texture(device, &color_view, wgpu::FilterMode::Linear, prev.texture_id);
                prev.texture_id
            }
            None => renderer.register_native_texture(device, &color_view, wgpu::FilterMode::Linear),
        };
        drop(renderer);
        self.target = Some(Target {
            color_view,
            opaque_depth_view,
            texture_id,
            size,
            wboit: None,
            peel: None,
            outline: None,
        });
    }

    fn ensure_outline_targets(
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
        outline_buf: &wgpu::Buffer,
        target: &mut Target,
    ) {
        if target.outline.is_some() {
            return;
        }
        let extent = wgpu::Extent3d { width: target.size.0, height: target.size.1, depth_or_array_layers: 1 };
        let id_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu_outline_id"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let id_view = id_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu_outline_depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let composite_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_outline_composite_bg"),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&id_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&depth_view) },
                wgpu::BindGroupEntry { binding: 2, resource: outline_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&target.opaque_depth_view) },
            ],
        });
        target.outline = Some(OutlineTargets { id_view, depth_view, composite_bg });
    }

    fn ensure_wboit_targets(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, target: &mut Target) {
        if target.wboit.is_some() {
            return;
        }
        let extent = wgpu::Extent3d { width: target.size.0, height: target.size.1, depth_or_array_layers: 1 };
        let make_tex = |label: &str, format: wgpu::TextureFormat| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let accum_view = make_tex("gpu_wboit_accum", WBOIT_ACCUM_FORMAT);
        let reveal_view = make_tex("gpu_wboit_reveal", WBOIT_REVEAL_FORMAT);
        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_wboit_composite_bg"),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&accum_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&reveal_view) },
            ],
        });
        target.wboit = Some(WboitTargets { accum_view, reveal_view, composite_bind_group });
    }

    fn ensure_peel_targets(
        device: &wgpu::Device,
        peel_depth_bgl: &wgpu::BindGroupLayout,
        copy_bgl: &wgpu::BindGroupLayout,
        target: &mut Target,
    ) {
        if target.peel.is_some() {
            return;
        }
        let extent = wgpu::Extent3d { width: target.size.0, height: target.size.1, depth_or_array_layers: 1 };
        let make_tex = |label: &str, format: wgpu::TextureFormat| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let depth_views = [
            make_tex("gpu_peel_depth_0", DEPTH_FORMAT),
            make_tex("gpu_peel_depth_1", DEPTH_FORMAT),
        ];
        // Bind group for peel pass `k % 2` reads the opposite peel depth (`(k + 1) % 2`).
        let depth_bind_groups = std::array::from_fn(|p| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu_peel_depth_bg"),
                layout: peel_depth_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&depth_views[(p + 1) % 2]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&target.opaque_depth_view),
                    },
                ],
            })
        });
        let peel_color_view = make_tex("gpu_peel_color", COLOR_FORMAT);
        let peel_accum_view = make_tex("gpu_peel_accum", COLOR_FORMAT);
        let make_copy_bg = |label: &str, view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: copy_bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }],
            })
        };
        let peel_color_bg = make_copy_bg("gpu_peel_color_bg", &peel_color_view);
        let peel_accum_bg = make_copy_bg("gpu_peel_accum_bg", &peel_accum_view);
        target.peel = Some(PeelTargets {
            depth_views,
            depth_bind_groups,
            peel_color_view,
            peel_color_bg,
            peel_accum_view,
            peel_accum_bg,
        });
    }

    /// Rebuilds a mesh's GPU vertex buffer when missing or when `draw_gen`/`color_sig` advances.
    fn ensure_mesh(&mut self, cache_key: (u64, GpuMeshKey), draw: &GpuMeshDraw<'_>, draw_gen: u64) {
        if self
            .meshes
            .get(&cache_key)
            .is_some_and(|e| e.draw_gen == draw_gen && e.color_sig == draw.color_sig)
        {
            return;
        }
        let mesh = draw.mesh;
        let vert_limit = mesh.vertices.len().min(mesh.normals.len());
        let mut vertices = Vec::with_capacity(mesh.triangles.len() * 3);
        let mut min_p = Vec3::splat(f32::INFINITY);
        let mut max_p = Vec3::splat(f32::NEG_INFINITY);

        for &[i0, i1, i2] in &mesh.triangles {
            let (v0, v1, v2) = (i0 as usize, i1 as usize, i2 as usize);
            if v0 >= vert_limit || v1 >= vert_limit || v2 >= vert_limit {
                continue;
            }
            let fn_raw = (Vec3::from(mesh.normals[v0]) + Vec3::from(mesh.normals[v1]) + Vec3::from(mesh.normals[v2])) / 3.0;
            let face_normal: [f32; 3] = fn_raw.into();
            for v in [v0, v1, v2] {
                let p = mesh.vertices[v];
                let pv = Vec3::from(p);
                min_p = min_p.min(pv);
                max_p = max_p.max(pv);
                let (vc, ec) = match draw.vertex_colors {
                    Some(cols) => {
                        let base = cols.get(v).copied().unwrap_or(Color32::WHITE);
                        let edge = palette::edge_color(base);
                        (
                            [base.r() as f32 / 255.0, base.g() as f32 / 255.0, base.b() as f32 / 255.0, 1.0],
                            [edge.r() as f32 / 255.0, edge.g() as f32 / 255.0, edge.b() as f32 / 255.0],
                        )
                    }
                    None => ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0]),
                };
                vertices.push(GpuVertex {
                    position: p,
                    face_normal,
                    vertex_normal: mesh.normals[v],
                    color: vc,
                    edge_color: ec,
                });
            }
        }
        let vertex_count = vertices.len() as u32;
        let (center, radius) = if vertex_count > 0 {
            (0.5 * (min_p + max_p), 0.5 * (max_p - min_p).length())
        } else {
            (Vec3::ZERO, 0.0)
        };

        let device = &self.render_state.device;
        let vertex_buf = (!vertices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gpu_surface_mesh_vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let style_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_surface_mesh_style"),
            contents: bytemuck::cast_slice(&[0.0_f32; 16]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let style_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_surface_mesh_style_bg"),
            layout: &self.mesh_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: style_buf.as_entire_binding(),
            }],
        });
        self.meshes.insert(
            cache_key,
            MeshEntry {
                vertex_buf,
                vertex_count,
                center,
                radius,
                style_buf,
                style_bind_group,
                draw_gen,
                color_sig: draw.color_sig,
            },
        );
    }

    /// Renders `meshes`, `polys`, `lines`, and `discs` into one shared depth-tested offscreen target.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        rect: Rect,
        pixels_per_point: f32,
        viewport_bg: Color32,
        camera: &CameraState,
        frame_to_local: glam::Mat4,
        dataset_instance_id: u64,
        draw_gen: u64,
        transparency: TransparencyMode,
        peel_layers: u8,
        mesh_draws: &[GpuMeshDraw<'_>],
        poly_draws: &[GpuPolyTri],
        line_draws: &[GpuLineDraw],
        disc_draws: &[GpuDiscDraw],
    ) -> Option<egui::TextureId> {
        if (mesh_draws.is_empty() && poly_draws.is_empty() && line_draws.is_empty() && disc_draws.is_empty())
            || !rect.is_finite()
            || rect.width() <= 0.0
            || rect.height() <= 0.0
            || !pixels_per_point.is_finite()
            || pixels_per_point <= 0.0
        {
            return None;
        }
        let max_dim = self.render_state.device.limits().max_texture_dimension_2d.max(1);
        let size = (
            ((rect.width() * pixels_per_point).round() as u32).clamp(1, max_dim),
            ((rect.height() * pixels_per_point).round() as u32).clamp(1, max_dim),
        );
        self.ensure_target(size);
        self.meshes.retain(|&(id, _), _| id == dataset_instance_id);

        let device = self.render_state.device.clone();
        let queue = self.render_state.queue.clone();
        let mut opaque_meshes = Vec::with_capacity(mesh_draws.len());
        let mut translucent_meshes: Vec<(f32, usize)> = Vec::new();
        let mut outlined_meshes: Vec<usize> = Vec::new();
        let mut outline_colors = [[0.0_f32; 4]; 256];
        let mut outline_widths = [[0.0_f32; 4]; 256];
        let mut max_outline_px = 0.0_f32;
        let mut next_outline_id = 1_usize;
        let mut view_z_min = f32::INFINITY;
        let mut view_z_max = f32::NEG_INFINITY;

        for (idx, draw) in mesh_draws.iter().enumerate() {
            let cache_key = (dataset_instance_id, draw.key);
            self.ensure_mesh(cache_key, draw, draw_gen);
            let entry = &self.meshes[&cache_key];
            if entry.vertex_count == 0 {
                continue;
            }
            let outline_id = if draw.show_faces
                && draw.border_width_px > 0.0
                && draw.border_color.a() > 0
                && draw.border_alpha > 0.004
                && next_outline_id < 256
            {
                let id = next_outline_id;
                next_outline_id += 1;
                let w_px = (draw.border_width_px * pixels_per_point).clamp(0.0, 48.0);
                let col = unmultiplied_rgba(draw.border_color, draw.border_alpha);
                if col[3] > 0.004 && w_px > 0.0 {
                    outline_colors[id] = col;
                    outline_widths[id][0] = w_px;
                    max_outline_px = max_outline_px.max(w_px);
                    outlined_meshes.push(idx);
                    id as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let [r, g, b, _] = draw.color.to_array();
            let [er, eg, eb, _] = draw.edge_color.to_array();
            let style = [
                draw.offset.x,
                draw.offset.y,
                draw.offset.z,
                outline_id,
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                draw.alpha.clamp(0.0, 1.0),
                er as f32 / 255.0,
                eg as f32 / 255.0,
                eb as f32 / 255.0,
                draw.wireframe_width_px.max(0.0),
                if draw.show_faces { 1.0 } else { 0.0 },
                shading_code(draw.shading),
                draw.edge_alpha.clamp(0.0, 1.0),
                0.0,
            ];
            queue.write_buffer(&entry.style_buf, 0, bytemuck::cast_slice(&style));

            let local_c = frame_to_local.transform_point3(entry.center) + draw.offset;
            let view_z = camera.rotate(local_c - camera.origin).z;
            view_z_min = view_z_min.min(view_z - entry.radius);
            view_z_max = view_z_max.max(view_z + entry.radius);

            if is_opaque_mesh(draw) {
                opaque_meshes.push(idx);
            } else {
                translucent_meshes.push((view_z, idx));
            }
        }

        // Partition filled triangles, lines, and discs into `[opaque..., translucent...]`.
        let mut opaque_poly_verts = Vec::new();
        let mut translucent_poly_verts = Vec::new();
        for p in poly_draws {
            let color = unmultiplied_rgba(p.color, p.alpha);
            if color[3] <= 0.004 {
                continue;
            }
            for &pt in &p.pts {
                let zp = camera.rotate(pt - camera.origin).z;
                view_z_min = view_z_min.min(zp);
                view_z_max = view_z_max.max(zp);
            }
            let dest = if color[3] >= OPAQUE_ALPHA_THRESHOLD {
                &mut opaque_poly_verts
            } else {
                &mut translucent_poly_verts
            };
            for &pt in &p.pts {
                dest.push(GpuVertex {
                    position: pt.into(),
                    face_normal: [0.0, 0.0, 1.0],
                    vertex_normal: [0.0, 0.0, 1.0],
                    color,
                    edge_color: [1.0, 1.0, 1.0],
                });
            }
        }
        let opaque_poly_vert_count = opaque_poly_verts.len() as u32;
        opaque_poly_verts.extend(translucent_poly_verts);
        let total_poly_vert_count =
            self.poly_buf.upload(&device, &queue, "gpu_poly_vertices", &opaque_poly_verts);
        let translucent_poly_vert_count = total_poly_vert_count - opaque_poly_vert_count;

        let mut opaque_lines = Vec::new();
        let mut translucent_lines = Vec::new();
        for l in line_draws {
            if l.width_pt <= 0.0 {
                continue;
            }
            let color = unmultiplied_rgba(l.color, l.alpha);
            let border_color = unmultiplied_rgba(l.border_color, l.border_alpha);
            if color[3] <= 0.004 && (l.border_pt <= 0.0 || border_color[3] <= 0.004) {
                continue;
            }
            let za = camera.rotate(l.a - camera.origin).z;
            let zb = camera.rotate(l.b - camera.origin).z;
            view_z_min = view_z_min.min(za.min(zb));
            view_z_max = view_z_max.max(za.max(zb));
            let join_mask = (u8::from(l.prev.is_some()) | (u8::from(l.next.is_some()) << 1)) as f32;
            let [dash_on, dash_off] = l.dash.unwrap_or([0.0, 0.0]);
            let inst = GpuLineInstance {
                a: l.a.into(),
                b: l.b.into(),
                prev: l.prev.unwrap_or(l.a).into(),
                next: l.next.unwrap_or(l.b).into(),
                color,
                params: [l.width_pt, join_mask, dash_on.max(0.0), dash_off.max(0.0)],
                border_color,
                border_pt: l.border_pt.max(0.0),
            };
            if color[3] >= OPAQUE_ALPHA_THRESHOLD {
                opaque_lines.push(inst);
            } else {
                translucent_lines.push(inst);
            }
        }
        let opaque_line_count = opaque_lines.len() as u32;
        opaque_lines.extend(translucent_lines);
        let total_line_count = self.line_buf.upload(&device, &queue, "gpu_line_instances", &opaque_lines);
        let translucent_line_count = total_line_count - opaque_line_count;

        let mut opaque_discs = Vec::new();
        let mut translucent_discs = Vec::new();
        for d in disc_draws {
            if d.radius_pt <= 0.0 {
                continue;
            }
            let fill_color = unmultiplied_rgba(d.fill_color, d.fill_alpha);
            let outline_color = unmultiplied_rgba(d.outline_color, d.outline_alpha);
            if fill_color[3] <= 0.004 && (d.outline_pt <= 0.0 || outline_color[3] <= 0.004) {
                continue;
            }
            let zc = camera.rotate(d.center - camera.origin).z;
            let r_world = if d.shaded_3d { d.radius_pt / camera.zoom.max(1e-4) } else { 0.0 };
            view_z_min = view_z_min.min(zc - r_world);
            view_z_max = view_z_max.max(zc + r_world);
            let inst = GpuDiscInstance {
                center: d.center.into(),
                radius_pt: d.radius_pt,
                outline_pt: d.outline_pt.max(0.0),
                fill_color,
                outline_color,
                shaded_3d: if d.shaded_3d { 1.0 } else { 0.0 },
            };
            if fill_color[3] >= OPAQUE_ALPHA_THRESHOLD {
                opaque_discs.push(inst);
            } else {
                translucent_discs.push(inst);
            }
        }
        let opaque_disc_count = opaque_discs.len() as u32;
        opaque_discs.extend(translucent_discs);
        let total_disc_count = self.disc_buf.upload(&device, &queue, "gpu_disc_instances", &opaque_discs);
        let translucent_disc_count = total_disc_count - opaque_disc_count;

        if opaque_meshes.is_empty()
            && translucent_meshes.is_empty()
            && total_poly_vert_count == 0
            && total_line_count == 0
            && total_disc_count == 0
        {
            return None;
        }

        if !view_z_min.is_finite() || !view_z_max.is_finite() {
            view_z_min = -32.0;
            view_z_max = 32.0;
        }
        let half_depth_a = view_z_min.abs().max(view_z_max.abs()).max(32.0) + 16.0;
        let vp = camera.view_projection_matrix(rect, half_depth_a);
        let clip_z_near = 0.5 - 0.5 * (view_z_max / half_depth_a);
        let clip_z_far = 0.5 - 0.5 * (view_z_min / half_depth_a);
        let inv_clip_z_span = 1.0 / (clip_z_far - clip_z_near).max(1e-5);
        let peel_z_eps = (0.03 / (2.0 * half_depth_a)).clamp(2e-6, 5e-4);
        let r_clip_z_per_pt = 0.5 / (camera.zoom.max(1e-4) * half_depth_a);
        let world_light = Vec3::new(0.3, 0.5, 0.8).normalize();
        let local_light = frame_to_local.transform_vector3(world_light);
        let view_light = camera.rotate(local_light).normalize_or(world_light);
        let camera_data = pack_camera_uniform(
            vp,
            frame_to_local,
            [clip_z_near, clip_z_far, inv_clip_z_span, peel_z_eps],
            [rect.width(), rect.height(), 1.0 / pixels_per_point, r_clip_z_per_pt],
            [view_light.x, view_light.y, view_light.z, 0.0],
        );
        queue.write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&camera_data));

        let mode = match transparency {
            TransparencyMode::Wboit if self.wboit_pipelines.is_some() => TransparencyMode::Wboit,
            TransparencyMode::Wboit => TransparencyMode::DepthPeeling,
            other => other,
        };

        let has_translucent = !translucent_meshes.is_empty()
            || translucent_poly_vert_count > 0
            || translucent_line_count > 0
            || translucent_disc_count > 0;
        if has_translucent {
            let target = self.target.as_mut().expect("ensure_target sets target");
            match mode {
                TransparencyMode::Wboit => Self::ensure_wboit_targets(&device, &self.wboit_composite_bgl, target),
                TransparencyMode::DepthPeeling => {
                    Self::ensure_peel_targets(&device, &self.peel_depth_bgl, &self.copy_bgl, target);
                }
                TransparencyMode::CentroidSorted => {}
            }
        }
        if !outlined_meshes.is_empty() {
            outline_widths[0][1] = max_outline_px;
            queue.write_buffer(&self.outline_buf, 0, bytemuck::cast_slice(&outline_colors));
            queue.write_buffer(
                &self.outline_buf,
                (256 * 16) as u64,
                bytemuck::cast_slice(&outline_widths),
            );
            let target = self.target.as_mut().expect("ensure_target sets target");
            Self::ensure_outline_targets(&device, &self.outline_bgl, &self.outline_buf, target);
        }

        let target = self.target.as_ref().expect("ensure_target always sets it");
        let clear_bg = srgb_color_to_linear_wgpu(viewport_bg);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gpu_surface_encoder") });

        let draw_meshes = |pass: &mut wgpu::RenderPass<'_>, pipelines: &[wgpu::RenderPipeline; 3], indices: &[usize]| {
            for &idx in indices {
                let draw = &mesh_draws[idx];
                let entry = &self.meshes[&(dataset_instance_id, draw.key)];
                if let Some(vb) = &entry.vertex_buf {
                    pass.set_pipeline(&pipelines[culling_index(draw.culling)]);
                    pass.set_bind_group(1, &entry.style_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.draw(0..entry.vertex_count, 0..1);
                }
            }
        };
        let draw_polys = |pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline, start: u32, end: u32| {
            if end > start
                && let Some(vb) = &self.poly_buf.buf
            {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &self.poly_style_bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(start..end, 0..1);
            }
        };
        let draw_instanced = |pass: &mut wgpu::RenderPass<'_>,
                              pipeline: &wgpu::RenderPipeline,
                              buf: &Option<wgpu::Buffer>,
                              start: u32,
                              end: u32| {
            if end > start
                && let Some(vb) = buf
            {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..6, start..end);
            }
        };

        // Pass 1: Clear offscreen color to opaque viewport_bg + opaque depth, and render opaque geometry.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gpu_opaque_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear_bg), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.opaque_depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            draw_meshes(&mut pass, &self.opaque_pipelines, &opaque_meshes);
            draw_polys(&mut pass, &self.opaque_pipelines[0], 0, opaque_poly_vert_count);
            draw_instanced(&mut pass, &self.line_pipelines.opaque, &self.line_buf.buf, 0, opaque_line_count);
            draw_instanced(&mut pass, &self.disc_pipelines.opaque, &self.disc_buf.buf, 0, opaque_disc_count);
        }

        // Pass 2: Translucent geometry resolved via the selected transparency mode.
        if has_translucent {
            let trans_idx: Vec<usize> = match mode {
                TransparencyMode::CentroidSorted => {
                    translucent_meshes.sort_by(|a, b| a.0.total_cmp(&b.0));
                    translucent_meshes.iter().map(|&(_, idx)| idx).collect()
                }
                TransparencyMode::Wboit | TransparencyMode::DepthPeeling => {
                    translucent_meshes.iter().map(|&(_, idx)| idx).collect()
                }
            };

            match mode {
                TransparencyMode::CentroidSorted => {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("gpu_sorted_translucent_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &target.color_view,
                            resolve_target: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &target.opaque_depth_view,
                            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    draw_meshes(&mut pass, &self.sorted_pipelines, &trans_idx);
                    draw_polys(&mut pass, &self.sorted_pipelines[0], opaque_poly_vert_count, total_poly_vert_count);
                    draw_instanced(&mut pass, &self.line_pipelines.sorted, &self.line_buf.buf, opaque_line_count, total_line_count);
                    draw_instanced(&mut pass, &self.disc_pipelines.sorted, &self.disc_buf.buf, opaque_disc_count, total_disc_count);
                }
                TransparencyMode::Wboit => {
                    let wboit = target.wboit.as_ref().expect("wboit targets ensured");
                    let wboit_pipelines = self.wboit_pipelines.as_ref().expect("wboit pipelines checked");
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("gpu_wboit_accum_pass"),
                            color_attachments: &[
                                Some(wgpu::RenderPassColorAttachment {
                                    view: &wboit.accum_view,
                                    resolve_target: None,
                                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                                }),
                                Some(wgpu::RenderPassColorAttachment {
                                    view: &wboit.reveal_view,
                                    resolve_target: None,
                                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                                }),
                            ],
                            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                                view: &target.opaque_depth_view,
                                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                                stencil_ops: None,
                            }),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        pass.set_bind_group(0, &self.camera_bind_group, &[]);
                        draw_meshes(&mut pass, wboit_pipelines, &trans_idx);
                        draw_polys(&mut pass, &wboit_pipelines[0], opaque_poly_vert_count, total_poly_vert_count);
                        if let Some(lp) = &self.line_pipelines.wboit {
                            draw_instanced(&mut pass, lp, &self.line_buf.buf, opaque_line_count, total_line_count);
                        }
                        if let Some(dp) = &self.disc_pipelines.wboit {
                            draw_instanced(&mut pass, dp, &self.disc_buf.buf, opaque_disc_count, total_disc_count);
                        }
                    }
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("gpu_wboit_composite_pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &target.color_view,
                                resolve_target: None,
                                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        pass.set_pipeline(&self.wboit_composite_pipeline);
                        pass.set_bind_group(0, &wboit.composite_bind_group, &[]);
                        pass.draw(0..3, 0..1);
                    }
                }
                TransparencyMode::DepthPeeling => {
                    let peel = target.peel.as_ref().expect("peel targets ensured");
                    // Clear previous peel depth to `0.0` so layer 0 keeps all front-most translucent fragments.
                    {
                        let _clear_prev = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("gpu_peel_clear_init_depth"),
                            color_attachments: &[],
                            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                                view: &peel.depth_views[1],
                                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                                stencil_ops: None,
                            }),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                    }
                    let layers = peel_layers.clamp(1, 12) as usize;
                    for layer in 0..layers {
                        let slot = layer % 2;
                        {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("gpu_peel_layer_pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &peel.peel_color_view,
                                    resolve_target: None,
                                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                                })],
                                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                                    view: &peel.depth_views[slot],
                                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                                    stencil_ops: None,
                                }),
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });
                            pass.set_bind_group(0, &self.camera_bind_group, &[]);
                            pass.set_bind_group(2, &peel.depth_bind_groups[slot], &[]);
                            draw_meshes(&mut pass, &self.peel_pipelines, &trans_idx);
                            draw_polys(&mut pass, &self.peel_pipelines[0], opaque_poly_vert_count, total_poly_vert_count);
                            pass.set_bind_group(1, &peel.depth_bind_groups[slot], &[]);
                            draw_instanced(&mut pass, &self.line_pipelines.peel, &self.line_buf.buf, opaque_line_count, total_line_count);
                            draw_instanced(&mut pass, &self.disc_pipelines.peel, &self.disc_buf.buf, opaque_disc_count, total_disc_count);
                        }
                        {
                            let accum_load = if layer == 0 {
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                            } else {
                                wgpu::LoadOp::Load
                            };
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("gpu_peel_under_composite"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &peel.peel_accum_view,
                                    resolve_target: None,
                                    ops: wgpu::Operations { load: accum_load, store: wgpu::StoreOp::Store },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });
                            pass.set_pipeline(&self.peel_under_pipeline);
                            pass.set_bind_group(0, &peel.peel_color_bg, &[]);
                            pass.draw(0..3, 0..1);
                        }
                    }
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("gpu_peel_over_composite"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &target.color_view,
                                resolve_target: None,
                                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        pass.set_pipeline(&self.peel_over_pipeline);
                        pass.set_bind_group(0, &peel.peel_accum_bg, &[]);
                        pass.draw(0..3, 0..1);
                    }
                }
            }
        }

        if !outlined_meshes.is_empty() {
            let outline = target.outline.as_ref().expect("outline targets ensured");
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gpu_outline_id_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &outline.id_view,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &outline.depth_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                draw_meshes(&mut pass, &self.outline_id_pipelines, &outlined_meshes);
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gpu_outline_composite_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.color_view,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.outline_composite_pipeline);
                pass.set_bind_group(0, &outline.composite_bg, &[]);
                pass.draw(0..3, 0..1);
            }
        }

        queue.submit(Some(encoder.finish()));
        Some(target.texture_id)
    }
}
