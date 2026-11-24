//! GPU-rendered molecular surfaces on a depth-tested `wgpu` pipeline.
//!
//! Uses an opaque/translucent two-pass split: opaque surfaces (`alpha >= OPAQUE_ALPHA_THRESHOLD`
//! with faces visible) write depth and draw first; translucent and wireframe-only surfaces draw
//! second, sorted back-to-front by chain centroid depth, with depth testing enabled and depth
//! writes disabled.

use crate::camera::CameraState;
use crate::state::SurfaceCulling;
use egui::{Color32, Rect};
use glam::Vec3;
use smorgasprot_core::SurfaceMesh;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

const SHADER_SRC: &str = include_str!("surface.wgsl");
/// Offscreen color format; must be `*Srgb` per `egui_wgpu::Renderer::register_native_texture`.
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Alpha at or above which a filled surface is treated as opaque (depth-writing).
const OPAQUE_ALPHA_THRESHOLD: f32 = 0.999;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    face_normal: [f32; 3],
}

struct ChainEntry {
    vertex_buf: Option<wgpu::Buffer>,
    vertex_count: u32,
    centroid: Vec3,
    style_buf: wgpu::Buffer,
    style_bind_group: wgpu::BindGroup,
    draw_gen: u64,
}

/// Offscreen render target, resized to match the viewport rect.
struct Target {
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    texture_id: egui::TextureId,
    size: (u32, u32),
}

/// One chain surface to draw this frame; mirrors `complex_scene::add_chain_surfaces` inputs.
pub struct GpuChainDraw<'a> {
    pub chain_idx: usize,
    pub mesh: &'a SurfaceMesh,
    pub offset: Vec3,
    pub color: Color32,
    pub edge_color: Color32,
    pub alpha: f32,
    pub show_faces: bool,
    pub wireframe_width_px: f32,
    pub culling: SurfaceCulling,
}

pub struct GpuSurfaceRenderer {
    render_state: egui_wgpu::RenderState,
    // Indexed by `pipeline_index`: `SurfaceCulling`'s 3 variants × opaque/translucent.
    pipelines: [wgpu::RenderPipeline; 6],
    chain_bind_group_layout: wgpu::BindGroupLayout,
    camera_buf: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    target: Option<Target>,
    chains: HashMap<(u64, usize), ChainEntry>,
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

fn pipeline_index(culling: SurfaceCulling, is_opaque_pass: bool) -> usize {
    culling_index(culling) * 2 + usize::from(!is_opaque_pass)
}

fn is_opaque_draw(draw: &GpuChainDraw<'_>) -> bool {
    draw.show_faces && draw.alpha >= OPAQUE_ALPHA_THRESHOLD
}

impl GpuSurfaceRenderer {
    /// Builds pipelines and static bind-group layouts from `cc.wgpu_render_state`.
    pub fn new(render_state: &egui_wgpu::RenderState) -> Self {
        let device = &render_state.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu_surface"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_surface_camera_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let chain_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_surface_chain_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu_surface_pipeline_layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &chain_bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
        };

        let make_pipeline = |cull_mode: Option<wgpu::Face>, depth_write_enabled: bool, label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[vertex_layout.clone()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode,
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
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(wgpu::BlendState {
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
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                multiview: None,
                cache: None,
            })
        };

        let pipelines = [
            make_pipeline(None, true, "gpu_surface_pipeline_no_cull_opaque"),
            make_pipeline(None, false, "gpu_surface_pipeline_no_cull_translucent"),
            make_pipeline(Some(wgpu::Face::Back), true, "gpu_surface_pipeline_cull_back_opaque"),
            make_pipeline(Some(wgpu::Face::Back), false, "gpu_surface_pipeline_cull_back_translucent"),
            make_pipeline(Some(wgpu::Face::Front), true, "gpu_surface_pipeline_cull_front_opaque"),
            make_pipeline(Some(wgpu::Face::Front), false, "gpu_surface_pipeline_cull_front_translucent"),
        ];

        let identity_cols = glam::Mat4::IDENTITY.to_cols_array();
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_surface_camera_buf"),
            contents: bytemuck::cast_slice(&[identity_cols, identity_cols].concat()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_surface_camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        Self {
            render_state: render_state.clone(),
            pipelines,
            chain_bind_group_layout,
            camera_buf,
            camera_bind_group,
            target: None,
            chains: HashMap::new(),
        }
    }

    /// (Re)creates the offscreen target if `size` changed, registering/updating its egui texture.
    fn ensure_target(&mut self, size: (u32, u32)) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let device = &self.render_state.device;
        let (w, h) = size;
        let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
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
            label: Some("gpu_surface_depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let mut renderer = self.render_state.renderer.write();
        let texture_id = match &mut self.target {
            Some(prev) => {
                renderer.update_egui_texture_from_wgpu_texture(device, &color_view, wgpu::FilterMode::Linear, prev.texture_id);
                prev.texture_id
            }
            None => renderer.register_native_texture(device, &color_view, wgpu::FilterMode::Linear),
        };
        drop(renderer);
        self.target = Some(Target { color_view, depth_view, texture_id, size });
    }

    /// Rebuilds a chain's GPU vertex buffer when missing or when `draw_gen` advances.
    fn ensure_chain(&mut self, key: (u64, usize), mesh: &SurfaceMesh, draw_gen: u64) {
        if self.chains.get(&key).is_some_and(|e| e.draw_gen == draw_gen) {
            return;
        }
        let vert_limit = mesh.vertices.len().min(mesh.normals.len());
        let mut vertices = Vec::with_capacity(mesh.triangles.len() * 3);
        let mut pos_sum = Vec3::ZERO;
        for &[i0, i1, i2] in &mesh.triangles {
            let (v0, v1, v2) = (i0 as usize, i1 as usize, i2 as usize);
            if v0 >= vert_limit || v1 >= vert_limit || v2 >= vert_limit {
                continue;
            }
            let n = (Vec3::from(mesh.normals[v0]) + Vec3::from(mesh.normals[v1]) + Vec3::from(mesh.normals[v2])) / 3.0;
            for v in [v0, v1, v2] {
                let p = mesh.vertices[v];
                pos_sum += Vec3::from(p);
                vertices.push(GpuVertex { position: p, face_normal: n.into() });
            }
        }
        let vertex_count = vertices.len() as u32;
        let centroid = if vertex_count > 0 { pos_sum / (vertex_count as f32) } else { Vec3::ZERO };

        let device = &self.render_state.device;
        let vertex_buf = (!vertices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gpu_surface_chain_vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let style_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_surface_chain_style"),
            // `offset_and_faces` + `color` + `wireframe`, 3 × `vec4<f32>`.
            contents: bytemuck::cast_slice(&[0.0_f32; 12]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let style_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_surface_chain_style_bind_group"),
            layout: &self.chain_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: style_buf.as_entire_binding(),
            }],
        });
        self.chains.insert(
            key,
            ChainEntry { vertex_buf, vertex_count, centroid, style_buf, style_bind_group, draw_gen },
        );
    }

    /// Renders `draws` into the offscreen target and returns its egui `TextureId`, or `None` when
    /// `rect`/camera parameters are degenerate or nothing is drawable.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        rect: Rect,
        pixels_per_point: f32,
        camera: &CameraState,
        half_depth_a: f32,
        frame_to_local: glam::Mat4,
        dataset_instance_id: u64,
        draw_gen: u64,
        draws: &[GpuChainDraw<'_>],
    ) -> Option<egui::TextureId> {
        if draws.is_empty()
            || !rect.is_finite()
            || rect.width() <= 0.0
            || rect.height() <= 0.0
            || !pixels_per_point.is_finite()
            || pixels_per_point <= 0.0
            || !half_depth_a.is_finite()
            || half_depth_a <= 0.0
        {
            return None;
        }
        let max_dim = self.render_state.device.limits().max_texture_dimension_2d.max(1);
        let size = (
            ((rect.width() * pixels_per_point).round() as u32).clamp(1, max_dim),
            ((rect.height() * pixels_per_point).round() as u32).clamp(1, max_dim),
        );
        self.ensure_target(size);
        self.chains.retain(|&(id, _), _| id == dataset_instance_id);

        let queue = self.render_state.queue.clone();
        let vp = camera.view_projection_matrix(rect, half_depth_a);
        let camera_data = [vp.to_cols_array(), frame_to_local.to_cols_array()].concat();
        queue.write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&camera_data));

        let mut opaque_indices = Vec::with_capacity(draws.len());
        let mut translucent_indices: Vec<(f32, usize)> = Vec::new();

        for (idx, draw) in draws.iter().enumerate() {
            let key = (dataset_instance_id, draw.chain_idx);
            self.ensure_chain(key, draw.mesh, draw_gen);
            let entry = &self.chains[&key];
            if entry.vertex_count == 0 {
                continue;
            }
            let [r, g, b, _] = draw.color.to_array();
            let [er, eg, eb, _] = draw.edge_color.to_array();
            let style = [
                draw.offset.x,
                draw.offset.y,
                draw.offset.z,
                if draw.show_faces { 1.0 } else { 0.0 },
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                draw.alpha.clamp(0.0, 1.0),
                er as f32 / 255.0,
                eg as f32 / 255.0,
                eb as f32 / 255.0,
                draw.wireframe_width_px.max(0.0),
            ];
            queue.write_buffer(&entry.style_buf, 0, bytemuck::cast_slice(&style));

            if is_opaque_draw(draw) {
                opaque_indices.push(idx);
            } else {
                let local_c = frame_to_local.transform_point3(entry.centroid) + draw.offset;
                let view_z = camera.rotate(local_c).z;
                translucent_indices.push((view_z, idx));
            }
        }

        if opaque_indices.is_empty() && translucent_indices.is_empty() {
            return None;
        }
        // Paint translucent chains farthest-first so overlapping layers blend in proper order.
        translucent_indices.sort_by(|a, b| a.0.total_cmp(&b.0));

        let target = self.target.as_ref().expect("ensure_target always sets it");
        let device = &self.render_state.device;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gpu_surface_encoder") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gpu_surface_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.camera_bind_group, &[]);

            let draw_chain = |pass: &mut wgpu::RenderPass<'_>, draw: &GpuChainDraw<'_>, is_opaque: bool| {
                let entry = &self.chains[&(dataset_instance_id, draw.chain_idx)];
                if let Some(vb) = &entry.vertex_buf {
                    pass.set_pipeline(&self.pipelines[pipeline_index(draw.culling, is_opaque)]);
                    pass.set_bind_group(1, &entry.style_bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.draw(0..entry.vertex_count, 0..1);
                }
            };

            for &idx in &opaque_indices {
                draw_chain(&mut pass, &draws[idx], true);
            }
            for &(_, idx) in &translucent_indices {
                draw_chain(&mut pass, &draws[idx], false);
            }
        }
        queue.submit(Some(encoder.finish()));

        Some(target.texture_id)
    }
}
