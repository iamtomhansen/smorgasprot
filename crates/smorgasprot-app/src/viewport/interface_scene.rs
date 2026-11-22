//! Interface tab scene: atoms morphing between true 3D and the flat 2D map, with territories and contacts.
//! Painted in fixed layers (territories < springs < arcs < atoms) rather than depth-sorted.

use super::plane_space::PlaneFrame;
use crate::camera::CameraState;
use crate::dataset::{InterfaceViewData, ViewerDataset};
use crate::state::{InterfaceViewSettings, Selection};
use crate::style::{lenses, palette, sizing};
use egui::{Align2, FontId, Painter, Pos2, Rect, Shape, Stroke};
use smorgasprot_core::{AtomNodeIdx, InterfaceSide};

/// Sides at or below this opacity are not drawn.
const MIN_DRAW_ALPHA: f32 = 0.02;
/// Sides at or below this opacity (e.g. soloed out) are ghosted: no picking, springs, or arcs.
const MIN_INTERACTIVE_ALPHA: f32 = 0.05;
/// Maximum pointer distance (px) for hovering an atom.
const PICK_RADIUS_PX: f32 = 14.0;
/// Half-length (Å) of the drawn open-book hinge line.
const HINGE_HALF_LEN_A: f32 = 28.0;

/// Shared per-frame inputs for every layer.
struct Ctx<'a> {
    painter: &'a Painter,
    rect: Rect,
    ds: &'a ViewerDataset,
    iface: &'a InterfaceViewData,
    camera: &'a CameraState,
    frame: PlaneFrame,
    s: &'a InterfaceViewSettings,
}

impl Ctx<'_> {
    /// Morphs a point by the current flatten/butterfly state and projects it to screen.
    fn morph(&self, world: [f32; 3], uv: [f32; 2], side: InterfaceSide) -> Pos2 {
        let local = self.frame.morph(world, uv, side, self.s.flatten.value, self.s.book.value);
        self.camera.project(local, self.rect).0
    }
}

/// Screen positions of every interface node under the current morph, indexed like `map.nodes`.
pub fn project_nodes(ds: &ViewerDataset, rect: Rect, s: &InterfaceViewSettings) -> Vec<Pos2> {
    let Some(iface) = ds.active_interface() else { return Vec::new() };
    let frame = PlaneFrame::from(&iface.map.plane);
    iface
        .map
        .nodes
        .iter()
        .map(|n| {
            let uv = if s.use_relaxed_uv { n.pos_uv } else { n.pos_uv_raw };
            let local = frame.morph(n.pos_3d, uv, n.role, s.flatten.value, s.book.value);
            s.camera.project(local, rect).0
        })
        .collect()
}

/// Nearest visible node within [`PICK_RADIUS_PX`] of `pointer`.
pub fn pick_atom(ds: &ViewerDataset, s: &InterfaceViewSettings, nodes: &[Pos2], pointer: Pos2) -> Option<AtomNodeIdx> {
    let iface = ds.active_interface()?;
    nodes
        .iter()
        .zip(&iface.map.nodes)
        .filter(|(_, n)| s.side_alpha(n.role) > MIN_INTERACTIVE_ALPHA)
        .map(|(p, n)| (p.distance_sq(pointer), n.idx))
        .filter(|(d2, _)| *d2 < PICK_RADIUS_PX * PICK_RADIUS_PX)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, idx)| idx)
}

/// Paints the Interface tab; `nodes` comes from [`project_nodes`] for this frame.
pub fn paint(
    painter: &Painter,
    rect: Rect,
    ds: &ViewerDataset,
    s: &InterfaceViewSettings,
    selection: &Selection,
    nodes: &[Pos2],
) {
    let Some(iface) = ds.active_interface() else { return };
    let cx = Ctx { painter, rect, ds, iface, camera: &s.camera, frame: PlaneFrame::from(&iface.map.plane), s };

    paint_hinge(&cx);
    if s.show_backbone {
        paint_backbone(&cx);
    }
    if s.show_territories {
        paint_territories(&cx, selection);
    }
    if s.show_intra_edges {
        paint_intra_edges(&cx, nodes);
    }
    if s.show_cross_arcs {
        paint_cross_arcs(&cx, nodes);
    }
    paint_atoms(&cx, selection, nodes);
}

/// Hinge line for the open-book butterfly, shown once the map is mostly flat and opened.
fn paint_hinge(cx: &Ctx) {
    if cx.s.book.value <= 0.01 || cx.s.flatten.value <= 0.1 {
        return;
    }
    let h = cx.frame.hinge_u;
    let (a, _) = cx.camera.project(glam::Vec3::new(h, -HINGE_HALF_LEN_A, 0.0), cx.rect);
    let (b, _) = cx.camera.project(glam::Vec3::new(h, HINGE_HALF_LEN_A, 0.0), cx.rect);
    cx.painter.line_segment([a, b], Stroke::new(1.5_f32, palette::HINGE));
}

/// Faint Cα trace of both groups, fading out as the map flattens.
fn paint_backbone(cx: &Ctx) {
    let t = cx.s.flatten.value;
    if t >= 0.98 {
        return;
    }
    let fade = 1.0 - t * 0.85;
    let width = (cx.camera.zoom * 0.16).max(2.0);

    for ch in &cx.ds.chains {
        let side = cx.ds.chain_side(&ch.chain_id);
        let alpha = cx.s.side_alpha(side);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        let stroke = Stroke::new(width, palette::with_alpha(palette::side_dark(side), fade * alpha * 0.45));
        // Use each Cα's own tangent-plane (u, v) so the trace follows the butterfly hinge.
        let pts: Vec<Pos2> = ch
            .ca_positions
            .iter()
            .map(|&p| {
                let local = cx.frame.to_local(p);
                cx.morph(p, [local.x, local.y], side)
            })
            .collect();
        for w in pts.windows(2) {
            cx.painter.line_segment([w[0], w[1]], stroke);
        }
    }
}

/// Residue territory polygons with labels; side B first so side A reads on top.
fn paint_territories(cx: &Ctx, selection: &Selection) {
    let font = FontId::proportional((cx.camera.zoom * 0.22).clamp(9.0, 14.0));

    for side in [InterfaceSide::SideB, InterfaceSide::SideA] {
        let alpha = cx.s.side_alpha(side);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        for terr in cx.iface.map.territories.iter().filter(|t| t.role == side && !t.contour_uv.is_empty()) {
            // Contours exist only for relaxed (u, v), so they ignore the raw-UV toggle.
            let pts: Vec<Pos2> =
                terr.contour_uv.iter().zip(&terr.contour_3d).map(|(&uv, &p)| cx.morph(p, uv, side)).collect();

            let selected = selection.selected_residue == Some(terr.global_residue);
            let (fill_a, width): (f32, f32) = if selected { (0.40, 2.0) } else { (0.14, 1.2) };
            let fill = palette::with_alpha(palette::side(side), alpha * fill_a);
            let stroke = Stroke::new(width, palette::with_alpha(palette::side_dark(side), alpha * 0.7));
            // Voronoi cells clipped to a convex hull stay convex, so a convex fill is valid.
            cx.painter.add(Shape::convex_polygon(pts, fill, stroke));

            let label_pos = cx.morph(terr.label_center_3d, terr.label_center_uv, side);
            let ink = palette::with_alpha(palette::INK, alpha * 0.85);
            cx.painter.text(label_pos, Align2::CENTER_CENTER, &terr.label_top, font.clone(), ink);
        }
    }
}

/// First-shell springs within each side.
fn paint_intra_edges(cx: &Ctx, nodes: &[Pos2]) {
    for e in cx.iface.map.edges.iter().filter(|e| !e.is_cross_interface) {
        let side = cx.ds.node(e.a).map_or(InterfaceSide::None, |n| n.role);
        let alpha = cx.s.side_alpha(side);
        if alpha <= MIN_INTERACTIVE_ALPHA {
            continue;
        }
        let stroke = Stroke::new(1.0_f32, palette::with_alpha(palette::CONTACT, alpha * 0.45));
        cx.painter.line_segment([nodes[e.a.0 as usize], nodes[e.b.0 as usize]], stroke);
    }
}

/// Cross-interface contacts colored by interaction kind.
fn paint_cross_arcs(cx: &Ctx, nodes: &[Pos2]) {
    let alpha = cx.s.side_a_alpha.value.min(cx.s.side_b_alpha.value);
    if alpha <= MIN_INTERACTIVE_ALPHA {
        return;
    }
    for e in cx.iface.map.edges.iter().filter(|e| e.is_cross_interface) {
        let stroke = Stroke::new(lenses::contact_width(e.kind), palette::with_alpha(lenses::contact_color(e.kind), alpha * 0.86));
        cx.painter.line_segment([nodes[e.a.0 as usize], nodes[e.b.0 as usize]], stroke);
    }
}

/// Atom discs colored by the active lens and sized by the active sizing mode.
fn paint_atoms(cx: &Ctx, selection: &Selection, nodes: &[Pos2]) {
    for ((node, meta), &pos) in cx.iface.map.nodes.iter().zip(&cx.iface.atom_meta).zip(nodes) {
        let alpha = cx.s.side_alpha(node.role);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        let radius = sizing::atom_radius_px(node, cx.s.size_mode, cx.camera.zoom);
        let fill = palette::with_alpha(lenses::atom_fill_color(node, meta, cx.s.lens, cx.s.bfactor_mode), alpha);

        let hovered = selection.hovered_atom == Some(node.global_atom);
        let outline = if hovered { Stroke::new(2.5_f32, egui::Color32::WHITE) } else { Stroke::new(1.0_f32, palette::INK) };
        cx.painter.circle(pos, radius, fill, outline);
        if hovered {
            cx.painter.circle_stroke(pos, radius + 3.0, Stroke::new(1.5_f32, palette::HOVER_RING));
        }
    }
}
