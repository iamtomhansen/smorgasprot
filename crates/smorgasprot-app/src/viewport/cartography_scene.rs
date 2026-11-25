//! Cartography ("Sandwich") tab scene: atoms morphing between true 3D and the flat 2D map, with
//! territories for residues and edges for contacts. All spatial layers render via the shared `wgpu`
//! depth-tested pipeline (with the CPU `DrawList` kept as a fallback for Glow); hover rings and text
//! labels always draw on top via `egui::Painter`.

use super::draw_list::DrawList;
use super::grid;
use super::plane_space::PlaneFrame;
use crate::camera::CameraState;
use crate::dataset::{InterfaceViewData, ViewerDataset};
use crate::render::{GpuDiscDraw, GpuLineDraw, GpuPolyTri, GpuSurfaceRenderer};
use crate::state::{
    AtomDrawParams, AtomSizeMode, CartographyViewSettings, ColorLens, EdgeColorLens, EdgeWidthLens, GridParams,
    LensParams, NodeStrokeColorLens, RenderingSettings, Selection, TerritoryColorLens, UiState,
};
use crate::style::{Palette, lenses, palette, sizing};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, vec2};
use glam::Vec3;
use smorgasprot_core::{AtomNodeIdx, InterfaceEdge, InterfaceSide, ResidueTerritory};

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
        frame: PlaneFrame::from(&iface.map.plane),
        s,
        lenses: &state.view_params.lenses,
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
fn paint_gpu_scene(
    cx: &Ctx,
    selection: &Selection,
    rendering: &RenderingSettings,
    nodes: &[(Pos2, f32)],
    gpu: &mut GpuSurfaceRenderer,
) -> Option<Option<(Pos2, f32)>> {
    let cam_step = camera_toward_vec(cx.camera) * COPLANAR_LAYER_STEP_A;
    let mut poly_draws = Vec::new();
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

    if cx.s.territories.visible {
        for_each_territory(cx, selection, |terr, pts, fill, border, side_idx| {
            if let Some((color, alpha)) = fill {
                let off = cam_step * (2.0 + side_idx as f32);
                for tri in &terr.contour_triangles {
                    let [i0, i1, i2] = tri.map(|i| i as usize);
                    poly_draws.push(GpuPolyTri {
                        pts: [pts[i0] + off, pts[i1] + off, pts[i2] + off],
                        color,
                        alpha,
                    });
                }
            }
            if let Some((color, alpha, width_pt)) = border {
                let off = cam_step * (4.0 + side_idx as f32);
                let n = pts.len();
                for i in 0..n {
                    line_draws.push(GpuLineDraw {
                        a: pts[i] + off,
                        b: pts[(i + 1) % n] + off,
                        prev: (n >= 3).then(|| pts[(i + n - 1) % n] + off),
                        next: (n >= 3).then(|| pts[(i + 2) % n] + off),
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

    for_each_edge(cx, |e, color, alpha, width_pt| {
        let off = cam_step * if e.is_cross_interface { 8.0 } else { 7.0 };
        line_draws.push(GpuLineDraw {
            a: local_nodes[e.a.0 as usize] + off,
            b: local_nodes[e.b.0 as usize] + off,
            prev: None,
            next: None,
            color,
            alpha,
            width_pt,
            dash: None,
            border_pt: 0.0,
            border_color: Color32::TRANSPARENT,
            border_alpha: 1.0,
        });
    });

    let disc_off = cam_step * 9.0;
    let mut hover_ring = None;
    for_each_atom(cx, selection, |idx, radius_pt, fill, outline, hovered| {
        if fill.is_some() || outline.is_some() {
            let (fill_color, fill_alpha) = fill.unwrap_or((Color32::TRANSPARENT, 0.0));
            let (outline_color, outline_alpha, outline_pt) = outline.unwrap_or((Color32::TRANSPARENT, 0.0, 0.0));
            disc_draws.push(GpuDiscDraw {
                center: local_nodes[idx] + disc_off,
                radius_pt,
                outline_pt,
                fill_color,
                fill_alpha,
                outline_color,
                outline_alpha,
                shaded_3d: false,
            });
        }
        if hovered {
            hover_ring = Some((nodes[idx].0, radius_pt));
        }
    });

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
        &[],
        &poly_draws,
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
        for_each_territory(cx, selection, |terr, pts, fill, border, _| {
            let projected: Vec<(Pos2, f32)> = pts.iter().map(|&p| cx.camera.project(p, cx.rect)).collect();
            if let Some((color, alpha)) = fill {
                let c = palette::with_alpha(color, alpha);
                for tri in &terr.contour_triangles {
                    let [i0, i1, i2] = tri.map(|i| i as usize);
                    let depth = (projected[i0].1 + projected[i1].1 + projected[i2].1) / 3.0;
                    list.triangle([projected[i0].0, projected[i1].0, projected[i2].0], depth, c, Stroke::NONE);
                }
            }
            if let Some((color, alpha, width)) = border {
                let stroke = Stroke::new(width, palette::with_alpha(color, alpha));
                let n = projected.len();
                for i in 0..n {
                    let (a, b) = (projected[i], projected[(i + 1) % n]);
                    list.segment(a.0, b.0, 0.5 * (a.1 + b.1), stroke);
                }
            }
        });
    }
    for_each_edge(cx, |e, color, alpha, width| {
        let stroke = Stroke::new(width, palette::with_alpha(color, alpha));
        let (a, b) = (nodes[e.a.0 as usize], nodes[e.b.0 as usize]);
        list.segment(a.0, b.0, 0.5 * (a.1 + b.1), stroke);
    });
    let mut hover_ring = None;
    for_each_atom(cx, selection, |idx, radius, fill, outline, hovered| {
        let (pos, depth) = nodes[idx];
        let fill_color = fill.map_or(Color32::TRANSPARENT, |(c, a)| palette::with_alpha(c, a));
        let stroke = outline.map_or(Stroke::NONE, |(c, a, w)| Stroke::new(w, palette::with_alpha(c, a)));
        if fill_color != Color32::TRANSPARENT || stroke != Stroke::NONE {
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
    mut emit: impl FnMut(&ResidueTerritory, &[Vec3], Option<(Color32, f32)>, Option<(Color32, f32, f32)>, usize),
) {
    let color_lens = cx.s.territories.fill.lens;
    let stroke_lens = cx.s.territories.stroke_width.lens;
    let fixed_stroke_w = cx.s.territories.stroke_width.fixed;

    for (side_idx, side) in cx.s.side_paint_order().into_iter().enumerate() {
        let alpha = cx.s.side_alpha(side);
        if alpha <= MIN_DRAW_ALPHA {
            continue;
        }
        for terr in cx.iface.map.territories.iter().filter(|t| t.side == side && !t.contour_uv.is_empty()) {
            // Contours exist only for relaxed (u, v), so they ignore the raw-UV toggle.
            let pts: Vec<Vec3> = terr
                .contour_uv
                .iter()
                .zip(&terr.contour_3d)
                .map(|(&uv, &p)| cx.morph_local(p, uv, side))
                .collect();

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
            let fill = (color_lens != TerritoryColorLens::Off).then_some((base_color, alpha * TERRITORY_FILL_ALPHA));
            let border_base = if color_lens == TerritoryColorLens::Off {
                let side_color = cx.pal.sides.side(terr.side);
                if selected { palette::highlight(side_color) } else { side_color }
            } else {
                base_color
            };
            let border = (width > 0.0).then(|| (palette::edge_color(border_base), alpha * TERRITORY_BORDER_ALPHA_SCALE, width));
            emit(terr, &pts, fill, border, side_idx);
        }
    }
}

/// Contact edges — first-shell springs within a side, and cross-interface contacts.
fn for_each_edge(cx: &Ctx, mut emit: impl FnMut(&InterfaceEdge, Color32, f32, f32)) {
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
        emit(e, color, alpha * alpha_scale, width * width_scale);
    }
}

/// Atom discs colored and sized by the active node fill, radius, stroke color, and stroke width lenses.
fn for_each_atom(
    cx: &Ctx,
    selection: &Selection,
    mut emit: impl FnMut(usize, f32, Option<(Color32, f32)>, Option<(Color32, f32, f32)>, bool),
) {
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
