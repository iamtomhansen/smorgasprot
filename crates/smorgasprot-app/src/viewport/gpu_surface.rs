//! GPU-rendered molecular surfaces: the dominant triangle count in the Complex tab, moved off
//! the CPU `DrawList` painter's algorithm and onto a depth-tested wgpu pipeline. Everything else
//! (cartoon, spheres, interface geometry) stays on `DrawList`; see `complex_scene::paint`. Note
//! this doesn't fix those CPU layers always painting over the whole GPU surface quad regardless
//! of true relative depth — that needs those layers to join this same render target, which is
//! future work, not this round's.
//!
//! Depth handling is a conventional opaque/transparent split, not Weighted Blended OIT (tried in
//! an earlier round and reverted): a surface at `alpha ~= 1` uses a normal depth-tested,
//! depth-writing pipeline, so triangle submission order stops mattering entirely — exactly the
//! guarantee OIT's weighted-average approach can never give, no matter how it's tuned, since it
//! has no concept of one fragment fully occluding another. A translucent surface (`alpha < 1`)
//! depth-*tests* (so it's still properly hidden behind nearer opaque geometry) but doesn't
//! depth-*write*, so overlapping translucent fragments don't fight each other for the z-buffer
//! (no patchwork/holes); they blend in draw-submission order rather than true back-to-front
//! order, a conventional, much smaller residual approximation.
//!
//! One `GpuSurfaceRenderer` is built once at startup from `cc.wgpu_render_state` and kept on
//! `SmorgasprotApp`; `complex_scene::add_chain_surfaces` falls back to the CPU triangle path
//! when it's `None` (no wgpu backend available).

use crate::camera::CameraState;
use crate::state::SurfaceCulling;
use egui::{Color32, Rect};
use glam::Vec3;
use smorgasprot_core::surface::{GeodesicPreset, SurfaceParams};
use smorgasprot_core::SurfaceMesh;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

const SHADER_SRC: &str = include_str!("gpu_surface.wgsl");
/// Offscreen color format; must be `*Srgb` per `egui_wgpu::Renderer::register_native_texture`'s
/// contract (see the shader's `srgb_decode_channel` comment for why this round-trips correctly).
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Alpha at or above which a chain is treated as opaque (depth-writing) rather than translucent
/// (depth-testing only) — see the module doc for why this split replaces OIT.
const OPAQUE_ALPHA_THRESHOLD: f32 = 0.999;

/// One chain's surface, projected once at upload time and re-drawn every frame until stale.
/// Position + a per-triangle (not per-vertex) averaged normal, duplicated across that triangle's
/// 3 vertices so flat facet shading falls out of ordinary interpolation — see the shader comment.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    face_normal: [f32; 3],
}

struct ChainEntry {
    vertex_buf: wgpu::Buffer,
    vertex_count: u32,
    style_buf: wgpu::Buffer,
    style_bind_group: wgpu::BindGroup,
    preset: GeodesicPreset,
    surface_params: SurfaceParams,
}

/// Offscreen render target, resized to match the viewport rect.
struct Target {
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    texture_id: egui::TextureId,
    size: (u32, u32),
}

/// One chain surface to draw this frame; mirrors `complex_scene::add_chain_surfaces`'s per-chain
/// styling inputs exactly, so the GPU and CPU paths compute identical colors.
pub struct GpuChainDraw<'a> {
    pub chain_idx: usize,
    pub mesh: &'a SurfaceMesh,
    pub preset: GeodesicPreset,
    pub surface_params: &'a SurfaceParams,
    /// Plane-local explode offset (`chain_total_offset`'s result), added after the shared
    /// world→plane-local `frame` matrix in `render`'s `frame_to_local`.
    pub offset: Vec3,
    pub color: Color32,
    pub alpha: f32,
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
    chains: HashMap<(String, usize), ChainEntry>,
}

fn culling_index(culling: SurfaceCulling) -> usize {
    match culling {
        SurfaceCulling::None => 0,
        SurfaceCulling::Backfaces => 1,
        SurfaceCulling::Frontfaces => 2,
    }
}

/// `culling_index`'s 3 variants, doubled for opaque (depth-writing) vs translucent (depth-test
/// only) — see the module doc for why the split exists instead of OIT.
fn pipeline_index(culling: SurfaceCulling, alpha: f32) -> usize {
    let opaque = alpha >= OPAQUE_ALPHA_THRESHOLD;
    culling_index(culling) * 2 + usize::from(!opaque)
}

impl GpuSurfaceRenderer {
    /// Builds the pipelines and static bind group layouts from `cc.wgpu_render_state`. Callers
    /// (`SmorgasprotApp::new`) only call this when that's `Some` — on the glow escape hatch,
    /// or a wgpu init failure, they keep `None` and `complex_scene` falls back to the CPU
    /// `DrawList` path for surfaces.
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
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let chain_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_surface_chain_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                // `offset` is consumed by the vertex stage, `color` by the fragment stage.
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
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
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, front_face: wgpu::FrontFace::Ccw, cull_mode, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    // `project()`'s "larger local-z is nearer" becomes wgpu's smaller-is-nearer
                    // via `CameraState::view_projection_matrix`, so nearer still wins with `Less`.
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
                        // Premultiplied alpha, matching egui's own image blend state. A no-op for
                        // the opaque (alpha=1) variant since the destination term zeroes out.
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
        // Indices must match `pipeline_index`'s `(culling, opaque)` → pipeline mapping:
        // culling_index * 2 + (0 for opaque, 1 for translucent).
        let pipelines = [
            make_pipeline(None, true, "gpu_surface_pipeline_no_cull_opaque"),
            make_pipeline(None, false, "gpu_surface_pipeline_no_cull_translucent"),
            make_pipeline(Some(wgpu::Face::Back), true, "gpu_surface_pipeline_cull_back_opaque"),
            make_pipeline(Some(wgpu::Face::Back), false, "gpu_surface_pipeline_cull_back_translucent"),
            make_pipeline(Some(wgpu::Face::Front), true, "gpu_surface_pipeline_cull_front_opaque"),
            make_pipeline(Some(wgpu::Face::Front), false, "gpu_surface_pipeline_cull_front_translucent"),
        ];

        // `view_proj` (camera-only) + `frame` (world→plane-local), two `mat4x4<f32>`.
        let identity_cols = glam::Mat4::IDENTITY.to_cols_array();
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_surface_camera_buf"),
            // glam 0.29 has no `bytemuck` feature, so matrices go through `to_cols_array`
            // (column-major `[f32; 16]`, matching WGSL's `mat4x4<f32>` layout exactly).
            contents: bytemuck::cast_slice(&[identity_cols, identity_cols].concat()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_surface_camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buf.as_entire_binding() }],
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

    /// Rebuilds a chain's GPU mesh from `mesh` if it's missing or `preset`/`surface_params` changed.
    fn ensure_chain(&mut self, key: (String, usize), mesh: &SurfaceMesh, preset: GeodesicPreset, surface_params: &SurfaceParams) {
        if let Some(entry) = self.chains.get(&key) {
            if entry.preset == preset && &entry.surface_params == surface_params {
                return;
            }
        }
        let mut vertices = Vec::with_capacity(mesh.triangles.len() * 3);
        for &[i0, i1, i2] in &mesh.triangles {
            let (v0, v1, v2) = (i0 as usize, i1 as usize, i2 as usize);
            let n = (Vec3::from(mesh.normals[v0]) + Vec3::from(mesh.normals[v1]) + Vec3::from(mesh.normals[v2])) / 3.0;
            for v in [v0, v1, v2] {
                vertices.push(GpuVertex { position: mesh.vertices[v], face_normal: n.into() });
            }
        }
        let device = &self.render_state.device;
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_surface_chain_vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let style_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu_surface_chain_style"),
            // `offset: vec4<f32>` + `color: vec4<f32>`; actual values are written every frame.
            contents: bytemuck::cast_slice(&[0.0_f32; 8]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let style_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_surface_chain_style_bind_group"),
            layout: &self.chain_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: style_buf.as_entire_binding() }],
        });
        self.chains.insert(
            key,
            ChainEntry { vertex_buf, vertex_count: vertices.len() as u32, style_buf, style_bind_group, preset, surface_params: surface_params.clone() },
        );
    }

    /// Renders every chain in `draws` into the offscreen target and returns its texture, or
    /// `None` if `rect` has no area or `draws` is empty (nothing to display). `frame_to_local`
    /// is `PlaneFrame::to_local_matrix()` for the view's active interface frame (or the identity
    /// matrix via `PlaneFrame::world_centered`'s equivalent, when there isn't one).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        rect: Rect,
        pixels_per_point: f32,
        camera: &CameraState,
        half_depth_a: f32,
        frame_to_local: glam::Mat4,
        dataset_id: &str,
        draws: &[GpuChainDraw<'_>],
    ) -> Option<egui::TextureId> {
        if draws.is_empty() || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let size = (((rect.width() * pixels_per_point).round() as u32).max(1), ((rect.height() * pixels_per_point).round() as u32).max(1));
        self.ensure_target(size);
        // Only the active dataset's chains are ever drawn, so stale entries from a previously
        // active one are safe to drop rather than tracked for incremental eviction.
        self.chains.retain(|(id, _), _| id == dataset_id);

        // Owned (wgpu's handle types are internally `Arc`-based) so `ensure_chain`'s `&mut self`
        // below doesn't conflict with holding this across the loop.
        let queue = self.render_state.queue.clone();
        let vp = camera.view_projection_matrix(rect, half_depth_a);
        let camera_data = [vp.to_cols_array(), frame_to_local.to_cols_array()].concat();
        queue.write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&camera_data));

        for draw in draws {
            let key = (dataset_id.to_owned(), draw.chain_idx);
            self.ensure_chain(key.clone(), draw.mesh, draw.preset, draw.surface_params);
            let entry = &self.chains[&key];
            let [r, g, b, _] = draw.color.to_array();
            let style = [
                draw.offset.x,
                draw.offset.y,
                draw.offset.z,
                0.0,
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                draw.alpha.clamp(0.0, 1.0),
            ];
            queue.write_buffer(&entry.style_buf, 0, bytemuck::cast_slice(&style));
        }

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
            for draw in draws {
                let key = (dataset_id.to_owned(), draw.chain_idx);
                let entry = &self.chains[&key];
                pass.set_pipeline(&self.pipelines[pipeline_index(draw.culling, draw.alpha)]);
                pass.set_bind_group(1, &entry.style_bind_group, &[]);
                pass.set_vertex_buffer(0, entry.vertex_buf.slice(..));
                pass.draw(0..entry.vertex_count, 0..1);
            }
        }
        queue.submit(Some(encoder.finish()));

        Some(target.texture_id)
    }
}
