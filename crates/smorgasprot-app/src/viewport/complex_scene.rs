//! Complex tab scene: chain surfaces, derived interface geometry, Cα cartoon, and interface spheres.
//! All primitives go through one [`DrawList`] so they depth-sort against each other.

use super::draw_list::DrawList;
use super::plane_space::{PlaneFrame, explode_offset};
use crate::camera::CameraState;
use crate::dataset::{InterfaceViewData, ViewerDataset};
use crate::state::{ComplexViewSettings, ExplodeParams, InterfaceDisplayMode};
use crate::style::{ChromePalette, InterfaceSidePalette, palette};
use egui::{Color32, Painter, Pos2, Rect, Stroke};
use glam::Vec3;
use smorgasprot_core::{InterfaceSide, SurfaceMesh};

/// Layers at or below this opacity are skipped entirely.
const MIN_VISIBLE_ALPHA: f32 = 0.02;
/// Opacity multiplier for chains in neither group, so the active sandwich stands out.
const UNASSIGNED_ALPHA_SCALE: f32 = 0.45;
/// Fixed world-space light direction for Lambert shading (not normalized; matches the HTML viewer).
const LIGHT_DIR: Vec3 = Vec3::new(0.3, 0.5, 0.8);

/// Paints the Complex tab for the active dataset.
pub fn paint(
    painter: &Painter,
    rect: Rect,
    ds: &ViewerDataset,
    s: &ComplexViewSettings,
    explode: &ExplodeParams,
    sides: &InterfaceSidePalette,
    chrome: &ChromePalette,
) {
    let iface = ds.active_interface();
    let frame = iface.map_or_else(|| PlaneFrame::world_centered(ds.default_origin), |i| PlaneFrame::from(&i.map.plane));
    let view = View { frame, camera: &s.camera, explode, sides, chrome, rect };
    let mut list = DrawList::default();

    if s.show_surface && s.surface_alpha.value > MIN_VISIBLE_ALPHA {
        add_chain_surfaces(&mut list, &view, ds, s);
    }
    if let Some(iface) = iface {
        if s.interface_mode != InterfaceDisplayMode::Off && s.interface_alpha.value > MIN_VISIBLE_ALPHA {
            add_interface_geometry(&mut list, &view, iface, s);
        }
    }
    if s.show_cartoon && s.cartoon_alpha.value > MIN_VISIBLE_ALPHA {
        add_cartoon(&mut list, &view, ds, s);
    }
    if let Some(iface) = iface {
        if s.show_spheres && s.spheres_alpha.value > MIN_VISIBLE_ALPHA {
            add_spheres(&mut list, &view, iface, s);
        }
    }
    list.paint(painter);
}

/// Per-frame world → screen pipeline shared by every layer.
struct View<'a> {
    frame: PlaneFrame,
    camera: &'a CameraState,
    explode: &'a ExplodeParams,
    sides: &'a InterfaceSidePalette,
    chrome: &'a ChromePalette,
    rect: Rect,
}

impl View<'_> {
    /// World point (+ plane-local offset) → `(screen position, depth)`.
    fn project(&self, world: [f32; 3], offset: Vec3) -> (Pos2, f32) {
        self.camera.project(self.frame.to_local(world) + offset, self.rect)
    }
}

/// Molecular surface per chain, pushed apart along ±n by the explode slider.
fn add_chain_surfaces(list: &mut DrawList, view: &View, ds: &ViewerDataset, s: &ComplexViewSettings) {
    for ch in &ds.chains {
        let side = ds.chain_side(&ch.chain_id);
        let offset = explode_offset(side, s.explode.value, view.explode);
        let alpha = match side {
            InterfaceSide::None => s.surface_alpha.value * UNASSIGNED_ALPHA_SCALE,
            _ => s.surface_alpha.value,
        };
        let base = view.sides.side(side);
        add_mesh(list, view, &ch.surface_mesh, offset, s.cull_backfaces, |_, shade| {
            (palette::shaded(base, shade, alpha), Stroke::NONE)
        });
    }
}

/// The selected derived interface mesh; anchored at the interface, so it ignores explode.
fn add_interface_geometry(list: &mut DrawList, view: &View, iface: &InterfaceViewData, s: &ComplexViewSettings) {
    let a = s.interface_alpha.value;
    let geo = &iface.geometry;
    let sides = view.sides;
    match s.interface_mode {
        InterfaceDisplayMode::SmoothedSdf => {
            // Flat (unshaded) fill with a fine wireframe.
            let fill = palette::with_alpha(sides.sheet, a * 0.78);
            let edge = Stroke::new(0.5_f32, palette::with_alpha(palette::edge_color(sides.sheet), 0.86));
            add_mesh(list, view, &geo.sheet_mesh, Vec3::ZERO, false, |_, _| (fill, edge));
        }
        InterfaceDisplayMode::ContactEnvelope => {
            // Color each triangle by the side of its first vertex's nearest atom.
            add_mesh(list, view, &geo.envelope_mesh, Vec3::ZERO, false, |v0, shade| {
                let side = iface.envelope_sides.get(v0).copied().unwrap_or(InterfaceSide::None);
                let fill = sides.envelope(side);
                let edge = palette::edge_color(fill);
                (palette::shaded(fill, shade, a), Stroke::new(0.6_f32, palette::with_alpha(edge, 0.63)))
            });
        }
        InterfaceDisplayMode::NegativeSpace => {
            let edge = Stroke::new(0.5_f32, palette::with_alpha(palette::edge_color(sides.void), 0.78));
            add_mesh(list, view, &geo.negative_mesh, Vec3::ZERO, false, |_, shade| {
                (palette::shaded(sides.void, shade, a), edge)
            });
        }
        InterfaceDisplayMode::Off => {}
    }
}

/// Cα trace per chain as thick segments.
fn add_cartoon(list: &mut DrawList, view: &View, ds: &ViewerDataset, s: &ComplexViewSettings) {
    let width = (view.camera.zoom * 0.28).max(3.0);
    for ch in &ds.chains {
        let side = ds.chain_side(&ch.chain_id);
        let offset = explode_offset(side, s.explode.value, view.explode);
        let (color, alpha) = match side {
            InterfaceSide::None => (view.sides.unassigned, s.cartoon_alpha.value * UNASSIGNED_ALPHA_SCALE),
            side => (view.sides.side_dark(side), s.cartoon_alpha.value),
        };
        let stroke = Stroke::new(width, palette::with_alpha(color, alpha));
        let pts: Vec<(Pos2, f32)> = ch.ca_positions.iter().map(|&p| view.project(p, offset)).collect();
        for w in pts.windows(2) {
            list.segment(w[0].0, w[1].0, 0.5 * (w[0].1 + w[1].1), stroke);
        }
    }
}

/// Interface heavy atoms as van der Waals–scaled discs.
fn add_spheres(list: &mut DrawList, view: &View, iface: &InterfaceViewData, s: &ComplexViewSettings) {
    let outline = Stroke::new(1.0_f32, view.chrome.ink);
    for (node, meta) in iface.map.nodes.iter().zip(&iface.atom_meta) {
        let (center, depth) = view.project(node.pos_3d, explode_offset(node.role, s.explode.value, view.explode));
        let radius = (meta.vdw_radius_a * view.camera.zoom * 0.35).max(3.5);
        let fill = palette::with_alpha(view.sides.side_light(node.role), s.spheres_alpha.value);
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
    cull_backfaces: bool,
    mut style: impl FnMut(usize, f32) -> (Color32, Stroke),
) {
    if mesh.triangles.is_empty() {
        return;
    }
    let projected: Vec<(Pos2, f32)> = mesh.vertices.iter().map(|&v| view.project(v, offset)).collect();

    for &[i0, i1, i2] in &mesh.triangles {
        let (v0, v1, v2) = (i0 as usize, i1 as usize, i2 as usize);
        let (p0, p1, p2) = (projected[v0], projected[v1], projected[v2]);

        // Screen-space winding (y points down); negative cross product faces away from the camera.
        let cross_z = (p1.0.x - p0.0.x) * (p2.0.y - p0.0.y) - (p1.0.y - p0.0.y) * (p2.0.x - p0.0.x);
        if cull_backfaces && cross_z < 0.0 {
            continue;
        }
        let n = (Vec3::from(mesh.normals[v0]) + Vec3::from(mesh.normals[v1]) + Vec3::from(mesh.normals[v2])) / 3.0;
        let shade = 0.55 + 0.45 * n.dot(LIGHT_DIR).abs();
        let (fill, edge) = style(v0, shade);
        let depth = (p0.1 + p1.1 + p2.1) / 3.0;
        list.triangle([p0.0, p1.0, p2.0], depth, fill, edge);
    }
}
