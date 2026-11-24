//! Cartography ("Sandwich") tab scene: atoms morphing between true 3D and the flat 2D map, with territories
//! indicating amino acid residues (and/or small molecules) and edges indicating contacts. Territories,
//! backbone, contact edges, and atoms depth-sort through the same CPU `DrawList` painter's algorithm as
//! the Complex tab; text labels stay a final always-on-top pass (no text primitive in `DrawList`).
//!
//! TODO: still CPU painter's-algorithm depth, not a real GPU depth buffer — revisit if/when the Complex
//! tab's own GPU migration (`render/surface.rs`) reaches this far.

use super::draw_list::DrawList;
use super::plane_space::PlaneFrame;
use crate::camera::CameraState;
use crate::dataset::{InterfaceViewData, ViewerDataset};
use crate::state::{
    AtomDrawParams, AtomSizeMode, CartographyViewSettings, ColorLens, EdgeColorLens, EdgeWidthLens, LensParams,
    NodeStrokeColorLens, Selection, TerritoryColorLens, UiState,
};
use crate::style::{Palette, lenses, palette, sizing};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, vec2};
use smorgasprot_core::{AtomNodeIdx, InterfaceSide};

/// Sides at or below this opacity are not drawn.
const MIN_DRAW_ALPHA: f32 = 0.02;
/// Sides at or below this opacity (e.g. soloed out) are ghosted: no picking, springs, or arcs.
const MIN_INTERACTIVE_ALPHA: f32 = 0.05;
/// Maximum pointer distance (px) for hovering an atom.
const PICK_RADIUS_PX: f32 = 14.0;

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

// Territory fill/border (`paint_territories`)
/// Base territory fill opacity; selection is shown via `palette::highlight`, not a second alpha tier.
const TERRITORY_FILL_ALPHA: f32 = 0.14;
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
    pal: &'a Palette,
}

impl Ctx<'_> {
    /// Morphs a point by the current flatten/butterfly state and projects it to screen, returning
    /// `(position, depth)` — depth follows `CameraState::project`'s "larger is nearer" convention,
    /// the same one `DrawList` sorts by.
    fn morph(&self, world: [f32; 3], uv: [f32; 2], side: InterfaceSide) -> (Pos2, f32) {
        let local = self.frame.morph(world, uv, side, self.s.flatten.value, self.s.book.value);
        self.camera.project(local, self.rect)
    }
}

/// Screen `(position, depth)` of every interface node under the current morph, indexed like `map.nodes`.
pub fn project_nodes(ds: &ViewerDataset, rect: Rect, s: &CartographyViewSettings) -> Vec<(Pos2, f32)> {
    let Some(iface) = ds.active_interface() else { return Vec::new() };
    let frame = PlaneFrame::from(&iface.map.plane);
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
pub fn paint(painter: &Painter, rect: Rect, ds: &ViewerDataset, state: &UiState, nodes: &[(Pos2, f32)]) {
    let Some(iface) = ds.active_interface() else {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No active interface selected — select or create an interface in the Complex tab.",
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
        frame: PlaneFrame::from(&iface.map.plane),
        s,
        lenses: &state.view_params.lenses,
        atom_draw: &state.view_params.atom_draw,
        pal: &state.palette,
    };

    paint_hinge(&cx);

    let mut list = DrawList::default();
    if s.show_backbone {
        paint_backbone(&cx, &mut list);
    }
    if s.territories.visible {
        paint_territories(&cx, &state.selection, &mut list);
    }
    paint_edges(&cx, nodes, &mut list);
    let hover_ring = paint_atoms(&cx, &state.selection, nodes, &mut list);
    list.paint(painter);

    // The hover ring is a UI affordance, not molecular content — always on top, like labels.
    if let Some((pos, radius)) = hover_ring {
        cx.painter.circle_stroke(pos, radius + HOVER_RING_OFFSET_PX, Stroke::new(HOVER_RING_WIDTH_PX, cx.pal.chrome.hover_ring()));
    }
    paint_labels(&cx, nodes);
}

/// Hinge line for the open-book butterfly, shown once the map is mostly flat and opened.
fn paint_hinge(cx: &Ctx) {
    if cx.s.book.value <= 0.01 || cx.s.flatten.value <= 0.1 {
        return;
    }
    let h = cx.frame.hinge_u;
    let (a, _) = cx.camera.project(glam::Vec3::new(h, -HINGE_HALF_LEN_A, 0.0), cx.rect);
    let (b, _) = cx.camera.project(glam::Vec3::new(h, HINGE_HALF_LEN_A, 0.0), cx.rect);
    cx.painter.line_segment([a, b], Stroke::new(HINGE_WIDTH_PX, cx.pal.chrome.hinge()));
}

/// Faint Cα trace of both groups, fading out as the map flattens.
fn paint_backbone(cx: &Ctx, list: &mut DrawList) {
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
        let stroke = Stroke::new(width, palette::with_alpha(cx.pal.sides.side_dark(side), fade * alpha * BACKBONE_ALPHA_SCALE));
        // Use each Cα's own tangent-plane (u, v) so the trace follows the butterfly hinge.
        let pts: Vec<(Pos2, f32)> = ch
            .ca_positions
            .iter()
            .map(|&p| {
                let local = cx.frame.to_local(p);
                cx.morph(p, [local.x, local.y], side)
            })
            .collect();
        for w in pts.windows(2) {
            list.segment(w[0].0, w[1].0, 0.5 * (w[0].1 + w[1].1), stroke);
        }
    }
}

/// Residue territory polygons; painted back-to-front per `CartographyViewSettings::side_paint_order`.
fn paint_territories(cx: &Ctx, selection: &Selection, list: &mut DrawList) {
    let color_lens = cx.s.territories.fill.lens;
    let stroke_lens = cx.s.territories.stroke_width.lens;
    let fixed_stroke_w = cx.s.territories.stroke_width.fixed;

    for side in cx.s.side_paint_order() {
        let alpha = cx.s.side_alpha(side);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        for terr in cx.iface.map.territories.iter().filter(|t| t.side == side && !t.contour_uv.is_empty()) {
            // Contours exist only for relaxed (u, v), so they ignore the raw-UV toggle.
            let pts: Vec<(Pos2, f32)> =
                terr.contour_uv.iter().zip(&terr.contour_3d).map(|(&uv, &p)| cx.morph(p, uv, side)).collect();

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
            // Selection is a derived highlight tint of the base color (`palette::highlight`, the
            // same kind of derived-color primitive `complex_scene` uses for Lambert shading), not
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
            let fill = if color_lens == TerritoryColorLens::Off {
                Color32::TRANSPARENT
            } else {
                palette::with_alpha(base_color, alpha * TERRITORY_FILL_ALPHA)
            };
            let border_base = if color_lens == TerritoryColorLens::Off {
                let side_color = cx.pal.sides.side(terr.side);
                if selected { palette::highlight(side_color) } else { side_color }
            } else {
                base_color
            };
            let stroke = if width <= 0.0 {
                Stroke::NONE
            } else {
                Stroke::new(width, palette::with_alpha(palette::edge_color(border_base), alpha * TERRITORY_BORDER_ALPHA_SCALE))
            };
            // `contour_triangles` is a one-time ear-clipping triangulation computed in
            // `contour::compute_territory_contours`, not redone here every frame — depth per
            // triangle, no per-triangle edge (the boundary below carries the stroke).
            if fill != Color32::TRANSPARENT {
                for tri in &terr.contour_triangles {
                    let [i0, i1, i2] = tri.map(|i| i as usize);
                    let depth = (pts[i0].1 + pts[i1].1 + pts[i2].1) / 3.0;
                    list.triangle([pts[i0].0, pts[i1].0, pts[i2].0], depth, fill, Stroke::NONE);
                }
            }
            if stroke != Stroke::NONE {
                let n = pts.len();
                for i in 0..n {
                    let (a, b) = (pts[i], pts[(i + 1) % n]);
                    list.segment(a.0, b.0, 0.5 * (a.1 + b.1), stroke);
                }
            }
        }
    }
}

/// Contact edges — first-shell springs within a side, and cross-interface contacts — styled by
/// the active color/width lenses (which already read `EdgeKind` for the "Interaction Kind"
/// lenses). "Intra vs. cross" is `is_cross_interface`, a derived positional fact about an edge,
/// not a kind of edge — so it's handled as one of several per-edge parameters here (its own
/// alpha/width scale and visibility toggle), not a split into separate painter functions.
fn paint_edges(cx: &Ctx, nodes: &[(Pos2, f32)], list: &mut DrawList) {
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
        let stroke = Stroke::new(width * width_scale, palette::with_alpha(color, alpha * alpha_scale));
        let (a, b) = (nodes[e.a.0 as usize], nodes[e.b.0 as usize]);
        list.segment(a.0, b.0, 0.5 * (a.1 + b.1), stroke);
    }
}

/// Atom discs colored and sized by the active node fill, radius, stroke color, and stroke width
/// channels. Returns the hovered atom's `(center, radius)`, if any, so the caller can draw its
/// ring after `list` is painted — a UI affordance, not molecular content, so always on top.
fn paint_atoms(cx: &Ctx, selection: &Selection, nodes: &[(Pos2, f32)], list: &mut DrawList) -> Option<(Pos2, f32)> {
    let fill_lens = cx.s.atoms.fill.lens;
    let size_mode = cx.s.atoms.radius.lens;
    let stroke_color_lens = cx.s.atoms.stroke_color.lens;
    let stroke_width_lens = cx.s.atoms.stroke_width.lens;
    let fixed_stroke_w = cx.s.atoms.stroke_width.fixed;
    let mut hover_ring = None;

    for ((node, meta), &(pos, depth)) in cx.iface.map.nodes.iter().zip(&cx.iface.atom_meta).zip(nodes) {
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
                        cx.s.atoms.fill.fixed,
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
                cx.s.atoms.stroke_color.fixed,
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
                list.disc(pos, radius, depth, fill, outline);
            }
            if hovered {
                hover_ring = Some((pos, radius));
            }
        }
    }
    hover_ring
}

/// Topmost z-layer: territory and atom text labels painted above all polygons, edges, and atom discs.
fn paint_labels(cx: &Ctx, nodes: &[(Pos2, f32)]) {
    if cx.s.territories.visible && cx.s.territories.labels.enabled {
        let terr_font = FontId::proportional((cx.camera.zoom * TERRITORY_FONT_ZOOM_SCALE).clamp(TERRITORY_FONT_MIN_PX, TERRITORY_FONT_MAX_PX));
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
                    let label_pos = cx.morph(terr.label_center_3d, terr.label_center_uv, side).0;
                    let ink = palette::with_alpha(cx.pal.chrome.ink, alpha * TERRITORY_LABEL_ALPHA_SCALE);
                    cx.painter.text(label_pos, Align2::CENTER_CENTER, label_text, terr_font.clone(), ink);
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
                let radius = sizing::atom_radius_px(
                    node,
                    size_mode,
                    cx.s.atoms.radius.fixed,
                    cx.s.bfactor_mode,
                    cx.camera.zoom,
                    cx.atom_draw,
                    cx.lenses,
                );
                let label_pos = pos + vec2(0.0, -(radius + ATOM_LABEL_OFFSET_PX));
                let ink = palette::with_alpha(cx.pal.chrome.ink, alpha * ATOM_LABEL_ALPHA_SCALE);
                cx.painter.text(label_pos, Align2::CENTER_BOTTOM, label_text, atom_font.clone(), ink);
            }
        }
    }
}
