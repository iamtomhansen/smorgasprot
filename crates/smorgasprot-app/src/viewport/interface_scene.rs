//! Interface tab scene: atoms morphing between true 3D and the flat 2D map, with territories and contacts.
//! Painted in fixed layers (territories < springs < arcs < atoms) rather than depth-sorted.

use super::plane_space::PlaneFrame;
use crate::camera::CameraState;
use crate::dataset::{InterfaceViewData, ViewerDataset};
use crate::state::{
    AtomDrawParams, AtomSizeMode, ColorLens, EdgeColorLens, EdgeWidthLens, InterfaceViewSettings, LensParams,
    NodeStrokeColorLens, Selection, TerritoryColorLens, UiState,
};
use crate::style::{Palette, lenses, palette, sizing};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, vec2};
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
    lenses: &'a LensParams,
    atom_draw: &'a AtomDrawParams,
    pal: &'a Palette,
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
pub fn paint(painter: &Painter, rect: Rect, ds: &ViewerDataset, state: &UiState, nodes: &[Pos2]) {
    let Some(iface) = ds.active_interface() else { return };
    let s = &state.interface;
    let cx = Ctx {
        painter,
        rect,
        ds,
        iface,
        camera: &s.camera,
        frame: PlaneFrame::from(&iface.map.plane),
        s,
        lenses: &state.view_params.lenses,
        atom_draw: &state.view_params.atom_draw,
        pal: &state.palette,
    };

    paint_hinge(&cx);
    if s.show_backbone {
        paint_backbone(&cx);
    }
    if s.show_territories {
        paint_territories(&cx, &state.selection);
    }
    if s.show_intra_edges {
        paint_intra_edges(&cx, nodes);
    }
    if s.show_cross_arcs {
        paint_cross_arcs(&cx, nodes);
    }
    paint_atoms(&cx, &state.selection, nodes);
}

/// Hinge line for the open-book butterfly, shown once the map is mostly flat and opened.
fn paint_hinge(cx: &Ctx) {
    if cx.s.book.value <= 0.01 || cx.s.flatten.value <= 0.1 {
        return;
    }
    let h = cx.frame.hinge_u;
    let (a, _) = cx.camera.project(glam::Vec3::new(h, -HINGE_HALF_LEN_A, 0.0), cx.rect);
    let (b, _) = cx.camera.project(glam::Vec3::new(h, HINGE_HALF_LEN_A, 0.0), cx.rect);
    cx.painter.line_segment([a, b], Stroke::new(1.5_f32, cx.pal.chrome.hinge()));
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
        let stroke = Stroke::new(width, palette::with_alpha(cx.pal.sides.side_dark(side), fade * alpha * 0.45));
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
    let color_lens = cx.s.effective_territory_color_lens();
    let (stroke_lens, fixed_stroke_w) = cx.s.effective_territory_stroke_lens();

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
            let fill_a: f32 = if selected { 0.40 } else { 0.14 };
            let base_color = lenses::territory_fill_color(
                terr,
                &cx.iface.map.nodes,
                &cx.iface.atom_meta,
                color_lens,
                cx.s.fixed_territory_fill,
                cx.s.bfactor_mode,
                cx.lenses,
                cx.pal,
            );
            let width = sizing::territory_stroke_width_px(
                terr,
                &cx.iface.map.nodes,
                stroke_lens,
                fixed_stroke_w,
                cx.s.bfactor_mode,
                selected,
                cx.lenses,
            );
            let fill = if color_lens == TerritoryColorLens::Off {
                Color32::TRANSPARENT
            } else {
                palette::with_alpha(base_color, alpha * fill_a)
            };
            let border_base = if color_lens == TerritoryColorLens::Off {
                cx.pal.sides.side(terr.role)
            } else {
                base_color
            };
            let stroke = if width <= 0.0 {
                Stroke::NONE
            } else {
                Stroke::new(width, palette::with_alpha(palette::edge_color(border_base), alpha * 0.75))
            };
            if fill != Color32::TRANSPARENT || width > 0.0 {
                // Voronoi cells clipped to a convex hull stay convex, so a convex fill is valid.
                cx.painter.add(Shape::convex_polygon(pts, fill, stroke));
            }

            if let Some(label_text) = lenses::format_territory_label(
                terr,
                &cx.iface.map.nodes,
                &cx.iface.atom_meta,
                &cx.s.territory_labels,
            ) {
                let label_pos = cx.morph(terr.label_center_3d, terr.label_center_uv, side);
                let ink = palette::with_alpha(cx.pal.chrome.ink, alpha * 0.85);
                cx.painter.text(label_pos, Align2::CENTER_CENTER, label_text, font.clone(), ink);
            }
        }
    }
}

/// First-shell springs within each side.
fn paint_intra_edges(cx: &Ctx, nodes: &[Pos2]) {
    let color_lens = cx.s.effective_edge_color_lens();
    let width_lens = cx.s.effective_edge_width_lens();
    if color_lens == EdgeColorLens::Off || width_lens == EdgeWidthLens::Off {
        return;
    }
    for e in cx.iface.map.edges.iter().filter(|e| !e.is_cross_interface) {
        let side = cx.ds.node(e.a).map_or(InterfaceSide::None, |n| n.role);
        let alpha = cx.s.side_alpha(side);
        if alpha <= MIN_INTERACTIVE_ALPHA {
            continue;
        }
        let color = lenses::edge_stroke_color(
            e,
            &cx.iface.map.nodes,
            color_lens,
            cx.s.fixed_edge_color,
            cx.s.bfactor_mode,
            cx.lenses,
            cx.pal,
        );
        let width = sizing::edge_width_px(
            e,
            &cx.iface.map.nodes,
            width_lens,
            cx.s.fixed_edge_width_px,
            cx.s.bfactor_mode,
            cx.lenses,
        );
        if width <= 0.0 {
            continue;
        }
        let stroke = Stroke::new(width * 0.85, palette::with_alpha(color, alpha * 0.45));
        cx.painter.line_segment([nodes[e.a.0 as usize], nodes[e.b.0 as usize]], stroke);
    }
}

/// Cross-interface contacts colored and sized by the active edge visual channels.
fn paint_cross_arcs(cx: &Ctx, nodes: &[Pos2]) {
    let color_lens = cx.s.effective_edge_color_lens();
    let width_lens = cx.s.effective_edge_width_lens();
    if color_lens == EdgeColorLens::Off || width_lens == EdgeWidthLens::Off {
        return;
    }
    let alpha = cx.s.side_a_alpha.value.min(cx.s.side_b_alpha.value);
    if alpha <= MIN_INTERACTIVE_ALPHA {
        return;
    }
    for e in cx.iface.map.edges.iter().filter(|e| e.is_cross_interface) {
        let color = lenses::edge_stroke_color(
            e,
            &cx.iface.map.nodes,
            color_lens,
            cx.s.fixed_edge_color,
            cx.s.bfactor_mode,
            cx.lenses,
            cx.pal,
        );
        let width = sizing::edge_width_px(
            e,
            &cx.iface.map.nodes,
            width_lens,
            cx.s.fixed_edge_width_px,
            cx.s.bfactor_mode,
            cx.lenses,
        );
        if width <= 0.0 {
            continue;
        }
        let stroke = Stroke::new(width, palette::with_alpha(color, alpha * 0.86));
        cx.painter.line_segment([nodes[e.a.0 as usize], nodes[e.b.0 as usize]], stroke);
    }
}

/// Atom discs colored and sized by the active node fill, radius, stroke color, and stroke width channels.
fn paint_atoms(cx: &Ctx, selection: &Selection, nodes: &[Pos2]) {
    let fill_lens = cx.s.effective_lens();
    let size_mode = cx.s.effective_size_mode();
    let stroke_color_lens = cx.s.effective_stroke_color_lens();
    let (stroke_width_lens, fixed_stroke_w) = cx.s.effective_stroke_width_lens();
    let atom_font = FontId::proportional((cx.camera.zoom * 0.18).clamp(8.5, 12.5));

    for ((node, meta), &pos) in cx.iface.map.nodes.iter().zip(&cx.iface.atom_meta).zip(nodes) {
        let alpha = cx.s.side_alpha(node.role);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        let radius = sizing::atom_radius_px(
            node,
            size_mode,
            cx.s.fixed_node_radius_scale,
            cx.s.bfactor_mode,
            cx.camera.zoom,
            cx.atom_draw,
            cx.lenses,
        );
        let hovered = selection.hovered_atom == Some(node.global_atom);

        if size_mode != AtomSizeMode::Off && radius > 0.0 {
            let fill = if fill_lens == ColorLens::Off {
                Color32::TRANSPARENT
            } else {
                palette::with_alpha(
                    lenses::atom_fill_color(
                        node,
                        meta,
                        fill_lens,
                        cx.s.fixed_node_fill,
                        cx.s.bfactor_mode,
                        cx.lenses,
                        cx.pal,
                    ),
                    alpha,
                )
            };

            let stroke_color = lenses::atom_stroke_color(
                node,
                meta,
                stroke_color_lens,
                cx.s.fixed_node_stroke_color,
                cx.s.bfactor_mode,
                hovered,
                cx.lenses,
                cx.pal,
            );
            let stroke_width = sizing::atom_stroke_width_px(
                node,
                stroke_width_lens,
                fixed_stroke_w,
                cx.s.bfactor_mode,
                hovered,
                cx.lenses,
            );
            let outline = if stroke_color_lens == NodeStrokeColorLens::Off || stroke_width <= 0.0 {
                Stroke::NONE
            } else {
                Stroke::new(stroke_width, palette::with_alpha(stroke_color, alpha))
            };
            if fill != Color32::TRANSPARENT || outline != Stroke::NONE {
                cx.painter.circle(pos, radius, fill, outline);
            }
            if hovered {
                cx.painter.circle_stroke(pos, radius + 3.0, Stroke::new(1.5_f32, cx.pal.chrome.hover_ring()));
            }
        }

        if let Some(label_text) = lenses::format_atom_label(node, meta, &cx.s.atom_labels) {
            let label_pos = pos + vec2(0.0, -(radius + 2.0));
            let ink = palette::with_alpha(cx.pal.chrome.ink, alpha * 0.92);
            cx.painter.text(label_pos, Align2::CENTER_BOTTOM, label_text, atom_font.clone(), ink);
        }
    }
}
