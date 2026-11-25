//! Complex & Garnish 3D scene renderer: molecular surfaces, Cα cartoons, atom spheres, and derived interface layers.
//! Renders through [`GpuSurfaceRenderer`] (`wgpu`) when available, with a CPU [`DrawList`] fallback for Glow.

use super::draw_list::DrawList;
use super::grid;
use super::plane_space::{PlaneFrame, explode_offset};
use crate::camera::CameraState;
use crate::dataset::ViewerDataset;
use crate::render::{GpuDiscDraw, GpuLineDraw, GpuMeshDraw, GpuMeshKey, GpuSurfaceRenderer};
use crate::state::{
    AtomDrawParams, AtomSizeMode, BFactorMode, BoundaryLayerStyle, CartoonRepStyle, CartoonStyle, ChainColorLens,
    ColorLens, ExplodeParams, GridParams, InterfaceDisplayMode, LensParams, MeshShading, NodeStrokeColorLens,
    OpacityParams, PresentationStyle, RenderingSettings, SphereQuality, SphereRepStyle, SurfaceCulling,
    SurfaceRepStyle, UiState,
};
use crate::style::{Palette, lenses, palette, sizing};
use egui::{Color32, Painter, Pos2, Rect, Stroke};
use glam::Vec3;
use smorgasprot_core::surface::{GeodesicPreset, unit_geodesic_sphere};
use smorgasprot_core::{InterfaceSide, SurfaceMesh};

/// Fixed world-space light direction for Lambert shading (not normalized; matches the HTML viewer).
const LIGHT_DIR: Vec3 = Vec3::new(0.3, 0.5, 0.8);

struct LayerAlpha {
    visible: bool,
    master: f32,
}

struct SceneInputs<'a> {
    camera: &'a CameraState,
    explode_val: f32,
    complex_explode_val: f32,
    bfactor_mode: BFactorMode,
    surfaces_gate: LayerAlpha,
    cartoon_gate: LayerAlpha,
    complex_spheres_gate: LayerAlpha,
    surface: &'a SurfaceRepStyle,
    cartoon: &'a CartoonRepStyle,
    complex_spheres: &'a SphereRepStyle,
    boundary: Option<&'a BoundaryLayerStyle>,
    interface_spheres: Option<(LayerAlpha, &'a SphereRepStyle)>,
}

/// Paints the Slice ("1. Slice") workspace tab using Slice visibility/explode/interface + Garnish styles.
pub fn paint(
    painter: &Painter,
    rect: Rect,
    ds: &ViewerDataset,
    state: &UiState,
    rendering: &RenderingSettings,
    gpu_surfaces: Option<&mut GpuSurfaceRenderer>,
) {
    let cx = &state.slice;
    let g = &state.garnish;
    let inputs = SceneInputs {
        camera: &cx.camera,
        explode_val: cx.explode.value,
        complex_explode_val: cx.complex_explode.value,
        bfactor_mode: g.bfactor_mode,
        surfaces_gate: LayerAlpha { visible: cx.show_surface, master: cx.surface_alpha.value },
        cartoon_gate: LayerAlpha { visible: cx.show_cartoon, master: cx.cartoon_alpha.value },
        complex_spheres_gate: LayerAlpha { visible: cx.show_spheres, master: cx.spheres_alpha.value },
        surface: &g.surface,
        cartoon: &g.cartoon,
        complex_spheres: &g.spheres,
        boundary: Some(&cx.boundary),
        interface_spheres: Some((
            LayerAlpha { visible: cx.interface_spheres_visible, master: cx.interface_spheres_alpha.value },
            &cx.interface_spheres,
        )),
    };
    paint_scene(painter, rect, ds, state, &inputs, rendering, gpu_surfaces);
}

/// Paints the Garnish ("3. Garnish") presentation studio using Garnish visibility, camera, and calibrated styles.
pub fn paint_garnish(
    painter: &Painter,
    rect: Rect,
    ds: &ViewerDataset,
    state: &UiState,
    rendering: &RenderingSettings,
    gpu_surfaces: Option<&mut GpuSurfaceRenderer>,
) {
    let g = &state.garnish;
    let inputs = SceneInputs {
        camera: &g.camera,
        explode_val: 0.0,
        complex_explode_val: 0.0,
        bfactor_mode: g.bfactor_mode,
        surfaces_gate: LayerAlpha { visible: g.surface_visible, master: g.surface_alpha.value },
        cartoon_gate: LayerAlpha { visible: g.cartoon_visible, master: g.cartoon_alpha.value },
        complex_spheres_gate: LayerAlpha { visible: g.spheres_visible, master: g.spheres_alpha.value },
        surface: &g.surface,
        cartoon: &g.cartoon,
        complex_spheres: &g.spheres,
        boundary: None,
        interface_spheres: None,
    };
    paint_scene(painter, rect, ds, state, &inputs, rendering, gpu_surfaces);
}

fn presentation_active<Q>(pres: &PresentationStyle<Q>, master: f32, min_alpha: f32) -> bool {
    let has_fill = master * pres.alphas.fill > min_alpha;
    let has_wf = pres.wireframe.enabled && master * pres.alphas.wireframe > min_alpha;
    let has_border = pres.border.width_px > 0.0 && pres.border.color.a() > 0 && master * pres.alphas.border > min_alpha;
    has_fill || has_wf || has_border
}

fn paint_scene(
    painter: &Painter,
    rect: Rect,
    ds: &ViewerDataset,
    state: &UiState,
    inputs: &SceneInputs<'_>,
    rendering: &RenderingSettings,
    gpu_surfaces: Option<&mut GpuSurfaceRenderer>,
) {
    let iface = ds.active_interface();
    let frame = iface.map_or_else(|| PlaneFrame::world_centered(ds.default_origin), |i| PlaneFrame::from(&i.map.plane));
    let opacity = &state.view_params.opacity;
    let view = View {
        frame,
        camera: inputs.camera,
        explode_val: inputs.explode_val,
        complex_explode_val: inputs.complex_explode_val,
        explode: &state.view_params.explode,
        grid: &state.view_params.grid,
        grid_color: state.view_params.grid_color,
        opacity,
        atom_draw: &state.view_params.atom_draw,
        lenses: &state.view_params.lenses,
        bfactor_mode: inputs.bfactor_mode,
        pal: &state.palette,
        rect,
    };
    let min_alpha = opacity.min_draw_alpha;

    let draw_surfaces = inputs.surfaces_gate.visible
        && inputs.surface.color.lens != ChainColorLens::Off
        && presentation_active(&inputs.surface.presentation, inputs.surfaces_gate.master, min_alpha);
    let draw_boundary = iface.is_some()
        && inputs.boundary.is_some_and(|b| {
            b.mode != InterfaceDisplayMode::Off && presentation_active(&b.presentation, b.alpha.value, min_alpha)
        });
    let draw_cartoon = inputs.cartoon_gate.visible
        && inputs.cartoon.color.lens != ChainColorLens::Off
        && presentation_active(&inputs.cartoon.presentation, inputs.cartoon_gate.master, min_alpha);
    let draw_complex_spheres = inputs.complex_spheres_gate.visible
        && inputs.complex_spheres_gate.master > min_alpha
        && inputs.complex_spheres.fill.lens != ColorLens::Off
        && inputs.complex_spheres.radius.lens != AtomSizeMode::Off;
    let draw_iface_spheres = iface.is_some()
        && inputs.interface_spheres.as_ref().is_some_and(|(gate, rep)| {
            gate.visible
                && gate.master > min_alpha
                && rep.fill.lens != ColorLens::Off
                && rep.radius.lens != AtomSizeMode::Off
        });

    let gpu_drawn = gpu_surfaces
        .and_then(|gpu| {
            paint_gpu_scene(
                painter,
                &view,
                ds,
                inputs,
                rendering,
                draw_surfaces,
                draw_boundary,
                draw_cartoon,
                draw_complex_spheres,
                draw_iface_spheres,
                gpu,
            )
        })
        .is_some();

    if !gpu_drawn {
        grid::paint(painter, rect, view.camera, view.grid, view.grid_color);
        let mut list = DrawList::default();
        if draw_surfaces {
            add_chain_surfaces(&mut list, &view, ds, inputs);
        }
        if let Some(iface_idx) = ds.active_interface
            && let Some(iface) = ds.interfaces.get(iface_idx)
            && draw_boundary
            && let Some(boundary) = inputs.boundary
        {
            add_interface_geometry(&mut list, &view, ds, iface, boundary);
        }
        if draw_cartoon {
            add_cartoon(&mut list, &view, ds, inputs);
        }
        if draw_complex_spheres {
            add_sphere_rep(&mut list, &view, ds, inputs.complex_spheres, inputs.complex_spheres_gate.master, false);
        }
        if draw_iface_spheres
            && let Some((gate, rep)) = &inputs.interface_spheres
        {
            add_sphere_rep(&mut list, &view, ds, rep, gate.master, true);
        }
        list.paint(painter);
    }
}

/// Per-frame world → screen pipeline and parameterized styling context shared by every layer.
struct View<'a> {
    frame: PlaneFrame,
    camera: &'a CameraState,
    explode_val: f32,
    complex_explode_val: f32,
    explode: &'a ExplodeParams,
    grid: &'a GridParams,
    grid_color: Color32,
    opacity: &'a OpacityParams,
    atom_draw: &'a AtomDrawParams,
    lenses: &'a LensParams,
    bfactor_mode: BFactorMode,
    pal: &'a Palette,
    rect: Rect,
}

impl View<'_> {
    fn project(&self, world: [f32; 3], offset: Vec3) -> (Pos2, f32) {
        self.camera.project(self.frame.to_local(world) + offset, self.rect)
    }

    fn chain_alpha(&self, side: InterfaceSide, base_alpha: f32, lens: ChainColorLens) -> f32 {
        if side == InterfaceSide::None && lens == ChainColorLens::InterfaceSide {
            base_alpha * self.opacity.unassigned_alpha_scale
        } else {
            base_alpha
        }
    }
}

fn mesh_wireframe_stroke(wf_width: f32, base: Color32, alpha: f32) -> Stroke {
    if wf_width > 0.0 && alpha > 0.004 {
        Stroke::new(wf_width, palette::with_alpha(palette::edge_color(base), alpha))
    } else {
        Stroke::NONE
    }
}

fn chain_total_offset(view: &View, side: InterfaceSide, draw_explode_offset: [f32; 3]) -> Vec3 {
    let iface_off = explode_offset(side, view.explode_val, view.explode);
    let complex_off = view.frame.vector_to_local(draw_explode_offset)
        * (view.complex_explode_val * view.explode.complex_explode_scale);
    iface_off + complex_off
}

fn interface_complex_offset(view: &View, ds: &ViewerDataset) -> Vec3 {
    if view.complex_explode_val <= 0.0 {
        return Vec3::ZERO;
    }
    let scale = view.complex_explode_val * view.explode.complex_explode_scale;
    let (mut sum_a, mut count_a) = (Vec3::ZERO, 0usize);
    let (mut sum_b, mut count_b) = (Vec3::ZERO, 0usize);
    for ch in &ds.chains {
        match ds.chain_side(&ch.chain_id) {
            InterfaceSide::SideA => {
                sum_a += view.frame.vector_to_local(ch.draw_explode_offset) * scale;
                count_a += 1;
            }
            InterfaceSide::SideB => {
                sum_b += view.frame.vector_to_local(ch.draw_explode_offset) * scale;
                count_b += 1;
            }
            InterfaceSide::None => {}
        }
    }
    let avg_a = if count_a > 0 { sum_a / count_a as f32 } else { Vec3::ZERO };
    let avg_b = if count_b > 0 { sum_b / count_b as f32 } else { Vec3::ZERO };
    0.5 * (avg_a + avg_b)
}

fn sides_color_sig(pal: &Palette) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for c in [pal.sides.side_a, pal.sides.side_b, pal.sides.unassigned] {
        for b in c.to_array() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// Iterates visible spheres for either interface atoms (`interface_only = true`) or all complex atoms.
fn for_each_sphere(
    view: &View,
    ds: &ViewerDataset,
    rep: &SphereRepStyle,
    interface_only: bool,
    mut emit: impl FnMut(usize, [f32; 3], Vec3, f32, f32, Color32, Option<(Color32, f32)>),
) {
    let fill_lens = rep.fill.lens;
    let size_mode = rep.radius.lens;
    if size_mode == AtomSizeMode::Off {
        return;
    }
    let stroke_color_lens = rep.stroke_color.lens;
    let stroke_width_lens = rep.stroke_width.lens;
    let fixed_stroke_w = rep.stroke_width.fixed;
    let inv_zoom = 1.0 / view.camera.zoom.max(1e-4);

    let emit_iface_node = |node: &smorgasprot_core::AtomNode,
                           meta: &crate::dataset::AtomMeta,
                           emit: &mut dyn FnMut(usize, [f32; 3], Vec3, f32, f32, Color32, Option<(Color32, f32)>)| {
        let chain_idx = ds
            .complex
            .as_ref()
            .and_then(|c| c.atoms.get(node.global_atom.0 as usize))
            .map_or(0, |a| a.chain_idx.0 as usize);
        let chain_explode = ds.chains.get(chain_idx).map_or([0.0; 3], |ch| ch.draw_explode_offset);
        let offset = chain_total_offset(view, node.side, chain_explode);
        let radius_px = sizing::complex_sphere_radius_px(
            node,
            meta.vdw_radius_a,
            size_mode,
            rep.radius.fixed,
            view.bfactor_mode,
            view.camera.zoom,
            view.atom_draw,
            view.lenses,
        );
        if radius_px <= 0.0 {
            return;
        }
        let base_color = lenses::atom_fill_color(
            node,
            meta,
            fill_lens,
            rep.fill.fixed,
            view.bfactor_mode,
            view.lenses,
            view.pal,
        );
        let stroke_width = sizing::atom_stroke_width_px(
            node,
            stroke_width_lens,
            fixed_stroke_w,
            view.bfactor_mode,
            false,
            view.lenses,
        );
        let outline = (stroke_color_lens != NodeStrokeColorLens::Off && stroke_width > 0.0).then(|| {
            let stroke_color = lenses::atom_stroke_color(
                node,
                meta,
                stroke_color_lens,
                rep.stroke_color.fixed,
                view.bfactor_mode,
                false,
                view.lenses,
                view.pal,
            );
            (stroke_color, stroke_width)
        });
        emit(chain_idx, node.pos_3d, offset, radius_px * inv_zoom, radius_px, base_color, outline);
    };

    if !interface_only
        && let Some(complex) = &ds.complex
    {
        let mut iface_lookup = vec![None; complex.atoms.len()];
        if let Some(iface) = ds.active_interface() {
            for (ni, node) in iface.map.nodes.iter().enumerate() {
                if let Some(slot) = iface_lookup.get_mut(node.global_atom.0 as usize) {
                    *slot = Some(ni);
                }
            }
        }
        let iface_opt = ds.active_interface();
        for atom in &complex.atoms {
            if let Some(ni) = iface_lookup[atom.idx.0 as usize]
                && let Some(iface) = iface_opt
            {
                emit_iface_node(&iface.map.nodes[ni], &iface.atom_meta[ni], &mut emit);
                continue;
            }
            let chain_idx = atom.chain_idx.0 as usize;
            let Some(ch) = ds.chains.get(chain_idx) else { continue };
            let side = ds.chain_side(&ch.chain_id);
            let offset = chain_total_offset(view, side, ch.draw_explode_offset);
            let mob_t = {
                let (v0, v1) = match view.bfactor_mode {
                    BFactorMode::AlphaFold => (view.lenses.plddt_very_high_min, view.lenses.plddt_low_min),
                    BFactorMode::Crystallographic => (view.lenses.bfactor_rigid_max_a2, view.lenses.bfactor_flexible_max_a2),
                };
                let span = v1 - v0;
                if span.abs() < 1e-6 { 0.5 } else { ((atom.b_factor - v0) / span).clamp(0.0, 1.0) }
            };
            let radius_scale = match size_mode {
                AtomSizeMode::Fixed => rep.radius.fixed,
                AtomSizeMode::Mobility => 0.65 + 0.85 * mob_t,
                AtomSizeMode::Rigidity => 0.65 + 0.85 * (1.0 - mob_t),
                _ => 1.0,
            };
            let radius_px = (atom.vdw_radius_a * view.camera.zoom * view.atom_draw.complex_sphere_radius_per_zoom * radius_scale)
                .max(view.atom_draw.min_atom_radius_px);
            if radius_px <= 0.0 {
                continue;
            }
            let base_color = match fill_lens {
                ColorLens::Fixed => rep.fill.fixed,
                ColorLens::Off => Color32::TRANSPARENT,
                ColorLens::ModernCpk => view.pal.elements.color(atom.element),
                ColorLens::AaChemistry => {
                    let res_name = complex
                        .residues
                        .get(atom.residue_idx.0 as usize)
                        .and_then(|r| std::str::from_utf8(&r.res_name).ok())
                        .unwrap_or("UNK");
                    lenses::aa_chemistry_color(res_name, 0.0, view.lenses, view.pal)
                }
                ColorLens::Bfactor => lenses::sample_metric_color(crate::state::ScalarMetric::Mobility, mob_t, view.pal),
                ColorLens::InterfaceSide | ColorLens::Interactions | ColorLens::BuriedSasa | ColorLens::AliphaticScore | ColorLens::ClashSeverity => {
                    view.pal.sides.side(side)
                }
            };
            let stroke_width = match stroke_width_lens {
                AtomSizeMode::Off => 0.0,
                AtomSizeMode::Fixed => fixed_stroke_w,
                AtomSizeMode::Mobility => 0.8 + 1.8 * mob_t,
                AtomSizeMode::Rigidity => 0.8 + 1.8 * (1.0 - mob_t),
                _ => fixed_stroke_w,
            };
            let outline = (stroke_color_lens != NodeStrokeColorLens::Off && stroke_width > 0.0).then(|| {
                let stroke_color = match stroke_color_lens {
                    NodeStrokeColorLens::Fixed => rep.stroke_color.fixed,
                    NodeStrokeColorLens::ModernCpk => view.pal.elements.color(atom.element),
                    NodeStrokeColorLens::InterfaceSide => view.pal.sides.side(side),
                    _ => view.pal.chrome.ink,
                };
                (stroke_color, stroke_width)
            });
            emit(chain_idx, atom.pos_3d, offset, radius_px * inv_zoom, radius_px, base_color, outline);
        }
    } else if let Some(iface) = ds.active_interface() {
        for (node, meta) in iface.map.nodes.iter().zip(&iface.atom_meta) {
            emit_iface_node(node, meta, &mut emit);
        }
    }
}

fn build_geodesic_spheres_mesh(atoms: &[([f32; 3], f32, Color32)], preset: GeodesicPreset) -> (SurfaceMesh, Vec<Color32>) {
    let (unit_verts, unit_tris) = unit_geodesic_sphere(preset);
    let nv = unit_verts.len();
    let nt = unit_tris.len();
    let mut vertices = Vec::with_capacity(atoms.len() * nv);
    let mut normals = Vec::with_capacity(atoms.len() * nv);
    let mut triangles = Vec::with_capacity(atoms.len() * nt);
    let mut colors = Vec::with_capacity(atoms.len() * nv);

    for &(center, radius_a, color) in atoms {
        let base = vertices.len() as u32;
        let c = Vec3::from(center);
        for &uv in &unit_verts {
            let n = Vec3::new(uv.x, uv.y, uv.z);
            vertices.push((c + n * radius_a).into());
            normals.push([n.x, n.y, n.z]);
            colors.push(color);
        }
        for &[i0, i1, i2] in &unit_tris {
            triangles.push([base + i0, base + i1, base + i2]);
        }
    }
    (
        SurfaceMesh { vertices, normals, triangles, nearest_atom: Vec::new() },
        colors,
    )
}

fn spheres_color_sig(view: &View, rep: &SphereRepStyle, interface_only: bool, preset: GeodesicPreset) -> u64 {
    let mut h = sides_color_sig(view.pal);
    let mix = |h: &mut u64, v: u64| {
        *h ^= v;
        *h = h.wrapping_mul(0x0100_0000_01b3);
    };
    mix(&mut h, interface_only as u64);
    mix(&mut h, preset as u64);
    mix(&mut h, rep.fill.lens as u64);
    mix(&mut h, u32::from_le_bytes(rep.fill.fixed.to_array()) as u64);
    mix(&mut h, rep.radius.lens as u64);
    mix(&mut h, rep.radius.fixed.to_bits() as u64);
    mix(&mut h, view.bfactor_mode as u64);
    mix(&mut h, view.atom_draw.complex_sphere_radius_per_zoom.to_bits() as u64);
    h
}

/// Calibrates the 3D prism cartoon radius (Å) to match the 2D capsule diameter at 1× scale.
fn cartoon_prism_radius_a(atom_draw: &AtomDrawParams, width_scale: f32) -> f32 {
    0.5 * atom_draw.complex_cartoon_width_per_zoom * width_scale
}

/// Builds a closed, parallel-transported N-sided prism mesh along `ca_positions` in world coordinates.
fn build_cartoon_prism_mesh(ca_positions: &[[f32; 3]], sides: usize, radius_a: f32) -> SurfaceMesh {
    let n = ca_positions.len();
    if n < 2 || sides < 3 || radius_a <= 0.0 {
        return SurfaceMesh::default();
    }
    let pts: Vec<Vec3> = ca_positions.iter().map(|&p| Vec3::from(p)).collect();
    let mut vertices = Vec::with_capacity(n * sides + 2);
    let mut normals = Vec::with_capacity(n * sides + 2);
    let mut triangles = Vec::with_capacity((n - 1) * sides * 2 + sides * 2);

    let mut tangents = Vec::with_capacity(n);
    for i in 0..n {
        let raw = if i == 0 {
            pts[1] - pts[0]
        } else if i + 1 == n {
            pts[n - 1] - pts[n - 2]
        } else {
            pts[i + 1] - pts[i - 1]
        };
        let t = raw.normalize_or_zero();
        tangents.push(if t.length_squared() > 0.5 { t } else { tangents.last().copied().unwrap_or(Vec3::Z) });
    }

    let mut u = tangents[0].any_orthogonal_vector();
    for (i, (&p, &t)) in pts.iter().zip(&tangents).enumerate() {
        if i > 0 {
            let proj = u - t * u.dot(t);
            u = if proj.length_squared() > 1e-6 {
                proj.normalize()
            } else {
                t.any_orthogonal_vector()
            };
        }
        let v = t.cross(u);
        for s in 0..sides {
            let angle = (s as f32 / sides as f32) * std::f32::consts::TAU;
            let (sa, ca) = angle.sin_cos();
            let dir = (u * ca + v * sa).normalize();
            vertices.push((p + dir * radius_a).into());
            normals.push(dir.into());
        }
    }

    for i in 0..(n - 1) {
        let r0 = (i * sides) as u32;
        let r1 = ((i + 1) * sides) as u32;
        for s in 0..sides {
            let s_next = (s + 1) % sides;
            let (a0, a1) = (r0 + s as u32, r0 + s_next as u32);
            let (b0, b1) = (r1 + s as u32, r1 + s_next as u32);
            triangles.push([a0, a1, b1]);
            triangles.push([a0, b1, b0]);
        }
    }

    let cap_start = vertices.len() as u32;
    vertices.push(pts[0].into());
    normals.push((-tangents[0]).into());
    for s in 0..sides {
        let s_next = (s + 1) % sides;
        triangles.push([cap_start, s_next as u32, s as u32]);
    }

    let cap_end = vertices.len() as u32;
    let last_ring = ((n - 1) * sides) as u32;
    vertices.push(pts[n - 1].into());
    normals.push(tangents[n - 1].into());
    for s in 0..sides {
        let s_next = (s + 1) % sides;
        triangles.push([cap_end, last_ring + s as u32, last_ring + s_next as u32]);
    }

    SurfaceMesh { vertices, normals, triangles, nearest_atom: Vec::new() }
}

fn collect_geodesic_sphere_meshes(
    view: &View,
    ds: &ViewerDataset,
    rep: &SphereRepStyle,
    interface_only: bool,
) -> Vec<(usize, SurfaceMesh, Vec<Color32>, Vec3)> {
    let Some(preset) = rep.presentation.quality.geodesic_preset() else {
        return Vec::new();
    };
    let chain_count = ds.chains.len().max(1);
    let mut by_chain: Vec<Vec<([f32; 3], f32, Color32)>> = vec![Vec::new(); chain_count];
    let mut chain_offsets: Vec<Vec3> = vec![Vec3::ZERO; chain_count];
    for_each_sphere(view, ds, rep, interface_only, |chain_idx, world_pos, offset, radius_a, _, color, _| {
        let ci = chain_idx.min(chain_count - 1);
        chain_offsets[ci] = offset;
        by_chain[ci].push((world_pos, radius_a, color));
    });
    by_chain
        .into_iter()
        .enumerate()
        .filter(|(_, atoms)| !atoms.is_empty())
        .map(|(chain_idx, atoms)| {
            let (mesh, colors) = build_geodesic_spheres_mesh(&atoms, preset);
            (chain_idx, mesh, colors, chain_offsets[chain_idx])
        })
        .collect()
}

fn push_mesh_draw<'a, Q>(
    mesh_draws: &mut Vec<GpuMeshDraw<'a>>,
    key: GpuMeshKey,
    mesh: &'a SurfaceMesh,
    vertex_colors: Option<&'a [Color32]>,
    color_sig: u64,
    offset: Vec3,
    color: Color32,
    edge_color: Color32,
    base_alpha: f32,
    edge_base_alpha: f32,
    culling: SurfaceCulling,
    pres: &PresentationStyle<Q>,
    min_alpha: f32,
) {
    let face_alpha = base_alpha * pres.alphas.fill;
    let wf_alpha = edge_base_alpha * pres.alphas.wireframe;
    let border_alpha = base_alpha * pres.alphas.border;
    let has_border = pres.border.width_px > 0.0 && pres.border.color.a() > 0 && border_alpha > min_alpha;
    let show_faces = face_alpha > min_alpha || has_border;
    let wf_width = if pres.wireframe.enabled && wf_alpha > min_alpha {
        pres.wireframe.width_px.max(0.5)
    } else {
        0.0
    };
    mesh_draws.push(GpuMeshDraw {
        key,
        mesh,
        vertex_colors,
        color_sig,
        offset,
        color,
        edge_color,
        alpha: if face_alpha > min_alpha { face_alpha } else { 0.0 },
        edge_alpha: wf_alpha,
        show_faces,
        wireframe_width_px: wf_width,
        culling,
        shading: pres.shading,
        border_width_px: pres.border.width_px,
        border_color: pres.border.color,
        border_alpha,
    });
}

#[allow(clippy::too_many_arguments)]
fn paint_gpu_scene(
    painter: &Painter,
    view: &View,
    ds: &ViewerDataset,
    inputs: &SceneInputs<'_>,
    rendering: &RenderingSettings,
    draw_surfaces: bool,
    draw_boundary: bool,
    draw_cartoon: bool,
    draw_complex_spheres: bool,
    draw_iface_spheres: bool,
    gpu: &mut GpuSurfaceRenderer,
) -> Option<()> {
    let min_alpha = view.opacity.min_draw_alpha;
    let mut mesh_draws = Vec::with_capacity(ds.chains.len() + 1);
    let mut line_draws = Vec::new();
    let mut disc_draws = Vec::new();
    let envelope_colors: Vec<Color32>;

    if view.grid.alpha > 0.004 && view.grid.grid_extent_a > 0.0 {
        grid::for_each_segment(view.grid, |a, b| {
            line_draws.push(GpuLineDraw {
                a,
                b,
                prev: None,
                next: None,
                color: view.grid_color,
                alpha: view.grid.alpha,
                width_pt: 1.0,
                dash: None,
                border_pt: 0.0,
                border_color: Color32::TRANSPARENT,
                border_alpha: 1.0,
            });
        });
    }

    let mut prism_meshes: Vec<(usize, SurfaceMesh, Vec3, Color32, f32)> = Vec::new();
    if draw_cartoon && inputs.cartoon.presentation.quality == CartoonStyle::Prism8 {
        let radius_a = cartoon_prism_radius_a(view.atom_draw, inputs.cartoon.width_scale);
        let lens = inputs.cartoon.color.lens;
        for (chain_idx, ch) in ds.chains.iter().enumerate() {
            let side = ds.chain_side(&ch.chain_id);
            let alpha = view.chain_alpha(side, inputs.cartoon_gate.master, lens);
            if alpha <= min_alpha || ch.ca_positions.len() < 2 {
                continue;
            }
            let offset = chain_total_offset(view, side, ch.draw_explode_offset);
            let color = lenses::chain_cartoon_color(chain_idx, side, lens, inputs.cartoon.color.fixed, view.pal);
            let mesh = build_cartoon_prism_mesh(&ch.ca_positions, 8, radius_a);
            prism_meshes.push((chain_idx, mesh, offset, color, alpha));
        }
    }

    let complex_sphere_meshes = if draw_complex_spheres {
        collect_geodesic_sphere_meshes(view, ds, inputs.complex_spheres, false)
    } else {
        Vec::new()
    };
    let iface_sphere_meshes = if draw_iface_spheres
        && let Some((_, rep)) = &inputs.interface_spheres
    {
        collect_geodesic_sphere_meshes(view, ds, rep, true)
    } else {
        Vec::new()
    };

    if draw_surfaces {
        let lens = inputs.surface.color.lens;
        let pres = &inputs.surface.presentation;
        for (chain_idx, ch) in ds.chains.iter().enumerate() {
            let side = ds.chain_side(&ch.chain_id);
            let alpha = view.chain_alpha(side, inputs.surfaces_gate.master, lens);
            if alpha <= min_alpha || ch.draw_surface_mesh.triangles.is_empty() {
                continue;
            }
            let color = lenses::chain_surface_color(chain_idx, side, lens, inputs.surface.color.fixed, view.pal);
            push_mesh_draw(
                &mut mesh_draws,
                GpuMeshKey::Chain(chain_idx),
                &ch.draw_surface_mesh,
                None,
                0,
                chain_total_offset(view, side, ch.draw_explode_offset),
                color,
                palette::edge_color(color),
                alpha,
                0.72 * alpha,
                ds.culling,
                pres,
                min_alpha,
            );
        }
    }

    if draw_boundary
        && let Some(boundary) = inputs.boundary
        && let Some(iface_idx) = ds.active_interface
        && let Some(iface) = ds.interfaces.get(iface_idx)
    {
        let a = boundary.alpha.value;
        let geo = &iface.draw_geometry;
        let sides = &view.pal.sides;
        let offset = interface_complex_offset(view, ds);
        let pres = &boundary.presentation;
        match boundary.mode {
            InterfaceDisplayMode::SmoothedSdf => {
                push_mesh_draw(
                    &mut mesh_draws,
                    GpuMeshKey::Boundary(iface_idx, InterfaceDisplayMode::SmoothedSdf),
                    &geo.sheet_mesh,
                    None,
                    0,
                    offset,
                    sides.sheet,
                    palette::edge_color(sides.sheet),
                    a * 0.78,
                    0.86 * a,
                    pres.culling,
                    pres,
                    min_alpha,
                );
            }
            InterfaceDisplayMode::ContactEnvelope => {
                envelope_colors = geo
                    .envelope_mesh
                    .vertices
                    .iter()
                    .enumerate()
                    .map(|(v, _)| {
                        let side = iface.draw_envelope_sides.get(v).copied().unwrap_or(InterfaceSide::None);
                        sides.envelope(side)
                    })
                    .collect();
                push_mesh_draw(
                    &mut mesh_draws,
                    GpuMeshKey::Boundary(iface_idx, InterfaceDisplayMode::ContactEnvelope),
                    &geo.envelope_mesh,
                    Some(&envelope_colors),
                    sides_color_sig(view.pal),
                    offset,
                    Color32::WHITE,
                    Color32::WHITE,
                    a,
                    0.63 * a,
                    pres.culling,
                    pres,
                    min_alpha,
                );
            }
            InterfaceDisplayMode::NegativeSpace => {
                push_mesh_draw(
                    &mut mesh_draws,
                    GpuMeshKey::Boundary(iface_idx, InterfaceDisplayMode::NegativeSpace),
                    &geo.negative_mesh,
                    None,
                    0,
                    offset,
                    sides.void,
                    palette::edge_color(sides.void),
                    a,
                    0.78 * a,
                    pres.culling,
                    pres,
                    min_alpha,
                );
            }
            InterfaceDisplayMode::Off => {}
        }
    }

    if draw_cartoon {
        let pres = &inputs.cartoon.presentation;
        if pres.quality == CartoonStyle::Capsule2D {
            let width_pt = sizing::complex_cartoon_width_px(view.camera.zoom, inputs.cartoon.width_scale, view.atom_draw);
            let lens = inputs.cartoon.color.lens;
            for (chain_idx, ch) in ds.chains.iter().enumerate() {
                let side = ds.chain_side(&ch.chain_id);
                let alpha = view.chain_alpha(side, inputs.cartoon_gate.master, lens);
                if alpha <= min_alpha || ch.ca_positions.len() < 2 {
                    continue;
                }
                let offset = chain_total_offset(view, side, ch.draw_explode_offset);
                let color = lenses::chain_cartoon_color(chain_idx, side, lens, inputs.cartoon.color.fixed, view.pal);
                let pts: Vec<Vec3> = ch.ca_positions.iter().map(|&p| view.frame.to_local(p) + offset).collect();
                for (i, w) in pts.windows(2).enumerate() {
                    line_draws.push(GpuLineDraw {
                        a: w[0],
                        b: w[1],
                        prev: i.checked_sub(1).map(|k| pts[k]),
                        next: pts.get(i + 2).copied(),
                        color,
                        alpha: alpha * pres.alphas.fill,
                        width_pt,
                        dash: None,
                        border_pt: pres.border.width_px,
                        border_color: pres.border.color,
                        border_alpha: alpha * pres.alphas.border,
                    });
                }
            }
        } else {
            let radius_a = cartoon_prism_radius_a(view.atom_draw, inputs.cartoon.width_scale);
            let sig = 8_u64 | ((radius_a.to_bits() as u64) << 8);
            for (chain_idx, mesh, offset, color, alpha) in &prism_meshes {
                push_mesh_draw(
                    &mut mesh_draws,
                    GpuMeshKey::Cartoon(*chain_idx),
                    mesh,
                    None,
                    sig,
                    *offset,
                    *color,
                    palette::edge_color(*color),
                    *alpha,
                    0.72 * *alpha,
                    pres.culling,
                    pres,
                    min_alpha,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_sphere_draws<'a>(
        view: &View,
        ds: &ViewerDataset,
        rep: &SphereRepStyle,
        master: f32,
        interface_only: bool,
        sphere_meshes: &'a [(usize, SurfaceMesh, Vec<Color32>, Vec3)],
        mesh_draws: &mut Vec<GpuMeshDraw<'a>>,
        disc_draws: &mut Vec<GpuDiscDraw>,
        min_alpha: f32,
    ) {
        let pres = &rep.presentation;
        if let Some(preset) = pres.quality.geodesic_preset() {
            let sig = spheres_color_sig(view, rep, interface_only, preset);
            for (chain_idx, mesh, colors, offset) in sphere_meshes {
                let key = if interface_only {
                    GpuMeshKey::InterfaceSpheres(*chain_idx)
                } else {
                    GpuMeshKey::Spheres(*chain_idx)
                };
                push_mesh_draw(
                    mesh_draws,
                    key,
                    mesh,
                    Some(colors),
                    sig,
                    *offset,
                    Color32::WHITE,
                    Color32::WHITE,
                    master,
                    0.72 * master,
                    pres.culling,
                    pres,
                    min_alpha,
                );
            }
        } else {
            let fill_alpha = master * pres.alphas.fill;
            let outline_alpha = master * pres.alphas.border;
            let shaded_3d = pres.quality == SphereQuality::Billboard3D;
            for_each_sphere(view, ds, rep, interface_only, |_, world_pos, offset, _, radius_pt, fill_color, outline| {
                let (outline_color, outline_pt) = outline.unwrap_or((Color32::TRANSPARENT, 0.0));
                disc_draws.push(GpuDiscDraw {
                    center: view.frame.to_local(world_pos) + offset,
                    radius_pt,
                    outline_pt,
                    fill_color,
                    fill_alpha,
                    outline_color,
                    outline_alpha,
                    shaded_3d,
                });
            });
        }
    }

    if draw_complex_spheres {
        push_sphere_draws(
            view,
            ds,
            inputs.complex_spheres,
            inputs.complex_spheres_gate.master,
            false,
            &complex_sphere_meshes,
            &mut mesh_draws,
            &mut disc_draws,
            min_alpha,
        );
    }
    if draw_iface_spheres
        && let Some((gate, rep)) = &inputs.interface_spheres
    {
        push_sphere_draws(
            view,
            ds,
            rep,
            gate.master,
            true,
            &iface_sphere_meshes,
            &mut mesh_draws,
            &mut disc_draws,
            min_alpha,
        );
    }

    let (instance_id, draw_gen) = ds.draw_cache_key();
    let texture_id = gpu.render(
        view.rect,
        painter.pixels_per_point(),
        view.pal.chrome.viewport_bg,
        view.camera,
        view.frame.to_local_matrix(),
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
    painter.image(texture_id, view.rect, uv, Color32::WHITE);
    Some(())
}

/// CPU fallback: molecular surface per chain.
fn add_chain_surfaces(list: &mut DrawList, view: &View, ds: &ViewerDataset, inputs: &SceneInputs<'_>) {
    let lens = inputs.surface.color.lens;
    let pres = &inputs.surface.presentation;
    let show_faces = pres.alphas.fill > view.opacity.min_draw_alpha;
    let wf_width = if pres.wireframe.enabled { pres.wireframe.width_px } else { 0.0 };
    for (chain_idx, ch) in ds.chains.iter().enumerate() {
        let side = ds.chain_side(&ch.chain_id);
        let offset = chain_total_offset(view, side, ch.draw_explode_offset);
        let alpha = view.chain_alpha(side, inputs.surfaces_gate.master, lens);
        let base = lenses::chain_surface_color(chain_idx, side, lens, inputs.surface.color.fixed, view.pal);
        let edge = mesh_wireframe_stroke(wf_width, base, 0.72 * alpha * pres.alphas.wireframe);
        add_mesh(list, view, &ch.draw_surface_mesh, offset, ds.culling, pres.shading, |_, shade| {
            let fill = if show_faces {
                palette::shaded(base, shade, alpha * pres.alphas.fill)
            } else {
                Color32::TRANSPARENT
            };
            (fill, edge)
        });
    }
}

/// CPU fallback: derived interface boundary mesh.
fn add_interface_geometry(
    list: &mut DrawList,
    view: &View,
    ds: &ViewerDataset,
    iface: &crate::dataset::InterfaceViewData,
    boundary: &BoundaryLayerStyle,
) {
    let a = boundary.alpha.value;
    let geo = &iface.draw_geometry;
    let sides = &view.pal.sides;
    let offset = interface_complex_offset(view, ds);
    let pres = &boundary.presentation;
    let show_faces = pres.alphas.fill > view.opacity.min_draw_alpha;
    let wf_width = if pres.wireframe.enabled { pres.wireframe.width_px } else { 0.0 };
    match boundary.mode {
        InterfaceDisplayMode::SmoothedSdf => {
            let edge = mesh_wireframe_stroke(wf_width, sides.sheet, 0.86 * a * pres.alphas.wireframe);
            add_mesh(list, view, &geo.sheet_mesh, offset, pres.culling, pres.shading, |_, shade| {
                let fill = if show_faces {
                    palette::shaded(sides.sheet, shade, a * 0.78 * pres.alphas.fill)
                } else {
                    Color32::TRANSPARENT
                };
                (fill, edge)
            });
        }
        InterfaceDisplayMode::ContactEnvelope => {
            let colors: Vec<Color32> = geo
                .envelope_mesh
                .vertices
                .iter()
                .enumerate()
                .map(|(v, _)| sides.envelope(iface.draw_envelope_sides.get(v).copied().unwrap_or(InterfaceSide::None)))
                .collect();
            add_mesh(list, view, &geo.envelope_mesh, offset, pres.culling, pres.shading, |v, shade| {
                let base = colors[v];
                let fill = if show_faces {
                    palette::shaded(base, shade, a * pres.alphas.fill)
                } else {
                    Color32::TRANSPARENT
                };
                let edge = mesh_wireframe_stroke(wf_width, base, 0.63 * a * pres.alphas.wireframe);
                (fill, edge)
            });
        }
        InterfaceDisplayMode::NegativeSpace => {
            let edge = mesh_wireframe_stroke(wf_width, sides.void, 0.78 * a * pres.alphas.wireframe);
            add_mesh(list, view, &geo.negative_mesh, offset, pres.culling, pres.shading, |_, shade| {
                let fill = if show_faces {
                    palette::shaded(sides.void, shade, a * pres.alphas.fill)
                } else {
                    Color32::TRANSPARENT
                };
                (fill, edge)
            });
        }
        InterfaceDisplayMode::Off => {}
    }
}

/// CPU fallback: Cα trace per chain as thick segments or 3D 8-sided prism meshes.
fn add_cartoon(list: &mut DrawList, view: &View, ds: &ViewerDataset, inputs: &SceneInputs<'_>) {
    let width = sizing::complex_cartoon_width_px(view.camera.zoom, inputs.cartoon.width_scale, view.atom_draw);
    let lens = inputs.cartoon.color.lens;
    let pres = &inputs.cartoon.presentation;
    for (chain_idx, ch) in ds.chains.iter().enumerate() {
        let side = ds.chain_side(&ch.chain_id);
        let offset = chain_total_offset(view, side, ch.draw_explode_offset);
        let alpha = view.chain_alpha(side, inputs.cartoon_gate.master, lens);
        let color = lenses::chain_cartoon_color(chain_idx, side, lens, inputs.cartoon.color.fixed, view.pal);
        if pres.quality == CartoonStyle::Capsule2D {
            let stroke = Stroke::new(width, palette::with_alpha(color, alpha * pres.alphas.fill));
            let pts: Vec<(Pos2, f32)> = ch.ca_positions.iter().map(|&p| view.project(p, offset)).collect();
            for w in pts.windows(2) {
                list.segment(w[0].0, w[1].0, 0.5 * (w[0].1 + w[1].1), stroke);
            }
        } else {
            let mesh = build_cartoon_prism_mesh(
                &ch.ca_positions,
                8,
                cartoon_prism_radius_a(view.atom_draw, inputs.cartoon.width_scale),
            );
            add_mesh(list, view, &mesh, offset, pres.culling, pres.shading, |_, shade| {
                (palette::shaded(color, shade, alpha * pres.alphas.fill), Stroke::NONE)
            });
        }
    }
}

/// CPU fallback: complex or interface heavy-atom spheres.
fn add_sphere_rep(
    list: &mut DrawList,
    view: &View,
    ds: &ViewerDataset,
    rep: &SphereRepStyle,
    master: f32,
    interface_only: bool,
) {
    let pres = &rep.presentation;
    if pres.quality.geodesic_preset().is_some() {
        let fill_alpha = master * pres.alphas.fill;
        for (chain_idx, mesh, colors, offset) in collect_geodesic_sphere_meshes(view, ds, rep, interface_only) {
            let _ = chain_idx;
            add_mesh(list, view, &mesh, offset, pres.culling, pres.shading, |v, shade| {
                (palette::shaded(colors[v], shade, fill_alpha), Stroke::NONE)
            });
        }
    } else {
        let fill_alpha = master * pres.alphas.fill;
        let border_alpha = master * pres.alphas.border;
        for_each_sphere(view, ds, rep, interface_only, |_, world_pos, offset, _, radius_pt, base_color, outline| {
            let local_center = view.frame.to_local(world_pos) + offset;
            let (center, depth) = view.camera.project(local_center, view.rect);
            let fill = palette::with_alpha(base_color, fill_alpha);
            let stroke = outline.map_or(Stroke::NONE, |(c, w)| Stroke::new(w, palette::with_alpha(c, border_alpha)));
            list.disc(center, radius_pt, depth, fill, stroke);
        });
    }
}

fn add_mesh(
    list: &mut DrawList,
    view: &View,
    mesh: &SurfaceMesh,
    offset: Vec3,
    culling: SurfaceCulling,
    shading: MeshShading,
    mut style: impl FnMut(usize, f32) -> (Color32, Stroke),
) {
    if mesh.triangles.is_empty() {
        return;
    }
    let projected: Vec<(Pos2, f32)> = mesh.vertices.iter().map(|&v| view.project(v, offset)).collect();

    for &[i0, i1, i2] in &mesh.triangles {
        let (v0, v1, v2) = (i0 as usize, i1 as usize, i2 as usize);
        let (p0, p1, p2) = (projected[v0], projected[v1], projected[v2]);

        let cross_z = (p1.0.x - p0.0.x) * (p2.0.y - p0.0.y) - (p1.0.y - p0.0.y) * (p2.0.x - p0.0.x);
        match culling {
            SurfaceCulling::None => {}
            SurfaceCulling::Backfaces if cross_z > 0.0 => continue,
            SurfaceCulling::Frontfaces if cross_z < 0.0 => continue,
            _ => {}
        }
        let shade = match shading {
            MeshShading::Unlit => 1.0,
            MeshShading::Flat | MeshShading::Smooth => {
                let n = (Vec3::from(mesh.normals[v0]) + Vec3::from(mesh.normals[v1]) + Vec3::from(mesh.normals[v2])) / 3.0;
                0.55 + 0.45 * n.dot(LIGHT_DIR).abs()
            }
        };
        let (fill, edge) = style(v0, shade);
        let depth = (p0.1 + p1.1 + p2.1) / 3.0;
        list.triangle([p0.0, p1.0, p2.0], depth, fill, edge);
    }
}
