//! Cartography ("Sandwich") tab scene: atoms morphing between true 3D and the flat 2D map, with
//! territories for residues and edges for contacts. All spatial layers render via the shared `wgpu`
//! depth-tested pipeline (with the CPU `DrawList` kept as a fallback for Glow); hover rings and text
//! labels always draw on top via `egui::Painter`.

use super::ball_stick_mesh::push_cylinder_segment;
use super::complex_scene::build_geodesic_spheres_mesh;
use super::draw_list::DrawList;
use super::grid;
use super::plane_space::PlaneFrame;
use crate::camera::CameraState;
use crate::dataset::{InterfaceViewData, ViewerDataset};
use crate::render::{GpuDiscDraw, GpuLineDraw, GpuMeshDraw, GpuMeshKey, GpuSurfaceRenderer};
use crate::state::{
    AtomDrawParams, AtomSizeMode, CartographyViewSettings, ColorLens, EdgeColorLens, EdgeWidthLens, GridParams,
    LensParams, NodeStrokeColorLens, PresentationStyle, RenderingSettings, RibbonStyle, Selection, SphereQuality,
    TerritoryColorLens, UiState,
};
use crate::style::{LabelPosition, Palette, lenses, palette, sizing};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, vec2};
use glam::Vec3;
use smorgasprot_core::projection2d::morph_sheet_with_strain;
use smorgasprot_core::{AtomNodeIdx, InterfaceEdge, InterfaceSide, ResidueTerritory, SurfaceMesh};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Sides at or below this opacity are not drawn.
const MIN_DRAW_ALPHA: f32 = 0.02;
/// Sides at or below this opacity (e.g. soloed out) are ghosted: no picking, springs, or arcs.
const MIN_INTERACTIVE_ALPHA: f32 = 0.05;
/// Maximum pointer distance (px) for hovering an atom.
const PICK_RADIUS_PX: f32 = 14.0;
/// View-ray tie-break offset (Å) separating coplanar Sandwich layers above `peel_z_eps` (0.03 Å).
const COPLANAR_LAYER_STEP_A: f32 = 0.06;

// Hinge (open-book butterfly line)
/// Half-length (Å) of the drawn open-book hinge line.
const HINGE_HALF_LEN_A: f32 = 28.0;
const HINGE_WIDTH_PX: f32 = 1.5;

// Backbone trace (`paint_backbone`)
/// The faint backbone trace dims by this factor on top of the flatten-progress fade.
const BACKBONE_ALPHA_SCALE: f32 = 0.45;
/// Backbone opacity at `flatten.value == 1.0` (fades linearly from full opacity at `0.0`).
const BACKBONE_FADE_MIN: f32 = 0.15;
const BACKBONE_WIDTH_ZOOM_SCALE: f32 = 0.16;
const BACKBONE_MIN_WIDTH_PX: f32 = 2.0;

// Territory border (`paint_territories`)
const TERRITORY_BORDER_ALPHA_SCALE: f32 = 0.75;

// Contact edges (`paint_edges`)
const INTRA_EDGE_WIDTH_SCALE: f32 = 0.85;
const INTRA_EDGE_ALPHA_SCALE: f32 = 0.45;
const CROSS_ARC_ALPHA_SCALE: f32 = 0.86;

// Atom hover ring (`paint_atoms`)
const HOVER_RING_OFFSET_PX: f32 = 3.0;
const HOVER_RING_WIDTH_PX: f32 = 1.5;

// Labels (`paint_labels`)
const TERRITORY_FONT_ZOOM_SCALE: f32 = 0.22;
const TERRITORY_FONT_MIN_PX: f32 = 9.0;
const TERRITORY_FONT_MAX_PX: f32 = 14.0;
const TERRITORY_LABEL_ALPHA_SCALE: f32 = 0.90;
const ATOM_FONT_ZOOM_SCALE: f32 = 0.18;
const ATOM_FONT_MIN_PX: f32 = 8.5;
const ATOM_FONT_MAX_PX: f32 = 12.5;
const ATOM_LABEL_ALPHA_SCALE: f32 = 0.95;
/// Gap (px) between an atom disc's edge and its label.
const ATOM_LABEL_OFFSET_PX: f32 = 2.0;

/// Shared per-frame inputs for every layer.
struct Ctx<'a> {
    painter: &'a Painter,
    rect: Rect,
    ds: &'a ViewerDataset,
    iface: &'a InterfaceViewData,
    camera: &'a CameraState,
    frame: PlaneFrame,
    s: &'a CartographyViewSettings,
    lenses: &'a LensParams,
    atom_draw: &'a AtomDrawParams,
    grid: &'a GridParams,
    grid_color: Color32,
    pal: &'a Palette,
}

impl Ctx<'_> {
    /// Morphs a point into plane-local `(u, v, n)` under the current flatten/butterfly state.
    fn morph_local(&self, world: [f32; 3], uv: [f32; 2], side: InterfaceSide) -> Vec3 {
        self.frame.morph(world, uv, side, self.s.flatten.value, self.s.book.value)
    }

    /// Morphs a point and projects it to screen `(position, depth)` (larger depth is nearer).
    fn morph(&self, world: [f32; 3], uv: [f32; 2], side: InterfaceSide) -> (Pos2, f32) {
        self.camera.project(self.morph_local(world, uv, side), self.rect)
    }
}

/// Screen `(position, depth)` of every interface node under the current morph, indexed like `map.nodes`.
pub fn project_nodes(ds: &ViewerDataset, rect: Rect, s: &CartographyViewSettings) -> Vec<(Pos2, f32)> {
    let Some(iface) = ds.active_interface() else { return Vec::new() };
    let frame = PlaneFrame::from(&iface.map);
    iface
        .map
        .nodes
        .iter()
        .map(|n| {
            let uv = if s.use_relaxed_uv { n.pos_uv } else { n.pos_uv_raw };
            let local = frame.morph(n.pos_3d, uv, n.side, s.flatten.value, s.book.value);
            s.camera.project(local, rect)
        })
        .collect()
}

/// Nearest visible node within [`PICK_RADIUS_PX`] of `pointer`.
pub fn pick_atom(ds: &ViewerDataset, s: &CartographyViewSettings, nodes: &[(Pos2, f32)], pointer: Pos2) -> Option<AtomNodeIdx> {
    if !s.atoms.visible {
        return None;
    }
    let iface = ds.active_interface()?;
    nodes
        .iter()
        .zip(&iface.map.nodes)
        .filter(|(_, n)| s.side_alpha(n.side) > MIN_INTERACTIVE_ALPHA)
        .map(|(&(p, _), n)| (p.distance_sq(pointer), n.idx))
        .filter(|(d2, _)| *d2 < PICK_RADIUS_PX * PICK_RADIUS_PX)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, idx)| idx)
}

/// Paints the Cartography ("Sandwich") tab; `nodes` comes from [`project_nodes`] for this frame.
pub fn paint(
    painter: &Painter,
    rect: Rect,
    ds: &ViewerDataset,
    state: &UiState,
    rendering: &RenderingSettings,
    nodes: &[(Pos2, f32)],
    gpu_surfaces: Option<&mut GpuSurfaceRenderer>,
) {
    let Some(iface) = ds.active_interface() else {
        grid::paint(painter, rect, state.active_camera(), &state.view_params.grid, state.view_params.grid_color);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No active interface selected — select or create an interface in the Slice tab.",
            FontId::proportional(14.0),
            palette::with_alpha(state.palette.chrome.ink, 0.55),
        );
        return;
    };
    let s = &state.cartography;
    let cx = Ctx {
        painter,
        rect,
        ds,
        iface,
        camera: &s.camera,
        frame: PlaneFrame::from(&iface.map),
        s,
        lenses: &state.palette.thresholds,
        atom_draw: &state.view_params.atom_draw,
        grid: &state.view_params.grid,
        grid_color: state.view_params.grid_color,
        pal: &state.palette,
    };

    let hover_ring = if let Some(gpu) = gpu_surfaces
        && let Some(ring) = paint_gpu_scene(&cx, &state.selection, rendering, nodes, gpu)
    {
        ring
    } else {
        paint_cpu_scene(&cx, &state.selection, nodes)
    };

    // The hover ring is a UI affordance, not molecular content — always on top, like labels.
    if let Some((pos, radius)) = hover_ring {
        cx.painter.circle_stroke(pos, radius + HOVER_RING_OFFSET_PX, Stroke::new(HOVER_RING_WIDTH_PX, cx.pal.chrome.hover_ring()));
    }
    paint_labels(&cx, nodes);
}

/// Plane-local unit vector pointing toward the orthographic viewer (inverse of `CameraState::rotate`).
fn camera_toward_vec(camera: &CameraState) -> Vec3 {
    let (sy, cy) = camera.yaw.sin_cos();
    let (sp, cp) = camera.pitch.sin_cos();
    Vec3::new(-sy * cp, sp, cy * cp)
}

#[allow(clippy::too_many_arguments)]
fn push_carto_mesh_draw<'a, Q>(
    mesh_draws: &mut Vec<GpuMeshDraw<'a>>,
    key: GpuMeshKey,
    mesh: &'a SurfaceMesh,
    vertex_colors: &'a [Color32],
    color_sig: u64,
    edge_color: Color32,
    base_alpha: f32,
    pres: &PresentationStyle<Q>,
    zoom: f32,
) {
    let face_alpha = base_alpha * pres.alphas.fill;
    let wf_alpha = base_alpha * pres.alphas.wireframe;
    let border_alpha = base_alpha * pres.alphas.border;
    let border_width_px = pres.border.effective_width_px(zoom);
    let has_border = border_width_px > 0.0 && pres.border.color.a() > 0 && border_alpha > MIN_DRAW_ALPHA;
    let show_faces = face_alpha > MIN_DRAW_ALPHA || has_border;
    let wf_width = if pres.wireframe.enabled && wf_alpha > MIN_DRAW_ALPHA {
        pres.wireframe.width_px.max(0.5)
    } else {
        0.0
    };
    if show_faces || wf_width > 0.0 {
        mesh_draws.push(GpuMeshDraw {
            key,
            mesh,
            vertex_colors: Some(vertex_colors),
            color_sig,
            offset: Vec3::ZERO,
            color: Color32::WHITE,
            edge_color,
            alpha: if face_alpha > MIN_DRAW_ALPHA { face_alpha } else { 0.0 },
            edge_alpha: wf_alpha,
            show_faces,
            wireframe_width_px: wf_width,
            culling: pres.culling,
            shading: pres.shading,
            border_width_px: if has_border { border_width_px } else { 0.0 },
            border_color: pres.border.color,
            border_alpha: if has_border { border_alpha } else { 0.0 },
        });
    }
}

/// Computes a fast cache signature for a dynamically morphed Sandwich GPU mesh.
fn hash_dynamic_mesh(mesh: &SurfaceMesh, colors: &[Color32], extra: u64) -> u64 {
    let mut h = DefaultHasher::new();
    extra.hash(&mut h);
    mesh.vertices.len().hash(&mut h);
    mesh.triangles.len().hash(&mut h);
    let step = (mesh.vertices.len() / 64).max(1);
    for i in (0..mesh.vertices.len()).step_by(step) {
        let v = mesh.vertices[i];
        v[0].to_bits().hash(&mut h);
        v[1].to_bits().hash(&mut h);
        v[2].to_bits().hash(&mut h);
        if let Some(&c) = colors.get(i) {
            c.to_array().hash(&mut h);
        }
    }
    h.finish()
}

#[allow(clippy::too_many_arguments)]
fn paint_gpu_scene(
    cx: &Ctx,
    selection: &Selection,
    rendering: &RenderingSettings,
    nodes: &[(Pos2, f32)],
    gpu: &mut GpuSurfaceRenderer,
) -> Option<Option<(Pos2, f32)>> {
    let cam_step = camera_toward_vec(cx.camera) * COPLANAR_LAYER_STEP_A;
    let to_world = cx.frame.to_local_matrix().inverse();
    let iface_idx = cx.ds.active_interface.unwrap_or(0);
    let inv_zoom = 1.0 / cx.camera.zoom.max(1e-3);

    let mut line_draws = Vec::new();
    let mut disc_draws = Vec::new();

    if cx.grid.alpha > 0.004 && cx.grid.grid_extent_a > 0.0 {
        grid::for_each_segment(cx.grid, |a, b| {
            line_draws.push(GpuLineDraw {
                a,
                b,
                prev: None,
                next: None,
                color: cx.grid_color,
                alpha: cx.grid.alpha,
                width_pt: 1.0,
                dash: None,
                border_pt: 0.0,
                border_color: Color32::TRANSPARENT,
                border_alpha: 1.0,
            });
        });
    }

    if let Some(([a, b], color, width_pt)) = hinge_segment(cx) {
        let off = cam_step;
        line_draws.push(GpuLineDraw {
            a: a + off,
            b: b + off,
            prev: None,
            next: None,
            color,
            alpha: 1.0,
            width_pt,
            dash: None,
            border_pt: 0.0,
            border_color: Color32::TRANSPARENT,
            border_alpha: 1.0,
        });
    }

    // ── Residue territories (per-side 3D SurfaceMesh + perimeter lines) ────────
    let mut terr_meshes: [Option<(SurfaceMesh, Vec<Color32>, f32, u64)>; 2] = [None, None];
    if cx.s.territories.visible {
        let mut verts: [Vec<[f32; 3]>; 2] = [Vec::new(), Vec::new()];
        let mut norms: [Vec<Vec3>; 2] = [Vec::new(), Vec::new()];
        let mut tris: [Vec<[u32; 3]>; 2] = [Vec::new(), Vec::new()];
        let mut cols: [Vec<Color32>; 2] = [Vec::new(), Vec::new()];
        let mut alphas: [f32; 2] = [0.0, 0.0];

        for_each_territory(cx, selection, |terr, border_pts, mesh_pts, fill, border, side_idx| {
            let slot = if terr.side == InterfaceSide::SideA { 0 } else { 1 };
            if let Some((color, alpha)) = fill {
                alphas[slot] = alpha;
                let off = cam_step * (2.0 + side_idx as f32);
                let base = verts[slot].len() as u32;
                for &p in mesh_pts {
                    verts[slot].push(to_world.transform_point3(p + off).to_array());
                    norms[slot].push(Vec3::ZERO);
                    cols[slot].push(color);
                }
                for tri in &terr.contour_triangles {
                    let [i0, i1, i2] = tri.map(|i| i as usize);
                    if i0 < mesh_pts.len() && i1 < mesh_pts.len() && i2 < mesh_pts.len() {
                        let (a, b, c) = (base + i0 as u32, base + i1 as u32, base + i2 as u32);
                        let va = Vec3::from(verts[slot][a as usize]);
                        let vb = Vec3::from(verts[slot][b as usize]);
                        let vc = Vec3::from(verts[slot][c as usize]);
                        let fn_vec = (vb - va).cross(vc - va);
                        norms[slot][a as usize] += fn_vec;
                        norms[slot][b as usize] += fn_vec;
                        norms[slot][c as usize] += fn_vec;
                        tris[slot].push([a, b, c]);
                    }
                }
            }
            if let Some((color, alpha, width_pt)) = border {
                let off = cam_step * (4.0 + side_idx as f32);
                let n = border_pts.len();
                for i in 0..n {
                    line_draws.push(GpuLineDraw {
                        a: border_pts[i] + off,
                        b: border_pts[(i + 1) % n] + off,
                        prev: (n >= 3).then(|| border_pts[(i + n - 1) % n] + off),
                        next: (n >= 3).then(|| border_pts[(i + 2) % n] + off),
                        color,
                        alpha,
                        width_pt,
                        dash: None,
                        border_pt: 0.0,
                        border_color: Color32::TRANSPARENT,
                        border_alpha: 1.0,
                    });
                }
            }
        });

        let fallback_n = Vec3::from(cx.iface.map.plane.normal).normalize_or_zero();
        for slot in 0..2 {
            if !tris[slot].is_empty() {
                let normal_arrs: Vec<[f32; 3]> = norms[slot]
                    .iter()
                    .map(|&n| {
                        let nn = n.normalize_or_zero();
                        (if nn.length_squared() > 0.5 { nn } else { fallback_n }).to_array()
                    })
                    .collect();
                let mesh = SurfaceMesh {
                    vertices: std::mem::take(&mut verts[slot]),
                    normals: normal_arrs,
                    triangles: std::mem::take(&mut tris[slot]),
                    nearest_atom: Vec::new(),
                };
                let c = std::mem::take(&mut cols[slot]);
                let sig = hash_dynamic_mesh(&mesh, &c, (slot as u64) << 32 | cx.s.side_a_on_top as u64);
                terr_meshes[slot] = Some((mesh, c, alphas[slot], sig));
            }
        }
    }

    if cx.s.show_backbone {
        let off = cam_step * 6.0;
        for_each_backbone_chain(cx, |pts, color, alpha, width_pt| {
            for (i, w) in pts.windows(2).enumerate() {
                line_draws.push(GpuLineDraw {
                    a: w[0] + off,
                    b: w[1] + off,
                    prev: i.checked_sub(1).map(|k| pts[k] + off),
                    next: pts.get(i + 2).map(|&p| p + off),
                    color,
                    alpha,
                    width_pt,
                    dash: None,
                    border_pt: 0.0,
                    border_color: Color32::TRANSPARENT,
                    border_alpha: 1.0,
                });
            }
        });
    }

    let local_nodes: Vec<Vec3> = cx
        .iface
        .map
        .nodes
        .iter()
        .map(|n| {
            let uv = if cx.s.use_relaxed_uv { n.pos_uv } else { n.pos_uv_raw };
            cx.morph_local(n.pos_3d, uv, n.side)
        })
        .collect();

    // ── Contact edges (2D Capsule lines or 3D Prism8 cylinders) ────────────────
    let edge_pres = &cx.s.edges.presentation;
    let mut edge_meshes: [Option<(SurfaceMesh, Vec<Color32>, f32, u64)>; 3] = [None, None, None];
    if edge_pres.quality == RibbonStyle::Prism8 {
        let mut verts: [Vec<[f32; 3]>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut norms: [Vec<[f32; 3]>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut tris: [Vec<[u32; 3]>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut cols: [Vec<Color32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut alphas: [f32; 3] = [0.0, 0.0, 0.0];

        for_each_edge(cx, |e, color, alpha, width_pt| {
            let bucket = if e.is_cross_interface {
                2
            } else if cx.ds.node(e.a).map_or(InterfaceSide::SideA, |n| n.side) == InterfaceSide::SideA {
                0
            } else {
                1
            };
            alphas[bucket] = alpha;
            let off = cam_step * if e.is_cross_interface { 8.0 } else { 7.0 };
            let pa = to_world.transform_point3(local_nodes[e.a.0 as usize] + off);
            let pb = to_world.transform_point3(local_nodes[e.b.0 as usize] + off);
            let radius_a = (0.5 * width_pt * inv_zoom).max(0.02);
            push_cylinder_segment(
                pa,
                pb,
                radius_a,
                color,
                color,
                8,
                &mut verts[bucket],
                &mut norms[bucket],
                &mut tris[bucket],
                &mut cols[bucket],
            );
        });
        for bucket in 0..3 {
            if !tris[bucket].is_empty() {
                let mesh = SurfaceMesh {
                    vertices: std::mem::take(&mut verts[bucket]),
                    normals: std::mem::take(&mut norms[bucket]),
                    triangles: std::mem::take(&mut tris[bucket]),
                    nearest_atom: Vec::new(),
                };
                let c = std::mem::take(&mut cols[bucket]);
                let sig = hash_dynamic_mesh(&mesh, &c, bucket as u64);
                edge_meshes[bucket] = Some((mesh, c, alphas[bucket], sig));
            }
        }
    } else {
        let edge_alpha_mul = edge_pres.alphas.fill;
        if edge_alpha_mul > MIN_DRAW_ALPHA {
            for_each_edge(cx, |e, color, alpha, width_pt| {
                let off = cam_step * if e.is_cross_interface { 8.0 } else { 7.0 };
                line_draws.push(GpuLineDraw {
                    a: local_nodes[e.a.0 as usize] + off,
                    b: local_nodes[e.b.0 as usize] + off,
                    prev: None,
                    next: None,
                    color,
                    alpha: alpha * edge_alpha_mul,
                    width_pt,
                    dash: None,
                    border_pt: 0.0,
                    border_color: Color32::TRANSPARENT,
                    border_alpha: 1.0,
                });
            });
        }
    }

    // ── Atom nodes (2D Disc, 3D Billboard, or 3D Geodesic Sphere mesh) ─────────
    let atom_pres = &cx.s.atoms.presentation;
    let top_side = if cx.s.side_a_on_top { InterfaceSide::SideA } else { InterfaceSide::SideB };
    let mut hover_ring = None;
    let mut atom_meshes: [Option<(SurfaceMesh, Vec<Color32>, f32, u64)>; 2] = [None, None];
    if let Some(preset) = atom_pres.quality.geodesic_preset() {
        let mut by_side: [Vec<([f32; 3], f32, Color32)>; 2] = [Vec::new(), Vec::new()];
        let mut alphas: [f32; 2] = [0.0, 0.0];
        for_each_atom(cx, selection, |idx, radius_pt, fill, _outline, hovered| {
            let side = cx.iface.map.nodes[idx].side;
            let slot = if side == InterfaceSide::SideA { 0 } else { 1 };
            let layer_step = if side == top_side { 10.0 } else { 9.0 };
            if let Some((fill_color, fill_alpha)) = fill {
                alphas[slot] = fill_alpha;
                let world_pos = to_world.transform_point3(local_nodes[idx] + cam_step * layer_step);
                let radius_a = (radius_pt * inv_zoom).max(0.05);
                by_side[slot].push((world_pos.to_array(), radius_a, fill_color));
            }
            if hovered {
                hover_ring = Some((nodes[idx].0, radius_pt));
            }
        });
        for slot in 0..2 {
            if !by_side[slot].is_empty() {
                let (mesh, colors) = build_geodesic_spheres_mesh(&by_side[slot], preset);
                let sig = hash_dynamic_mesh(&mesh, &colors, (slot as u64) << 16 | preset as u64);
                atom_meshes[slot] = Some((mesh, colors, alphas[slot], sig));
            }
        }
    } else {
        let shaded_3d = atom_pres.quality == SphereQuality::Billboard3D;
        let alpha_mul = atom_pres.alphas.fill;
        for_each_atom(cx, selection, |idx, radius_pt, fill, outline, hovered| {
            if (fill.is_some() || outline.is_some()) && alpha_mul > MIN_DRAW_ALPHA {
                let side = cx.iface.map.nodes[idx].side;
                let layer_step = if side == top_side { 10.0 } else { 9.0 };
                let (fill_color, fill_alpha) = fill.unwrap_or((Color32::TRANSPARENT, 0.0));
                let (outline_color, outline_alpha, outline_pt) = outline.unwrap_or((Color32::TRANSPARENT, 0.0, 0.0));
                disc_draws.push(GpuDiscDraw {
                    center: local_nodes[idx] + cam_step * layer_step,
                    radius_pt,
                    outline_pt,
                    fill_color,
                    fill_alpha: fill_alpha * alpha_mul,
                    outline_color,
                    outline_alpha: outline_alpha * alpha_mul,
                    shaded_3d,
                });
            }
            if hovered {
                hover_ring = Some((nodes[idx].0, radius_pt));
            }
        });
    }

    // ── Morphed interface sheet ────────────────────────────────────────────────
    let sheet_alpha = cx.s.sheet.alpha.value;
    let morphed_sheet = if cx.s.sheet.visible
        && sheet_alpha > MIN_DRAW_ALPHA
        && !cx.iface.draw_geometry.sheet_mesh.vertices.is_empty()
    {
        let m = morph_sheet_with_strain(
            &cx.iface.draw_geometry.sheet_mesh,
            &cx.iface.sheet_flat_uv,
            &cx.iface.map.plane,
            cx.s.flatten.value,
            cx.iface.half_buried_sasa_a2,
        );
        let vcols: Vec<Color32> = m
            .vertex_strain
            .iter()
            .map(|&eps| cx.s.sheet_color.sample(cx.pal, eps, cx.s.sheet_fixed_color))
            .collect();
        let mut h = DefaultHasher::new();
        cx.s.flatten.value.to_bits().hash(&mut h);
        cx.s.sheet_color.hash(&mut h);
        cx.s.sheet_fixed_color.to_array().hash(&mut h);
        cx.pal.strain.sample(0.0).to_array().hash(&mut h);
        cx.pal.strain.sample(0.5).to_array().hash(&mut h);
        cx.pal.strain.sample(1.0).to_array().hash(&mut h);
        m.mesh.vertices.len().hash(&mut h);
        Some((m, vcols, h.finish()))
    } else {
        None
    };

    let mut mesh_draws = Vec::new();
    if let Some((ref morphed, ref vcols, color_sig)) = morphed_sheet {
        push_carto_mesh_draw(
            &mut mesh_draws,
            GpuMeshKey::CartographySheet(iface_idx),
            &morphed.mesh,
            vcols,
            color_sig,
            cx.pal.chrome.ink,
            sheet_alpha,
            &cx.s.sheet.presentation,
            cx.camera.zoom,
        );
    }
    for (slot, entry) in terr_meshes.iter().enumerate() {
        if let Some((mesh, vcols, alpha, sig)) = entry {
            push_carto_mesh_draw(
                &mut mesh_draws,
                GpuMeshKey::CartographyTerritories(iface_idx, slot as u8),
                mesh,
                vcols,
                *sig,
                cx.pal.chrome.ink,
                *alpha,
                &cx.s.territories.presentation,
                cx.camera.zoom,
            );
        }
    }
    for (bucket, entry) in edge_meshes.iter().enumerate() {
        if let Some((mesh, vcols, alpha, sig)) = entry {
            push_carto_mesh_draw(
                &mut mesh_draws,
                GpuMeshKey::CartographyEdges(iface_idx, bucket as u8),
                mesh,
                vcols,
                *sig,
                cx.pal.chrome.ink,
                *alpha,
                edge_pres,
                cx.camera.zoom,
            );
        }
    }
    for (slot, entry) in atom_meshes.iter().enumerate() {
        if let Some((mesh, vcols, alpha, sig)) = entry {
            push_carto_mesh_draw(
                &mut mesh_draws,
                GpuMeshKey::CartographyAtoms(iface_idx, slot as u8),
                mesh,
                vcols,
                *sig,
                cx.pal.chrome.ink,
                *alpha,
                atom_pres,
                cx.camera.zoom,
            );
        }
    }

    let (instance_id, draw_gen) = cx.ds.draw_cache_key();
    let texture_id = gpu.render(
        cx.rect,
        cx.painter.pixels_per_point(),
        cx.pal.chrome.viewport_bg,
        cx.camera,
        cx.frame.to_local_matrix(),
        instance_id,
        draw_gen,
        rendering.transparency,
        rendering.depth_peel_layers,
        &mesh_draws,
        &[],
        &line_draws,
        &disc_draws,
    )?;
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    cx.painter.image(texture_id, cx.rect, uv, Color32::WHITE);
    Some(hover_ring)
}

fn paint_cpu_scene(cx: &Ctx, selection: &Selection, nodes: &[(Pos2, f32)]) -> Option<(Pos2, f32)> {
    grid::paint(cx.painter, cx.rect, cx.camera, cx.grid, cx.grid_color);
    if let Some(([a, b], color, width)) = hinge_segment(cx) {
        let (pa, _) = cx.camera.project(a, cx.rect);
        let (pb, _) = cx.camera.project(b, cx.rect);
        cx.painter.line_segment([pa, pb], Stroke::new(width, color));
    }

    let mut list = DrawList::default();
    let sheet_alpha = cx.s.sheet.alpha.value;
    let pres = &cx.s.sheet.presentation;
    if cx.s.sheet.visible
        && sheet_alpha > MIN_DRAW_ALPHA
        && !cx.iface.draw_geometry.sheet_mesh.vertices.is_empty()
    {
        let morphed = morph_sheet_with_strain(
            &cx.iface.draw_geometry.sheet_mesh,
            &cx.iface.sheet_flat_uv,
            &cx.iface.map.plane,
            cx.s.flatten.value,
            cx.iface.half_buried_sasa_a2,
        );
        let to_local = cx.frame.to_local_matrix();
        let projected: Vec<(Pos2, f32)> = morphed
            .mesh
            .vertices
            .iter()
            .map(|&v| cx.camera.project(to_local.transform_point3(Vec3::from(v)), cx.rect))
            .collect();
        let face_alpha = sheet_alpha * pres.alphas.fill;
        let wf_alpha = sheet_alpha * pres.alphas.wireframe;
        let stroke = if pres.wireframe.enabled && wf_alpha > MIN_DRAW_ALPHA {
            Stroke::new(pres.wireframe.width_px.max(0.5), palette::with_alpha(cx.pal.chrome.ink, wf_alpha))
        } else {
            Stroke::NONE
        };
        for &[i0, i1, i2] in &morphed.mesh.triangles {
            let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
            let avg_strain =
                (morphed.vertex_strain[u0] + morphed.vertex_strain[u1] + morphed.vertex_strain[u2]) / 3.0;
            let fill = palette::with_alpha(
                cx.s.sheet_color.sample(cx.pal, avg_strain, cx.s.sheet_fixed_color),
                face_alpha,
            );
            let depth = (projected[u0].1 + projected[u1].1 + projected[u2].1) / 3.0;
            list.triangle([projected[u0].0, projected[u1].0, projected[u2].0], depth, fill, stroke);
        }
    }
    if cx.s.show_backbone {
        for_each_backbone_chain(cx, |pts, color, alpha, width| {
            let stroke = Stroke::new(width, palette::with_alpha(color, alpha));
            let projected: Vec<(Pos2, f32)> = pts.iter().map(|&p| cx.camera.project(p, cx.rect)).collect();
            for w in projected.windows(2) {
                list.segment(w[0].0, w[1].0, 0.5 * (w[0].1 + w[1].1), stroke);
            }
        });
    }
    if cx.s.territories.visible {
        let terr_fill_alpha = cx.s.territories.presentation.alphas.fill;
        for_each_territory(cx, selection, |terr, border_pts, mesh_pts, fill, border, _| {
            if let Some((color, alpha)) = fill {
                let eff_alpha = alpha * terr_fill_alpha;
                if eff_alpha > MIN_DRAW_ALPHA {
                    let projected_mesh: Vec<(Pos2, f32)> =
                        mesh_pts.iter().map(|&p| cx.camera.project(p, cx.rect)).collect();
                    let c = palette::with_alpha(color, eff_alpha);
                    for tri in &terr.contour_triangles {
                        let [i0, i1, i2] = tri.map(|i| i as usize);
                        if i0 < projected_mesh.len() && i1 < projected_mesh.len() && i2 < projected_mesh.len() {
                            let depth = (projected_mesh[i0].1 + projected_mesh[i1].1 + projected_mesh[i2].1) / 3.0;
                            list.triangle(
                                [projected_mesh[i0].0, projected_mesh[i1].0, projected_mesh[i2].0],
                                depth,
                                c,
                                Stroke::NONE,
                            );
                        }
                    }
                }
            }
            if let Some((color, alpha, width)) = border {
                let projected: Vec<(Pos2, f32)> = border_pts.iter().map(|&p| cx.camera.project(p, cx.rect)).collect();
                let stroke = Stroke::new(width, palette::with_alpha(color, alpha));
                let n = projected.len();
                for i in 0..n {
                    let (a, b) = (projected[i], projected[(i + 1) % n]);
                    list.segment(a.0, b.0, 0.5 * (a.1 + b.1), stroke);
                }
            }
        });
    }
    let edge_alpha_mul = cx.s.edges.presentation.alphas.fill;
    if edge_alpha_mul > MIN_DRAW_ALPHA {
        for_each_edge(cx, |e, color, alpha, width| {
            let stroke = Stroke::new(width, palette::with_alpha(color, alpha * edge_alpha_mul));
            let (a, b) = (nodes[e.a.0 as usize], nodes[e.b.0 as usize]);
            list.segment(a.0, b.0, 0.5 * (a.1 + b.1), stroke);
        });
    }
    let atom_alpha_mul = cx.s.atoms.presentation.alphas.fill;
    let mut hover_ring = None;
    for_each_atom(cx, selection, |idx, radius, fill, outline, hovered| {
        let (pos, depth) = nodes[idx];
        let fill_color = fill.map_or(Color32::TRANSPARENT, |(c, a)| palette::with_alpha(c, a * atom_alpha_mul));
        let stroke = outline.map_or(Stroke::NONE, |(c, a, w)| Stroke::new(w, palette::with_alpha(c, a * atom_alpha_mul)));
        if atom_alpha_mul > MIN_DRAW_ALPHA && (fill_color != Color32::TRANSPARENT || stroke != Stroke::NONE) {
            list.disc(pos, radius, depth, fill_color, stroke);
        }
        if hovered {
            hover_ring = Some((pos, radius));
        }
    });
    list.paint(cx.painter);
    hover_ring
}

/// Hinge line for the open-book butterfly, shown once the map is mostly flat and opened.
fn hinge_segment(cx: &Ctx) -> Option<([Vec3; 2], Color32, f32)> {
    if cx.s.book.value <= 0.01 || cx.s.flatten.value <= 0.1 {
        return None;
    }
    let h = cx.frame.hinge_u;
    Some((
        [Vec3::new(h, -HINGE_HALF_LEN_A, 0.0), Vec3::new(h, HINGE_HALF_LEN_A, 0.0)],
        cx.pal.chrome.hinge(),
        HINGE_WIDTH_PX,
    ))
}

/// Faint Cα trace of both groups, fading out as the map flattens.
fn for_each_backbone_chain(cx: &Ctx, mut emit: impl FnMut(&[Vec3], Color32, f32, f32)) {
    let t = cx.s.flatten.value;
    if t >= 0.98 {
        return;
    }
    let fade = 1.0 - t * (1.0 - BACKBONE_FADE_MIN);
    let width = (cx.camera.zoom * BACKBONE_WIDTH_ZOOM_SCALE).max(BACKBONE_MIN_WIDTH_PX);

    for ch in &cx.ds.chains {
        let side = cx.ds.chain_side(&ch.chain_id);
        let alpha = cx.s.side_alpha(side);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        // Use each Cα's own tangent-plane (u, v) so the trace follows the butterfly hinge.
        let pts: Vec<Vec3> = ch
            .ca_positions
            .iter()
            .map(|&p| {
                let local = cx.frame.to_local(p);
                cx.morph_local(p, [local.x, local.y], side)
            })
            .collect();
        emit(&pts, cx.pal.sides.side_dark(side), fade * alpha * BACKBONE_ALPHA_SCALE, width);
    }
}

/// Residue territory polygons styled by fill/stroke lenses; iterates per `side_paint_order`.
fn for_each_territory(
    cx: &Ctx,
    selection: &Selection,
    mut emit: impl FnMut(&ResidueTerritory, &[Vec3], &[Vec3], Option<(Color32, f32)>, Option<(Color32, f32, f32)>, usize),
) {
    let color_lens = cx.s.territories.fill.lens;
    let stroke_lens = cx.s.territories.stroke_width.lens;
    let fixed_stroke_w = cx.s.territories.stroke_width.fixed;
    let blend = cx.s.contour_sheet_blend.clamp(0.0, 0.99);

    for (side_idx, side) in cx.s.side_paint_order().into_iter().enumerate() {
        let alpha = cx.s.side_alpha(side);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        for terr in cx.iface.map.territories.iter().filter(|t| t.side == side && !t.contour_uv.is_empty()) {
            // Contours exist only for relaxed (u, v), so they ignore the raw-UV toggle.
            let border_pts: Vec<Vec3> = terr
                .contour_uv
                .iter()
                .enumerate()
                .map(|(i, &uv)| {
                    let p_atom = Vec3::from(terr.contour_3d[i]);
                    let p_sheet = terr.contour_3d_sheet.get(i).map_or(p_atom, |&p| Vec3::from(p));
                    cx.morph_local(p_atom.lerp(p_sheet, blend).to_array(), uv, side)
                })
                .collect();

            let mesh_pts: Vec<Vec3> = if !terr.contour_mesh_uv.is_empty()
                && terr.contour_mesh_3d_atoms.len() == terr.contour_mesh_uv.len()
            {
                terr.contour_mesh_uv
                    .iter()
                    .enumerate()
                    .map(|(i, &uv)| {
                        let p_atom = Vec3::from(terr.contour_mesh_3d_atoms[i]);
                        let p_sheet = terr.contour_mesh_3d_sheet.get(i).map_or(p_atom, |&p| Vec3::from(p));
                        cx.morph_local(p_atom.lerp(p_sheet, blend).to_array(), uv, side)
                    })
                    .collect()
            } else {
                border_pts.clone()
            };

            let selected = selection.selected_residue == Some(terr.global_residue);
            let base_color = lenses::territory_fill_color(
                terr,
                &cx.iface.map.nodes,
                &cx.iface.atom_meta,
                color_lens,
                cx.s.territories.fill.fixed,
                cx.s.bfactor_mode,
                cx.lenses,
                cx.pal,
            );
            // Selection is a derived highlight tint of the base color (`palette::highlight`), not
            // a second hardcoded alpha tier.
            let base_color = if selected { palette::highlight(base_color) } else { base_color };
            let width = sizing::territory_stroke_width_px(
                terr,
                &cx.iface.map.nodes,
                stroke_lens,
                fixed_stroke_w,
                cx.s.bfactor_mode,
                selected,
                cx.lenses,
            );
            let fill = (color_lens != TerritoryColorLens::Off).then_some((base_color, alpha));
            let border_base = if color_lens == TerritoryColorLens::Off {
                let side_color = cx.pal.sides.side(terr.side);
                if selected { palette::highlight(side_color) } else { side_color }
            } else {
                base_color
            };
            let border = (width > 0.0).then(|| (palette::edge_color(border_base), alpha * TERRITORY_BORDER_ALPHA_SCALE, width));
            emit(terr, &border_pts, &mesh_pts, fill, border, side_idx);
        }
    }
}

/// Contact edges — first-shell springs within a side, and cross-interface contacts.
fn for_each_edge(cx: &Ctx, mut emit: impl FnMut(&InterfaceEdge, Color32, f32, f32)) {
    if !cx.s.edges.visible {
        return;
    }
    let color_lens = cx.s.edges.color.lens;
    let width_lens = cx.s.edges.width.lens;
    if color_lens == EdgeColorLens::Off || width_lens == EdgeWidthLens::Off {
        return;
    }
    if !cx.s.edges.show_intra && !cx.s.edges.show_cross {
        return;
    }
    let cross_alpha = cx.s.side_a_alpha.value.min(cx.s.side_b_alpha.value);

    for e in &cx.iface.map.edges {
        if e.is_cross_interface {
            if !cx.s.edges.show_cross {
                continue;
            }
        } else if !cx.s.edges.show_intra {
            continue;
        }
        let kind_allowed = match e.kind {
            smorgasprot_core::EdgeKind::Covalent(_) => cx.s.edges.show_covalent,
            smorgasprot_core::EdgeKind::HBond => cx.s.edges.show_hbond,
            smorgasprot_core::EdgeKind::SaltBridge => cx.s.edges.show_salt_bridge,
            smorgasprot_core::EdgeKind::Clash => cx.s.edges.show_clash,
            smorgasprot_core::EdgeKind::VanDerWaalsContact => cx.s.edges.show_vdw,
        };
        if !kind_allowed {
            continue;
        }
        let alpha = if e.is_cross_interface {
            cross_alpha
        } else {
            let side = cx.ds.node(e.a).map_or(InterfaceSide::None, |n| n.side);
            cx.s.side_alpha(side)
        };
        if alpha <= MIN_INTERACTIVE_ALPHA {
            continue;
        }
        let color = lenses::edge_stroke_color(
            e,
            &cx.iface.map.nodes,
            color_lens,
            cx.s.edges.color.fixed,
            cx.s.bfactor_mode,
            cx.lenses,
            cx.pal,
        );
        let width = sizing::edge_width_px(
            e,
            &cx.iface.map.nodes,
            width_lens,
            cx.s.edges.width.fixed,
            cx.s.bfactor_mode,
            cx.lenses,
        );
        if width <= 0.0 {
            continue;
        }
        let (width_scale, alpha_scale) =
            if e.is_cross_interface { (1.0, CROSS_ARC_ALPHA_SCALE) } else { (INTRA_EDGE_WIDTH_SCALE, INTRA_EDGE_ALPHA_SCALE) };
        emit(e, color, alpha * alpha_scale, width * width_scale);
    }
}

/// Atom discs colored and sized by the active node fill, radius, stroke color, and stroke width lenses.
fn for_each_atom(
    cx: &Ctx,
    selection: &Selection,
    mut emit: impl FnMut(usize, f32, Option<(Color32, f32)>, Option<(Color32, f32, f32)>, bool),
) {
    if !cx.s.atoms.visible {
        return;
    }
    let fill_lens = cx.s.atoms.fill.lens;
    let size_mode = cx.s.atoms.radius.lens;
    let stroke_color_lens = cx.s.atoms.stroke_color.lens;
    let stroke_width_lens = cx.s.atoms.stroke_width.lens;
    let fixed_stroke_w = cx.s.atoms.stroke_width.fixed;

    for (idx, (node, meta)) in cx.iface.map.nodes.iter().zip(&cx.iface.atom_meta).enumerate() {
        let alpha = cx.s.side_alpha(node.side);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        let radius = sizing::atom_radius_px(
            node,
            size_mode,
            cx.s.atoms.radius.fixed,
            cx.s.bfactor_mode,
            cx.camera.zoom,
            cx.atom_draw,
            cx.lenses,
        );
        if size_mode == AtomSizeMode::Off || radius <= 0.0 {
            continue;
        }
        let hovered = selection.hovered_atom == Some(node.global_atom);
        let fill = (fill_lens != ColorLens::Off).then(|| {
            (
                lenses::atom_fill_color(
                    node,
                    meta,
                    fill_lens,
                    cx.s.atoms.fill.fixed,
                    cx.s.bfactor_mode,
                    cx.s.side_a_on_top,
                    cx.lenses,
                    cx.pal,
                ),
                alpha,
            )
        });
        let stroke_width = sizing::atom_stroke_width_px(
            node,
            stroke_width_lens,
            fixed_stroke_w,
            cx.s.bfactor_mode,
            hovered,
            cx.lenses,
        );
        let outline = (stroke_color_lens != NodeStrokeColorLens::Off && stroke_width > 0.0).then(|| {
            (
                lenses::atom_stroke_color(
                    node,
                    meta,
                    stroke_color_lens,
                    cx.s.atoms.stroke_color.fixed,
                    cx.s.bfactor_mode,
                    cx.s.side_a_on_top,
                    hovered,
                    cx.lenses,
                    cx.pal,
                ),
                alpha,
                stroke_width,
            )
        });
        emit(idx, radius, fill, outline, hovered);
    }
}

/// Topmost z-layer: territory and atom text labels painted above all polygons, edges, and atom discs.
fn paint_labels(cx: &Ctx, nodes: &[(Pos2, f32)]) {
    if cx.s.territories.labels.enabled {
        let terr_font = FontId::proportional((cx.camera.zoom * TERRITORY_FONT_ZOOM_SCALE).clamp(TERRITORY_FONT_MIN_PX, TERRITORY_FONT_MAX_PX));
        let blend = cx.s.contour_sheet_blend.clamp(0.0, 0.99);
        for side in cx.s.side_paint_order() {
            let alpha = cx.s.side_alpha(side);
            if alpha <= MIN_DRAW_ALPHA {
                continue;
            }
            for terr in cx.iface.map.territories.iter().filter(|t| t.side == side && !t.contour_uv.is_empty()) {
                if let Some(label_text) = lenses::format_territory_label(
                    terr,
                    &cx.iface.map.nodes,
                    &cx.iface.atom_meta,
                    &cx.s.territories.labels,
                ) {
                    let p_atom = Vec3::from(terr.label_center_3d);
                    let p_sheet = Vec3::from(terr.label_center_3d_sheet);
                    let label_3d = p_atom.lerp(p_sheet, blend).to_array();
                    let center_pos = cx.morph(label_3d, terr.label_center_uv, side).0;
                    let (label_pos, align) = match cx.s.territories.labels.position {
                        LabelPosition::Center => (center_pos, Align2::CENTER_CENTER),
                        LabelPosition::Top => {
                            let min_y = terr
                                .contour_uv
                                .iter()
                                .enumerate()
                                .map(|(i, &uv)| {
                                    let pa = Vec3::from(terr.contour_3d[i]);
                                    let ps = terr.contour_3d_sheet.get(i).map_or(pa, |&p| Vec3::from(p));
                                    cx.morph(pa.lerp(ps, blend).to_array(), uv, side).0.y
                                })
                                .fold(center_pos.y, f32::min);
                            (Pos2::new(center_pos.x, min_y - ATOM_LABEL_OFFSET_PX), Align2::CENTER_BOTTOM)
                        }
                    };
                    let ink = palette::with_alpha(cx.pal.chrome.ink, alpha * TERRITORY_LABEL_ALPHA_SCALE);
                    cx.painter.text(label_pos, align, label_text, terr_font.clone(), ink);
                }
            }
        }
    }

    if cx.s.atoms.labels.enabled {
        let size_mode = cx.s.atoms.radius.lens;
        let atom_font = FontId::proportional((cx.camera.zoom * ATOM_FONT_ZOOM_SCALE).clamp(ATOM_FONT_MIN_PX, ATOM_FONT_MAX_PX));
        for ((node, meta), &(pos, _)) in cx.iface.map.nodes.iter().zip(&cx.iface.atom_meta).zip(nodes) {
            let alpha = cx.s.side_alpha(node.side);
            if alpha <= MIN_DRAW_ALPHA {
                continue;
            }
            if let Some(label_text) = lenses::format_atom_label(node, meta, &cx.s.atoms.labels) {
                let (label_pos, align) = match cx.s.atoms.labels.position {
                    LabelPosition::Center => (pos, Align2::CENTER_CENTER),
                    LabelPosition::Top => {
                        let radius = sizing::atom_radius_px(
                            node,
                            size_mode,
                            cx.s.atoms.radius.fixed,
                            cx.s.bfactor_mode,
                            cx.camera.zoom,
                            cx.atom_draw,
                            cx.lenses,
                        );
                        (pos + vec2(0.0, -(radius + ATOM_LABEL_OFFSET_PX)), Align2::CENTER_BOTTOM)
                    }
                };
                let ink = palette::with_alpha(cx.pal.chrome.ink, alpha * ATOM_LABEL_ALPHA_SCALE);
                cx.painter.text(label_pos, align, label_text, atom_font.clone(), ink);
            }
        }
    }
}
