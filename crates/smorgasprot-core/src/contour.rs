//! Residue territory contour generation (padded hulls + Voronoi bisector clipping + Chaikin smoothing).

use crate::model::{AtomNode, ResidueTerritory};
use glam::Vec2;

const PADDING_RADIUS_A: f32 = 1.35;
const CIRCLE_SAMPLES: usize = 12;
const VORONOI_GAP_A: f32 = 0.18;

/// Computes smooth, non-overlapping 2D contours (`contour_uv`) and label centers (`label_center_uv`)
/// for all residue territories in an `InterfaceMap`.
pub fn compute_territory_contours(
    territories: &mut [ResidueTerritory],
    nodes: &[AtomNode],
) {
    let n_terr = territories.len();

    for t_idx in 0..n_terr {
        let role = territories[t_idx].role;
        let my_atoms: Vec<Vec2> = territories[t_idx]
            .patch_atoms
            .iter()
            .map(|&a_idx| Vec2::from(nodes[a_idx.0 as usize].pos_uv))
            .collect();

        if my_atoms.is_empty() {
            continue;
        }

        // Collect nearby same-partner atoms belonging to OTHER residue territories for Voronoi clipping
        let centroid = my_atoms.iter().copied().sum::<Vec2>() / (my_atoms.len() as f32);
        let other_atoms: Vec<Vec2> = nodes
            .iter()
            .filter(|n| n.role == role && n.territory.0 as usize != t_idx)
            .map(|n| Vec2::from(n.pos_uv))
            .filter(|&p| p.distance_squared(centroid) <= 144.0) // within 12 A of centroid
            .collect();

        // 1. Generate padded circle points around each atom in this territory
        let mut sample_pts = Vec::with_capacity(my_atoms.len() * CIRCLE_SAMPLES);
        for &center in &my_atoms {
            for k in 0..CIRCLE_SAMPLES {
                let angle = (k as f32) * std::f32::consts::TAU / (CIRCLE_SAMPLES as f32);
                sample_pts.push(center + PADDING_RADIUS_A * Vec2::new(angle.cos(), angle.sin()));
            }
        }

        // 2. Convex hull of the padded sample points
        let hull = convex_hull_2d(&mut sample_pts);

        // 3. Subdivide hull edges so Voronoi clipping can curve cleanly around neighboring atoms
        let mut poly = subdivide_polygon(&hull, 0.75);

        // 4. Voronoi bisector clipping against neighboring same-partner territories
        for v in &mut poly {
            // Find closest atom in our own territory
            let mut best_my_atom = my_atoms[0];
            let mut best_my_d2 = v.distance_squared(best_my_atom);
            for &a in &my_atoms[1..] {
                let d2 = v.distance_squared(a);
                if d2 < best_my_d2 {
                    best_my_d2 = d2;
                    best_my_atom = a;
                }
            }

            // Clip against any foreign atom that is closer than our closest atom
            for &other in &other_atoms {
                let dir = other - best_my_atom;
                let dist = dir.length();
                if dist < 1e-4 {
                    continue;
                }
                let unit_dir = dir / dist;
                let max_proj = 0.5 * dist - VORONOI_GAP_A;
                let cur_proj = (*v - best_my_atom).dot(unit_dir);
                if cur_proj > max_proj {
                    *v -= (cur_proj - max_proj) * unit_dir;
                }
            }
        }

        // 5. Apply 2 passes of Chaikin corner-cutting for an organic contour
        let smooth1 = chaikin_smooth(&poly);
        let smooth2 = chaikin_smooth(&smooth1);

        // 6. Visual label center (centroid of polygon vertices, anchored near interior)
        let label_center = if smooth2.is_empty() {
            centroid
        } else {
            smooth2.iter().copied().sum::<Vec2>() / (smooth2.len() as f32)
        };

        territories[t_idx].contour_uv = smooth2.into_iter().map(|v| v.to_array()).collect();
        territories[t_idx].label_center_uv = label_center.to_array();
        territories[t_idx].local_uv_anchor = centroid.to_array();
    }
}

/// Andrew's monotone chain 2D convex hull in counter-clockwise order.
fn convex_hull_2d(pts: &mut [Vec2]) -> Vec<Vec2> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    pts.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut lower: Vec<Vec2> = Vec::with_capacity(pts.len());
    for &p in pts.iter() {
        while lower.len() >= 2
            && cross_2d(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 1e-6
        {
            lower.pop();
        }
        lower.push(p);
    }

    let mut upper: Vec<Vec2> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2
            && cross_2d(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 1e-6
        {
            upper.pop();
        }
        upper.push(p);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

#[inline]
fn cross_2d(o: Vec2, a: Vec2, b: Vec2) -> f32 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

fn subdivide_polygon(poly: &[Vec2], max_seg_len: f32) -> Vec<Vec2> {
    let n = poly.len();
    if n < 2 {
        return poly.to_vec();
    }
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let dist = a.distance(b);
        let steps = ((dist / max_seg_len).ceil() as usize).max(1);
        for s in 0..steps {
            let t = (s as f32) / (steps as f32);
            out.push(a.lerp(b, t));
        }
    }
    out
}

fn chaikin_smooth(poly: &[Vec2]) -> Vec<Vec2> {
    let n = poly.len();
    if n < 3 {
        return poly.to_vec();
    }
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let p0 = poly[i];
        let p1 = poly[(i + 1) % n];
        out.push(0.75 * p0 + 0.25 * p1);
        out.push(0.25 * p0 + 0.75 * p1);
    }
    out
}
