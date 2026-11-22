//! Complex tab scene: chain surfaces, derived interface geometry, Cα cartoon, and interface spheres.
//! All primitives go through one [`DrawList`] so they depth-sort against each other.

use super::draw_list::DrawList;
use super::plane_space::{PlaneFrame, explode_offset};
use crate::camera::CameraState;
use crate::dataset::{InterfaceViewData, ViewerDataset};
use crate::state::{ComplexViewSettings, InterfaceDisplayMode};
use crate::style::palette;
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
pub fn paint(painter: &Painter, rect: Rect, ds: &ViewerDataset, camera: &CameraState, s: &ComplexViewSettings) {
    let Some(iface) = ds.active_interface() else { return };
    let view = View { frame: PlaneFrame::from(&iface.map.plane), camera, rect };
    let mut list = DrawList::default();

    if s.show_surface && s.surface_alpha > MIN_VISIBLE_ALPHA {
        add_chain_surfaces(&mut list, &view, ds, s);
    }
    if s.interface_mode != InterfaceDisplayMode::Off && s.interface_alpha > MIN_VISIBLE_ALPHA {
        add_interface_geometry(&mut list, &view, iface, s);
    }
    if s.show_cartoon && s.cartoon_alpha > MIN_VISIBLE_ALPHA {
        add_cartoon(&mut list, &view, ds, s);
    }
    if s.show_spheres && s.spheres_alpha > MIN_VISIBLE_ALPHA {
        add_spheres(&mut list, &view, iface, s);
    }
    list.paint(painter);
}

/// Per-frame world → screen pipeline shared by every layer.
struct View<'a> {
    frame: PlaneFrame,
    camera: &'a CameraState,
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
        let offset = explode_offset(side, s.explode.value);
        let alpha = match side {
            InterfaceSide::None => s.surface_alpha * UNASSIGNED_ALPHA_SCALE,
            _ => s.surface_alpha,
        };
        let base = palette::side(side);
        add_mesh(list, view, &ch.surface_mesh, offset, s.cull_backfaces, |_, shade| {
            (palette::shaded(base, shade, alpha), Stroke::NONE)
        });
    }
}

/// The selected derived interface mesh; anchored at the interface, so it ignores explode.
fn add_interface_geometry(list: &mut DrawList, view: &View, iface: &InterfaceViewData, s: &ComplexViewSettings) {
    let a = s.interface_alpha;
    let geo = &iface.geometry;
    match s.interface_mode {
        InterfaceDisplayMode::SmoothedSdf => {
            // Flat (unshaded) fill with a fine wireframe.
            let fill = palette::with_alpha(palette::SHEET_FILL, a * 0.78);
            let edge = Stroke::new(0.5_f32, palette::with_alpha(palette::SHEET_EDGE, 0.86));
            add_mesh(list, view, &geo.sheet_mesh, Vec3::ZERO, false, |_, _| (fill, edge));
        }
        InterfaceDisplayMode::ContactEnvelope => {
            // Color each triangle by the side of its first vertex's nearest atom.
            add_mesh(list, view, &geo.envelope_mesh, Vec3::ZERO, false, |v0, shade| {
                let side = iface.envelope_sides.get(v0).copied().unwrap_or(InterfaceSide::None);
                let (fill, edge) = match side {
                    InterfaceSide::SideA => (palette::ENVELOPE_A, palette::ENVELOPE_A_EDGE),
                    _ => (palette::ENVELOPE_B, palette::ENVELOPE_B_EDGE),
                };
                (palette::shaded(fill, shade, a), Stroke::new(0.6_f32, palette::with_alpha(edge, 0.63)))
            });
        }
        InterfaceDisplayMode::NegativeSpace => {
            let edge = Stroke::new(0.5_f32, palette::with_alpha(palette::VOID_EDGE, 0.78));
            add_mesh(list, view, &geo.negative_mesh, Vec3::ZERO, false, |_, shade| {
                (palette::shaded(palette::VOID, shade, a), edge)
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
        let offset = explode_offset(side, s.explode.value);
        let (color, alpha) = match side {
            InterfaceSide::None => (palette::UNASSIGNED, s.cartoon_alpha * UNASSIGNED_ALPHA_SCALE),
            side => (palette::side_dark(side), s.cartoon_alpha),
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
    let outline = Stroke::new(1.0_f32, palette::INK);
    for (node, meta) in iface.map.nodes.iter().zip(&iface.atom_meta) {
        let (center, depth) = view.project(node.pos_3d, explode_offset(node.role, s.explode.value));
        let radius = (meta.vdw_radius_a * view.camera.zoom * 0.35).max(3.5);
        let fill = palette::with_alpha(palette::side_light(node.role), s.spheres_alpha);
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
    style: impl Fn(usize, f32) -> (Color32, Stroke),
) {
    let proj: Vec<(Pos2, f32)> = mesh.vertices.iter().map(|&v| view.project(v, offset)).collect();

    for &[i0, i1, i2] in &mesh.triangles {
        let idx = [i0 as usize, i1 as usize, i2 as usize];
        let (Some(&(p0, d0)), Some(&(p1, d1)), Some(&(p2, d2))) =
            (proj.get(idx[0]), proj.get(idx[1]), proj.get(idx[2]))
        else {
            continue;
        };

        // Screen-space winding test; counter-clockwise on screen faces away.
        if cull_backfaces && (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x) >= 0.0 {
            continue;
        }

        let (fill, edge) = style(idx[0], lambert_shade(mesh, idx));
        list.triangle([p0, p1, p2], (d0 + d1 + d2) / 3.0, fill, edge);
    }
}

/// Ambient + diffuse shade from the face's averaged vertex normals, in `[0.35, 1]`.
fn lambert_shade(mesh: &SurfaceMesh, idx: [usize; 3]) -> f32 {
    let n: Vec3 = idx.iter().filter_map(|&i| mesh.normals.get(i)).map(|&n| Vec3::from(n)).sum::<Vec3>() / 3.0;
    (0.65 + 0.35 * n.dot(LIGHT_DIR)).clamp(0.35, 1.0)
}
