//! 3D macromolecular complex scene: depth-sorted dual surfaces, interface representations, ribbons, and spheres.

use crate::camera::{CameraState, get_explode_offset_local, world_to_plane_local};
use crate::dataset::ViewerDataset;
use crate::state::InterfaceDisplayMode;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};
use smorgasprot_core::InterfaceSide;

pub struct DrawTriangle {
    pub p0: Pos2,
    pub p1: Pos2,
    pub p2: Pos2,
    pub depth: f32,
    pub fill: Color32,
    pub stroke: Stroke,
}

pub fn render_complex_scene(
    painter: &Painter,
    rect: Rect,
    ds: &ViewerDataset,
    camera: &CameraState,
    show_surface: bool,
    surface_alpha: f32,
    cull_backfaces: bool,
    show_cartoon: bool,
    cartoon_alpha: f32,
    interface_mode: InterfaceDisplayMode,
    interface_alpha: f32,
    show_spheres: bool,
    spheres_alpha: f32,
    explode_t: f32,
) {
    let plane = &ds.map.plane;
    let mut draw_triangles = Vec::new();

    // 1. Chain Surface Meshes
    if show_surface && surface_alpha > 0.02 {
        for ch in &ds.chains {
            let role = if ds.group_a_chains.iter().any(|c| c == &ch.chain_id) {
                InterfaceSide::SideA
            } else if ds.group_b_chains.iter().any(|c| c == &ch.chain_id) {
                InterfaceSide::SideB
            } else {
                InterfaceSide::None
            };

            let explode_shift = get_explode_offset_local(role, explode_t);
            let mesh = &ch.surface_mesh;

            let proj_verts: Vec<_> = mesh.vertices.iter().enumerate().map(|(v_idx, &v3d)| {
                let loc = glam::Vec3::from(world_to_plane_local(v3d, plane)) + glam::Vec3::from(explode_shift);
                let (screen, depth) = camera.project_local_point(loc.to_array(), rect);
                (screen, depth, mesh.normals[v_idx])
            }).collect();

            for &[i0, i1, i2] in &mesh.triangles {
                let u0 = i0 as usize;
                let u1 = i1 as usize;
                let u2 = i2 as usize;
                if u0 >= proj_verts.len() || u1 >= proj_verts.len() || u2 >= proj_verts.len() {
                    continue;
                }
                let (p0, d0, n0) = proj_verts[u0];
                let (p1, d1, n1) = proj_verts[u1];
                let (p2, d2, n2) = proj_verts[u2];

                if cull_backfaces {
                    let cross_z = (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x);
                    if cross_z >= 0.0 {
                        continue;
                    }
                }

                let depth = (d0 + d1 + d2) / 3.0;
                let nx = (n0[0] + n1[0] + n2[0]) / 3.0;
                let ny = (n0[1] + n1[1] + n2[1]) / 3.0;
                let nz = (n0[2] + n1[2] + n2[2]) / 3.0;
                let shade = (0.65 + 0.35 * (nx * 0.3 + ny * 0.5 + nz * 0.8)).clamp(0.35, 1.0);

                let fill = match role {
                    InterfaceSide::SideA => {
                        let r = (14.0 * shade) as u8;
                        let g = (165.0 * shade) as u8;
                        let b = (233.0 * shade) as u8;
                        Color32::from_rgba_unmultiplied(r, g, b, (surface_alpha * 255.0) as u8)
                    }
                    InterfaceSide::SideB => {
                        let r = (249.0 * shade) as u8;
                        let g = (115.0 * shade) as u8;
                        let b = (22.0 * shade) as u8;
                        Color32::from_rgba_unmultiplied(r, g, b, (surface_alpha * 255.0) as u8)
                    }
                    InterfaceSide::None => {
                        let g = (165.0 * shade) as u8;
                        Color32::from_rgba_unmultiplied(g, g + 5, g + 12, (surface_alpha * 0.45 * 255.0) as u8)
                    }
                };

                draw_triangles.push(DrawTriangle {
                    p0, p1, p2, depth, fill, stroke: Stroke::NONE,
                });
            }
        }
    }

    // 2. Standalone Interface Models
    if interface_mode != InterfaceDisplayMode::Off && interface_alpha > 0.02 {
        match interface_mode {
            InterfaceDisplayMode::SmoothedSdf => {
                let sheet = &ds.geometry.sheet_mesh;
                let proj: Vec<_> = sheet.vertices.iter().map(|&v3d| {
                    let loc = world_to_plane_local(v3d, plane);
                    camera.project_local_point(loc, rect)
                }).collect();

                for &[i0, i1, i2] in &sheet.triangles {
                    let (p0, d0) = proj[i0 as usize];
                    let (p1, d1) = proj[i1 as usize];
                    let (p2, d2) = proj[i2 as usize];
                    draw_triangles.push(DrawTriangle {
                        p0, p1, p2,
                        depth: (d0 + d1 + d2) / 3.0,
                        fill: Color32::from_rgba_unmultiplied(250, 204, 21, (interface_alpha * 200.0) as u8),
                        stroke: Stroke::new(0.5_f32, Color32::from_rgba_unmultiplied(217, 119, 6, 220)),
                    });
                }
            }
            InterfaceDisplayMode::ContactEnvelope => {
                let env = &ds.geometry.envelope_mesh;
                let proj: Vec<_> = env.vertices.iter().enumerate().map(|(v_idx, &v3d)| {
                    let loc = world_to_plane_local(v3d, plane);
                    let (scr, depth) = camera.project_local_point(loc, rect);
                    let g_atom = env.nearest_atom.get(v_idx).copied().unwrap_or_default();
                    (scr, depth, env.normals[v_idx], g_atom)
                }).collect();

                for &[i0, i1, i2] in &env.triangles {
                    let (p0, d0, n0, g0) = proj[i0 as usize];
                    let (p1, d1, n1, _) = proj[i1 as usize];
                    let (p2, d2, n2, _) = proj[i2 as usize];
                    let depth = (d0 + d1 + d2) / 3.0;

                    let nx = (n0[0] + n1[0] + n2[0]) / 3.0;
                    let ny = (n0[1] + n1[1] + n2[1]) / 3.0;
                    let nz = (n0[2] + n1[2] + n2[2]) / 3.0;
                    let shade = (0.65 + 0.35 * (nx * 0.3 + ny * 0.5 + nz * 0.8)).clamp(0.35, 1.0);

                    // Check which side this atom belongs to
                    let is_side_a = ds.map.nodes.iter().any(|n| n.global_atom == g0 && n.role == InterfaceSide::SideA);
                    let (fill, stroke) = if is_side_a {
                        let r = (250.0 * shade) as u8;
                        let g = (204.0 * shade) as u8;
                        let b = (21.0 * shade) as u8;
                        (Color32::from_rgba_unmultiplied(r, g, b, (interface_alpha * 255.0) as u8),
                         Stroke::new(0.6_f32, Color32::from_rgba_unmultiplied(180, 83, 9, 160)))
                    } else {
                        let r = (168.0 * shade) as u8;
                        let g = (85.0 * shade) as u8;
                        let b = (247.0 * shade) as u8;
                        (Color32::from_rgba_unmultiplied(r, g, b, (interface_alpha * 255.0) as u8),
                         Stroke::new(0.6_f32, Color32::from_rgba_unmultiplied(107, 33, 168, 160)))
                    };

                    draw_triangles.push(DrawTriangle { p0, p1, p2, depth, fill, stroke });
                }
            }
            InterfaceDisplayMode::NegativeSpace => {
                let neg = &ds.geometry.negative_mesh;
                let proj: Vec<_> = neg.vertices.iter().enumerate().map(|(v_idx, &v3d)| {
                    let loc = world_to_plane_local(v3d, plane);
                    let (scr, depth) = camera.project_local_point(loc, rect);
                    (scr, depth, neg.normals[v_idx])
                }).collect();

                for &[i0, i1, i2] in &neg.triangles {
                    let (p0, d0, n0) = proj[i0 as usize];
                    let (p1, d1, n1) = proj[i1 as usize];
                    let (p2, d2, n2) = proj[i2 as usize];
                    let depth = (d0 + d1 + d2) / 3.0;

                    let nx = (n0[0] + n1[0] + n2[0]) / 3.0;
                    let ny = (n0[1] + n1[1] + n2[1]) / 3.0;
                    let nz = (n0[2] + n1[2] + n2[2]) / 3.0;
                    let shade = (0.65 + 0.35 * (nx * 0.3 + ny * 0.5 + nz * 0.8)).clamp(0.35, 1.0);

                    let r = (6.0 * shade) as u8;
                    let g = (182.0 * shade) as u8;
                    let b = (212.0 * shade) as u8;
                    let fill = Color32::from_rgba_unmultiplied(r, g, b, (interface_alpha * 255.0) as u8);
                    let stroke = Stroke::new(0.5_f32, Color32::from_rgba_unmultiplied(14, 116, 144, 200));

                    draw_triangles.push(DrawTriangle { p0, p1, p2, depth, fill, stroke });
                }
            }
            InterfaceDisplayMode::Off => {}
        }
    }

    // Sort depth ascending (back to front) and draw
    draw_triangles.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap_or(std::cmp::Ordering::Equal));
    for tri in draw_triangles {
        painter.add(Shape::convex_polygon(vec![tri.p0, tri.p1, tri.p2], tri.fill, tri.stroke));
    }

    // 3. Draw Cartoon Backbone traces
    if show_cartoon && cartoon_alpha > 0.02 {
        for ch in &ds.chains {
            if ch.ca_positions.len() < 2 { continue; }
            let role = if ds.group_a_chains.iter().any(|c| c == &ch.chain_id) {
                InterfaceSide::SideA
            } else if ds.group_b_chains.iter().any(|c| c == &ch.chain_id) {
                InterfaceSide::SideB
            } else {
                InterfaceSide::None
            };
            let explode_shift = get_explode_offset_local(role, explode_t);

            let stroke_color = match role {
                InterfaceSide::SideA => Color32::from_rgba_unmultiplied(2, 132, 199, (cartoon_alpha * 255.0) as u8),
                InterfaceSide::SideB => Color32::from_rgba_unmultiplied(234, 88, 12, (cartoon_alpha * 255.0) as u8),
                InterfaceSide::None => Color32::from_rgba_unmultiplied(148, 163, 184, (cartoon_alpha * 0.45 * 255.0) as u8),
            };
            let stroke = Stroke::new((camera.zoom * 0.28).max(3.0), stroke_color);

            let pts: Vec<_> = ch.ca_positions.iter().map(|&pos3d| {
                let loc = glam::Vec3::from(world_to_plane_local(pos3d, plane)) + glam::Vec3::from(explode_shift);
                let (scr, _) = camera.project_local_point(loc.to_array(), rect);
                scr
            }).collect();

            for pair in pts.windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
        }
    }

    // 4. Draw Interfacial Heavy Atom Spheres
    if show_spheres && spheres_alpha > 0.02 {
        for (i, node) in ds.map.nodes.iter().enumerate() {
            let explode_shift = get_explode_offset_local(node.role, explode_t);
            let loc = glam::Vec3::from(world_to_plane_local(node.pos_3d, plane)) + glam::Vec3::from(explode_shift);
            let (scr, _) = camera.project_local_point(loc.to_array(), rect);
            let meta = &ds.atom_meta[i];
            let r = (meta.vdw_radius_a * camera.zoom * 0.35).max(3.5);
            let fill = if node.role == InterfaceSide::SideA {
                Color32::from_rgba_unmultiplied(56, 189, 248, (spheres_alpha * 255.0) as u8)
            } else {
                Color32::from_rgba_unmultiplied(251, 146, 60, (spheres_alpha * 255.0) as u8)
            };
            painter.circle(scr, r, fill, Stroke::new(1.0_f32, Color32::from_rgb(15, 23, 42)));
        }
    }
}
