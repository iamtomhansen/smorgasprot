//! Depth-sorted 3D primitives tessellated into a single egui mesh each frame.
//! A CPU stand-in for a GPU depth buffer; swapping in a wgpu/glow paint callback later only touches this file.

use egui::epaint::Mesh;
use egui::{Color32, Painter, Pos2, Shape, Stroke};

/// One screen-space primitive awaiting depth sorting.
enum Prim {
    Triangle { pts: [Pos2; 3], fill: Color32, edge: Stroke },
    Segment { a: Pos2, b: Pos2, stroke: Stroke },
    Disc { center: Pos2, radius: f32, fill: Color32, outline: Stroke },
}

/// Collects primitives with a depth key, then paints them back-to-front (painter's algorithm).
#[derive(Default)]
pub struct DrawList {
    items: Vec<(f32, Prim)>,
}

impl DrawList {
    /// Filled triangle with an optional edge stroke (`Stroke::NONE` for none).
    pub fn triangle(&mut self, pts: [Pos2; 3], depth: f32, fill: Color32, edge: Stroke) {
        self.items.push((depth, Prim::Triangle { pts, fill, edge }));
    }

    /// Thick line segment without caps.
    pub fn segment(&mut self, a: Pos2, b: Pos2, depth: f32, stroke: Stroke) {
        self.items.push((depth, Prim::Segment { a, b, stroke }));
    }

    /// Screen-aligned filled circle with an outline ring.
    pub fn disc(&mut self, center: Pos2, radius: f32, depth: f32, fill: Color32, outline: Stroke) {
        self.items.push((depth, Prim::Disc { center, radius, fill, outline }));
    }

    /// Sorts far-to-near and submits everything as one mesh.
    pub fn paint(mut self, painter: &Painter) {
        // Larger depth is nearer the viewer, so ascending order draws far geometry first.
        self.items.sort_by(|a, b| a.0.total_cmp(&b.0));

        let mut mesh = Mesh::default();
        for (_, prim) in &self.items {
            match *prim {
                Prim::Triangle { pts, fill, edge } => {
                    add_triangle(&mut mesh, pts, fill);
                    add_segment(&mut mesh, pts[0], pts[1], edge);
                    add_segment(&mut mesh, pts[1], pts[2], edge);
                    add_segment(&mut mesh, pts[2], pts[0], edge);
                }
                Prim::Segment { a, b, stroke } => add_segment(&mut mesh, a, b, stroke),
                Prim::Disc { center, radius, fill, outline } => add_disc(&mut mesh, center, radius, fill, outline),
            }
        }
        if !mesh.is_empty() {
            painter.add(Shape::mesh(mesh));
        }
    }
}

fn add_triangle(mesh: &mut Mesh, pts: [Pos2; 3], color: Color32) {
    let base = mesh.vertices.len() as u32;
    for p in pts {
        mesh.colored_vertex(p, color);
    }
    mesh.add_triangle(base, base + 1, base + 2);
}

/// Adds a segment as a screen-space quad of width `stroke.width`.
fn add_segment(mesh: &mut Mesh, a: Pos2, b: Pos2, stroke: Stroke) {
    let d = b - a;
    let len = d.length();
    if stroke.is_empty() || len < 1e-3 {
        return;
    }
    let n = d.rot90() * (0.5 * stroke.width / len);
    let base = mesh.vertices.len() as u32;
    for p in [a + n, b + n, b - n, a - n] {
        mesh.colored_vertex(p, stroke.color);
    }
    mesh.add_triangle(base, base + 1, base + 2);
    mesh.add_triangle(base, base + 2, base + 3);
}

/// Adds a triangle-fan disc plus an outline ring centered on `radius`.
fn add_disc(mesh: &mut Mesh, center: Pos2, radius: f32, fill: Color32, outline: Stroke) {
    // More segments for larger circles, within a sane range.
    let n = ((radius * 0.75) as u32).clamp(10, 32);
    let ring = |r: f32, i: u32| {
        let a = std::f32::consts::TAU * i as f32 / n as f32;
        center + egui::vec2(a.cos(), a.sin()) * r
    };

    // Fill: center vertex + n rim vertices.
    let c = mesh.vertices.len() as u32;
    mesh.colored_vertex(center, fill);
    for i in 0..n {
        mesh.colored_vertex(ring(radius, i), fill);
    }
    for i in 0..n {
        mesh.add_triangle(c, c + 1 + i, c + 1 + (i + 1) % n);
    }

    // Outline: quad strip between inner and outer radius.
    if outline.is_empty() {
        return;
    }
    let (r_in, r_out) = ((radius - 0.5 * outline.width).max(0.0), radius + 0.5 * outline.width);
    let o = mesh.vertices.len() as u32;
    for i in 0..n {
        mesh.colored_vertex(ring(r_in, i), outline.color);
        mesh.colored_vertex(ring(r_out, i), outline.color);
    }
    for i in 0..n {
        let (i0, o0, i1, o1) = (o + 2 * i, o + 2 * i + 1, o + 2 * ((i + 1) % n), o + 2 * ((i + 1) % n) + 1);
        mesh.add_triangle(i0, o0, o1);
        mesh.add_triangle(i0, o1, i1);
    }
}
