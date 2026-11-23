//! Complex tab scene: chain surfaces, derived interface geometry, Cα cartoon, and interface spheres.
//! All primitives go through one [`DrawList`] so they depth-sort against each other.

use super::draw_list::DrawList;
use super::plane_space::{PlaneFrame, explode_offset};
use crate::camera::CameraState;
use crate::dataset::{InterfaceViewData, ViewerDataset};
use crate::state::{
    AtomDrawParams, AtomSizeMode, BFactorMode, ChainColorLens, ColorLens, ComplexViewSettings, ExplodeParams,
    InterfaceDisplayMode, LensParams, OpacityParams, SurfaceCulling, UiState,
};
use crate::style::{Palette, lenses, palette, sizing};
use egui::{Color32, Painter, Pos2, Rect, Stroke};
use glam::Vec3;
use smorgasprot_core::{InterfaceSide, SurfaceMesh};

/// Fixed world-space light direction for Lambert shading (not normalized; matches the HTML viewer).
const LIGHT_DIR: Vec3 = Vec3::new(0.3, 0.5, 0.8);

/// Paints the Complex tab for the active dataset.
pub fn paint(painter: &Painter, rect: Rect, ds: &ViewerDataset, state: &UiState) {
    let iface = ds.active_interface();
    let frame = iface.map_or_else(|| PlaneFrame::world_centered(ds.default_origin), |i| PlaneFrame::from(&i.map.plane));
    let s = &state.complex;
    let opacity = &state.view_params.opacity;
    let view = View {
        frame,
        camera: &s.camera,
        explode: &state.view_params.explode,
        opacity,
        atom_draw: &state.view_params.atom_draw,
        lenses: &state.view_params.lenses,
        bfactor_mode: state.cartography.bfactor_mode,
        pal: &state.palette,
        rect,
    };
    let mut list = DrawList::default();
    let min_alpha = opacity.min_draw_alpha;

    if s.surface.visible && s.surface.alpha.value > min_alpha && s.surface.color.lens != ChainColorLens::Off {
        add_chain_surfaces(&mut list, &view, ds, s);
    }
    if let Some(iface) = iface {
        if s.boundary.mode != InterfaceDisplayMode::Off && s.boundary.alpha.value > min_alpha {
            add_interface_geometry(&mut list, &view, iface, s);
        }
    }
    if s.cartoon.visible && s.cartoon.alpha.value > min_alpha && s.cartoon.color.lens != ChainColorLens::Off {
        add_cartoon(&mut list, &view, ds, s);
    }
    if let Some(iface) = iface {
        if s.spheres.visible
            && s.spheres.alpha.value > min_alpha
            && s.spheres.fill.lens != ColorLens::Off
            && s.spheres.radius.lens != AtomSizeMode::Off
        {
            add_spheres(&mut list, &view, iface, s);
        }
    }
    list.paint(painter);
}

/// Per-frame world → screen pipeline and parameterized styling context shared by every layer.
struct View<'a> {
    frame: PlaneFrame,
    camera: &'a CameraState,
    explode: &'a ExplodeParams,
    opacity: &'a OpacityParams,
    atom_draw: &'a AtomDrawParams,
    lenses: &'a LensParams,
    bfactor_mode: BFactorMode,
    pal: &'a Palette,
    rect: Rect,
}

impl View<'_> {
    /// World point (+ plane-local offset) → `(screen position, depth)`.
    fn project(&self, world: [f32; 3], offset: Vec3) -> (Pos2, f32) {
        self.camera.project(self.frame.to_local(world) + offset, self.rect)
    }

    /// Effective opacity for a chain on `side`, dimming unassigned chains only when coloured by [`ChainColorLens::InterfaceSide`].
    fn chain_alpha(&self, side: InterfaceSide, base_alpha: f32, lens: ChainColorLens) -> f32 {
        if side == InterfaceSide::None && lens == ChainColorLens::InterfaceSide {
            base_alpha * self.opacity.unassigned_alpha_scale
        } else {
            base_alpha
        }
    }
}

/// Molecular surface per chain, pushed apart along ±n by the explode slider.
fn add_chain_surfaces(list: &mut DrawList, view: &View, ds: &ViewerDataset, s: &ComplexViewSettings) {
    let lens = s.surface.color.lens;
    for (chain_idx, ch) in ds.chains.iter().enumerate() {
        let side = ds.chain_side(&ch.chain_id);
        let offset = explode_offset(side, s.explode.value, view.explode);
        let alpha = view.chain_alpha(side, s.surface.alpha.value, lens);
        let base = lenses::chain_surface_color(chain_idx, side, lens, s.surface.color.fixed, view.pal);
        add_mesh(list, view, &ch.draw_surface_mesh, offset, s.surface.culling, |_, shade| {
            (palette::shaded(base, shade, alpha), Stroke::NONE)
        });
    }
}

/// The selected derived interface mesh; anchored at the interface, so it ignores explode.
fn add_interface_geometry(list: &mut DrawList, view: &View, iface: &InterfaceViewData, s: &ComplexViewSettings) {
    let a = s.boundary.alpha.value;
    let geo = &iface.draw_geometry;
    let sides = &view.pal.sides;
    let wf_width = if s.boundary.show_wireframe {
        view.atom_draw.boundary_wireframe_width_px
    } else {
        0.0
    };

    match s.boundary.mode {
        InterfaceDisplayMode::SmoothedSdf => {
            let fill = palette::with_alpha(sides.sheet, a * 0.78);
            let edge = if wf_width > 0.0 {
                Stroke::new(wf_width, palette::with_alpha(palette::edge_color(sides.sheet), 0.86))
            } else {
                Stroke::NONE
            };
            add_mesh(list, view, &geo.sheet_mesh, Vec3::ZERO, SurfaceCulling::None, |_, _| (fill, edge));
        }
        InterfaceDisplayMode::ContactEnvelope => {
            add_mesh(list, view, &geo.envelope_mesh, Vec3::ZERO, SurfaceCulling::None, |v0, shade| {
                let side = iface.draw_envelope_sides.get(v0).copied().unwrap_or(InterfaceSide::None);
                let fill = sides.envelope(side);
                let edge = if wf_width > 0.0 {
                    Stroke::new(wf_width, palette::with_alpha(palette::edge_color(fill), 0.63))
                } else {
                    Stroke::NONE
                };
                (palette::shaded(fill, shade, a), edge)
            });
        }
        InterfaceDisplayMode::NegativeSpace => {
            let edge = if wf_width > 0.0 {
                Stroke::new(wf_width, palette::with_alpha(palette::edge_color(sides.void), 0.78))
            } else {
                Stroke::NONE
            };
            add_mesh(list, view, &geo.negative_mesh, Vec3::ZERO, SurfaceCulling::None, |_, shade| {
                (palette::shaded(sides.void, shade, a), edge)
            });
        }
        InterfaceDisplayMode::Off => {}
    }
}

/// Cα trace per chain as thick segments.
fn add_cartoon(list: &mut DrawList, view: &View, ds: &ViewerDataset, s: &ComplexViewSettings) {
    let width = sizing::complex_cartoon_width_px(view.camera.zoom, s.cartoon.width_scale, view.atom_draw);
    let lens = s.cartoon.color.lens;
    for (chain_idx, ch) in ds.chains.iter().enumerate() {
        let side = ds.chain_side(&ch.chain_id);
        let offset = explode_offset(side, s.explode.value, view.explode);
        let alpha = view.chain_alpha(side, s.cartoon.alpha.value, lens);
        let color = lenses::chain_cartoon_color(chain_idx, side, lens, s.cartoon.color.fixed, view.pal);
        let stroke = Stroke::new(width, palette::with_alpha(color, alpha));
        let pts: Vec<(Pos2, f32)> = ch.ca_positions.iter().map(|&p| view.project(p, offset)).collect();
        for w in pts.windows(2) {
            list.segment(w[0].0, w[1].0, 0.5 * (w[0].1 + w[1].1), stroke);
        }
    }
}

/// Interface heavy atoms as discs colored and sized by the active 3D sphere visual channels.
fn add_spheres(list: &mut DrawList, view: &View, iface: &InterfaceViewData, s: &ComplexViewSettings) {
    let outline = Stroke::new(1.0_f32, view.pal.chrome.ink);
    let fill_lens = s.spheres.fill.lens;
    let size_mode = s.spheres.radius.lens;

    for (node, meta) in iface.map.nodes.iter().zip(&iface.atom_meta) {
        let (center, depth) = view.project(node.pos_3d, explode_offset(node.role, s.explode.value, view.explode));
        let radius = sizing::complex_sphere_radius_px(
            node,
            meta.vdw_radius_a,
            size_mode,
            s.spheres.radius.fixed,
            view.bfactor_mode,
            view.camera.zoom,
            view.atom_draw,
            view.lenses,
        );
        if radius <= 0.0 {
            continue;
        }
        let base_color = lenses::atom_fill_color(
            node,
            meta,
            fill_lens,
            s.spheres.fill.fixed,
            view.bfactor_mode,
            view.lenses,
            view.pal,
        );
        let fill = palette::with_alpha(base_color, s.spheres.alpha.value);
        list.disc(center, radius, depth, fill, outline);
    }
}

/// Projects a mesh and pushes one shaded triangle per face.
/// `style(first_vertex_index, lambert_shade)` returns the fill and edge stroke.
fn add_mesh(
    list: &mut DrawList,
    view: &View,
    mesh: &SurfaceMesh,
    offset: Vec3,
    culling: SurfaceCulling,
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
        let n = (Vec3::from(mesh.normals[v0]) + Vec3::from(mesh.normals[v1]) + Vec3::from(mesh.normals[v2])) / 3.0;
        let shade = 0.55 + 0.45 * n.dot(LIGHT_DIR).abs();
        let (fill, edge) = style(v0, shade);
        let depth = (p0.1 + p1.1 + p2.1) / 3.0;
        list.triangle([p0.0, p1.0, p2.0], depth, fill, edge);
    }
}
