//! 2D/3D interface cartography scene: Voronoi residue territories, 1st-shell intra springs, cross-contact arcs, and atom picking.

use crate::camera::{compute_interface_atom_local_3d, world_to_plane_local, CameraState};
use crate::dataset::ViewerDataset;
use crate::style::{get_atom_fill_color, get_atom_radius_px, AtomSizeMode, BFactorMode, ColorLens};
use egui::{Align2, Color32, FontId, Painter, Rect, Response, Shape, Stroke};
use smorgasprot_core::{EdgeKind, InterfaceSide};

pub fn render_interface_scene(
    painter: &Painter,
    rect: Rect,
    ds: &ViewerDataset,
    camera: &CameraState,
    flatten_t: f32,
    book_theta: f32,
    use_relaxed_uv: bool,
    lens: ColorLens,
    bfactor_mode: BFactorMode,
    size_mode: AtomSizeMode,
    side_a_alpha: f32,
    side_b_alpha: f32,
    show_territories: bool,
    show_intra_edges: bool,
    show_cross_arcs: bool,
    show_backbone: bool,
    hovered_atom_idx: &mut Option<usize>,
    selected_territory_idx: &mut Option<usize>,
    response: &Response,
) {
    let plane = &ds.map.plane;

    // Draw Book Hinge axis line when book_theta > 0
    if book_theta > 0.01 && flatten_t > 0.1 {
        let (h1, _) = camera.project_local_point([plane.hinge_u, -28.0, 0.0], rect);
        let (h2, _) = camera.project_local_point([plane.hinge_u, 28.0, 0.0], rect);
        painter.line_segment([h1, h2], Stroke::new(1.5_f32, Color32::from_rgb(100, 116, 139)));
    }

    // 1. Draw 3D C-alpha backbone trace
    if show_backbone && flatten_t < 0.98 {
        let bb_alpha = 1.0 - flatten_t * 0.85;
        for ch in &ds.chains {
            let role = if ds.group_a_chains.iter().any(|c| c == &ch.chain_id) {
                InterfaceSide::SideA
            } else if ds.group_b_chains.iter().any(|c| c == &ch.chain_id) {
                InterfaceSide::SideB
            } else {
                continue;
            };
            let role_alpha = if role == InterfaceSide::SideA { side_a_alpha } else { side_b_alpha };
            if role_alpha <= 0.02 { continue; }

            let stroke_color = if role == InterfaceSide::SideA {
                Color32::from_rgba_unmultiplied(2, 132, 199, (bb_alpha * role_alpha * 0.45 * 255.0) as u8)
            } else {
                Color32::from_rgba_unmultiplied(234, 88, 12, (bb_alpha * role_alpha * 0.45 * 255.0) as u8)
            };
            let stroke = Stroke::new((camera.zoom * 0.16).max(2.0), stroke_color);

            let pts: Vec<_> = ch.ca_positions.iter().map(|&pos3d| {
                let loc3d = world_to_plane_local(pos3d, plane);
                let pt = [loc3d[0] * (1.0 - flatten_t) + loc3d[0] * flatten_t,
                          loc3d[1] * (1.0 - flatten_t) + loc3d[1] * flatten_t,
                          loc3d[2] * (1.0 - flatten_t)];
                let (scr, _) = camera.project_local_point(pt, rect);
                scr
            }).collect();

            for pair in pts.windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
        }
    }

    // Projected nodes
    let proj_nodes: Vec<_> = ds.map.nodes.iter().map(|n| {
        let uv = if use_relaxed_uv { n.pos_uv } else { n.pos_uv_raw };
        let loc = compute_interface_atom_local_3d(n.pos_3d, uv, n.role, plane, flatten_t, book_theta);
        camera.project_local_point(loc, rect)
    }).collect();

    // Hover picking
    if let Some(mouse_pos) = response.hover_pos() {
        let mut best_idx = None;
        let mut best_d2 = 196.0_f32; // 14 px radius
        for (idx, (p, _)) in proj_nodes.iter().enumerate() {
            let role_alpha = if ds.map.nodes[idx].role == InterfaceSide::SideA { side_a_alpha } else { side_b_alpha };
            if role_alpha <= 0.05 { continue; }
            let d2 = p.distance_sq(mouse_pos);
            if d2 < best_d2 {
                best_d2 = d2;
                best_idx = Some(idx);
            }
        }
        *hovered_atom_idx = best_idx;
        if response.clicked() {
            if let Some(h_idx) = best_idx {
                *selected_territory_idx = Some(ds.map.nodes[h_idx].territory.0 as usize);
            } else {
                *selected_territory_idx = None;
            }
        }
    }

    // 2. Draw Residue Territory Contours
    if show_territories {
        for role_pass in [InterfaceSide::SideB, InterfaceSide::SideA] {
            let role_alpha = if role_pass == InterfaceSide::SideA { side_a_alpha } else { side_b_alpha };
            if role_alpha <= 0.02 { continue; }

            for (t_idx, terr) in ds.map.territories.iter().enumerate() {
                if terr.role != role_pass || terr.contour_uv.is_empty() { continue; }

                let pts: Vec<_> = terr.contour_uv.iter().zip(&terr.contour_3d).map(|(&uv, &p3d)| {
                    let loc = compute_interface_atom_local_3d(p3d, uv, terr.role, plane, flatten_t, book_theta);
                    let (scr, _) = camera.project_local_point(loc, rect);
                    scr
                }).collect();

                let is_selected = *selected_territory_idx == Some(t_idx);
                let (fill, stroke) = if terr.role == InterfaceSide::SideA {
                    let alpha = if is_selected { 0.40 } else { 0.14 };
                    (Color32::from_rgba_unmultiplied(14, 165, 233, (role_alpha * alpha * 255.0) as u8),
                     Stroke::new(if is_selected { 2.0_f32 } else { 1.2_f32 }, Color32::from_rgba_unmultiplied(2, 132, 199, (role_alpha * 0.7 * 255.0) as u8)))
                } else {
                    let alpha = if is_selected { 0.40 } else { 0.14 };
                    (Color32::from_rgba_unmultiplied(249, 115, 22, (role_alpha * alpha * 255.0) as u8),
                     Stroke::new(if is_selected { 2.0_f32 } else { 1.2_f32 }, Color32::from_rgba_unmultiplied(234, 88, 12, (role_alpha * 0.7 * 255.0) as u8)))
                };

                painter.add(Shape::convex_polygon(pts, fill, stroke));

                // Territory label
                let loc_center = compute_interface_atom_local_3d(plane.origin, terr.label_center_uv, terr.role, plane, flatten_t, book_theta);
                let (scr_lbl, _) = camera.project_local_point(loc_center, rect);
                painter.text(
                    scr_lbl,
                    Align2::CENTER_CENTER,
                    &terr.label_top,
                    FontId::proportional((camera.zoom * 0.22).clamp(9.0, 14.0)),
                    Color32::from_rgba_unmultiplied(15, 23, 42, (role_alpha * 0.85 * 255.0) as u8),
                );
            }
        }
    }

    // 3. Draw 1st-Shell Intra-Chain Edges
    if show_intra_edges {
        for edge in &ds.map.edges {
            if edge.is_cross_interface { continue; }
            let na = &ds.map.nodes[edge.a.0 as usize];
            let role_alpha = if na.role == InterfaceSide::SideA { side_a_alpha } else { side_b_alpha };
            if role_alpha <= 0.05 { continue; }

            let (p0, _) = proj_nodes[edge.a.0 as usize];
            let (p1, _) = proj_nodes[edge.b.0 as usize];
            let stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(148, 163, 184, (role_alpha * 0.45 * 255.0) as u8));
            painter.line_segment([p0, p1], stroke);
        }
    }

    // 4. Draw Cross-Interface Contact Arcs
    if show_cross_arcs {
        let cross_alpha = side_a_alpha.min(side_b_alpha);
        if cross_alpha > 0.05 {
            for edge in &ds.map.edges {
                if !edge.is_cross_interface { continue; }
                let (p0, _) = proj_nodes[edge.a.0 as usize];
                let (p1, _) = proj_nodes[edge.b.0 as usize];

                let (color, width): (Color32, f32) = match edge.kind {
                    EdgeKind::Clash => (Color32::from_rgb(242, 20, 20), 2.2_f32),
                    EdgeKind::SaltBridge => (Color32::from_rgb(157, 20, 255), 2.0_f32),
                    EdgeKind::HBond => (Color32::from_rgb(37, 99, 235), 1.8_f32),
                    _ => (Color32::from_rgb(148, 163, 184), 1.0_f32),
                };
                let stroke = Stroke::new(width, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (cross_alpha * 220.0) as u8));
                painter.line_segment([p0, p1], stroke);
            }
        }
    }

    // 5. Draw Atoms
    for (idx, node) in ds.map.nodes.iter().enumerate() {
        let role_alpha = if node.role == InterfaceSide::SideA { side_a_alpha } else { side_b_alpha };
        if role_alpha <= 0.02 { continue; }

        let (scr, _) = proj_nodes[idx];
        let meta = &ds.atom_meta[idx];
        let r = get_atom_radius_px(node, size_mode, camera.zoom);
        let color = get_atom_fill_color(node, meta, lens, bfactor_mode);
        let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (role_alpha * 255.0) as u8);

        let is_hovered = *hovered_atom_idx == Some(idx);
        let stroke = if is_hovered {
            Stroke::new(2.5_f32, Color32::WHITE)
        } else {
            Stroke::new(1.0_f32, Color32::from_rgb(15, 23, 42))
        };

        painter.circle(scr, r, fill, stroke);

        if is_hovered {
            painter.circle_stroke(scr, r + 3.0, Stroke::new(1.5_f32, Color32::from_rgb(14, 165, 233)));
        }
    }
}
