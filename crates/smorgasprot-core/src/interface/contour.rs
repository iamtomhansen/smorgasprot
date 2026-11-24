//! Residue territory 2D/3D contour generation: ΔSASA-scaled smooth-union signed distance fields
//! (preserving concave C/L/Y-shaped residues), radius-weighted Voronoi clipping, Marching Squares
//! contour extraction, Chaikin smoothing, and 3D relief draping.
//!
//! ## Works referenced
//! 1. **Marching Squares Iso-Contour Extraction:**
//!    Lorensen, W. E., & Cline, H. E. (1987). Marching cubes: A high resolution 3D surface
//!    construction algorithm (2D analog). *ACM SIGGRAPH Computer Graphics*, 21(4), 163–169.
//! 2. **Polynomial Smooth Minimum (`smin`) for Signed Distance Fields:**
//!    Quilez, I. (2013). Smooth minimum functions for implicit surface blending.
//! 3. **Weighted Voronoi / Power Diagrams:**
//!    Aurenhammer, F. (1987). Power diagrams: properties, algorithms and applications.
//!    *SIAM Journal on Computing*, 16(1), 78–96.
//! 4. **Corner-Cutting Curve Smoothing:**
//!    Chaikin, G. M. (1974). An algorithm for high-speed curve generation.
//!    *Computer Graphics and Image Processing*, 3(4), 346–349.

use super::model::{AtomNode, ProjectionPlane, ResidueTerritory};
use glam::{Vec2, Vec3};

crate::param_group! {
    /// Padded, Voronoi-clipped residue territory contours on the 2D map.
    pub struct ContourParams("contour", "Residue Territories") {
        /// Smallest padding disc around an atom (Å).
        min_padding_radius_a: f32 = 0.5, 0.0..=3.0, "Min atom padding radius", "Å";
        /// Largest padding disc around an atom (Å).
        max_padding_radius_a: f32 = 2.15, 0.5..=5.0, "Max atom padding radius", "Å";
        /// Padding growth per square root of buried area.
        sasa_padding_scale: f32 = 0.22, 0.0..=1.0, "ΔSASA padding growth scale", "";
        /// Smooth-minimum blend radius joining atom discs (Å).
        smin_blend_a: f32 = 0.65, 0.0..=3.0, "Smooth-minimum blend radius", "Å";
        /// Radius of capsules bridging bonded atoms (Å).
        bridge_capsule_radius_a: f32 = 0.58, 0.0..=2.0, "Bonded bridge capsule radius", "Å";
        /// Gap left between neighboring territories (Å).
        voronoi_gap_a: f32 = 0.05, 0.0..=1.0, "Inter-territory Voronoi gap", "Å";
        /// Resolution of the per-residue SDF grid (cells per side).
        sdf_grid_res: usize = 36, 8..=128, "Per-residue SDF grid resolution", "";
        /// Gaussian width for draping contours onto 3D atoms (Å).
        relief_drape_sigma_a: f32 = 1.85, 0.1..=5.0, "3D relief drape Gaussian width", "Å";
    }
}

/// Computes an atom's 2D territory splat radius from its buried surface area (`delta_SASA_i`):
/// r_text i = clamp(r_min + scale * squrt(delta SASA_i), r_min, r_max)
#[inline]
pub fn atom_padding_radius_a(buried_sasa_a2: f32, p: &ContourParams) -> f32 {
    (p.min_padding_radius_a + p.sasa_padding_scale * buried_sasa_a2.max(0.0).sqrt())
        .clamp(p.min_padding_radius_a, p.max_padding_radius_a)
}

/// Polynomial C^1 smooth minimum of two signed distances `a` and `b` with blend radius `k`.
#[inline]
fn smin_poly(a: f32, b: f32, k: f32) -> f32 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// Signed distance from point `p` to a 2D line-segment capsule `(a, b)` of radius `r`.
#[inline]
fn sd_capsule(p: Vec2, a: Vec2, b: Vec2, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = (pa.dot(ba) / ba.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (pa - ba * h).length() - r
}

/// Computes concave, smooth, non-overlapping 2D contours (`contour_uv`), 3D surface-draped
/// relief contours (`contour_3d`), and pole-of-inaccessibility label centers (`label_center_uv`,
/// `label_center_3d`) for all residue territories in an `InterfaceMap`.
pub fn compute_territory_contours(
    territories: &mut [ResidueTerritory],
    nodes: &[AtomNode],
    plane: &ProjectionPlane,
    params: &ContourParams,
) {
    let n_terr = territories.len();
    let res = params.sdf_grid_res;
    let origin = Vec3::from(plane.origin);
    let axis_u = Vec3::from(plane.axis_u);
    let axis_v = Vec3::from(plane.axis_v);

    for t_idx in 0..n_terr {
        let role = territories[t_idx].side;
        // Collect (pos_uv, pad_radius, delta_3d) for each atom in this territory, where
        // delta_3d = pos_3d - (origin + u * axis_u + v * axis_v) captures both the atom's
        // 3D normal elevation (z-height) and its lateral relaxation displacement.
        let my_atoms: Vec<(Vec2, f32, Vec3)> = territories[t_idx]
            .patch_atoms
            .iter()
            .map(|&a_idx| {
                let n = &nodes[a_idx.0 as usize];
                let uv = Vec2::from(n.pos_uv);
                let flat_3d = origin + uv.x * axis_u + uv.y * axis_v;
                let delta_3d = Vec3::from(n.pos_3d) - flat_3d;
                (uv, atom_padding_radius_a(n.buried_sasa_a2, params), delta_3d)
            })
            .collect();

        if my_atoms.is_empty() {
            continue;
        }

        let centroid =
            my_atoms.iter().map(|&(p, _, _)| p).sum::<Vec2>() / (my_atoms.len() as f32);

        // Build a minimal spanning tree of intra-residue bridges so all atoms of the residue
        // stay topologically connected in a single contour without filling in concave C-bays.
        let bridges = build_residue_spanning_bridges(&my_atoms);

        // Collect nearby same-side atoms belonging to OTHER residue territories for weighted Voronoi clipping
        let other_atoms: Vec<(Vec2, f32)> = nodes
            .iter()
            .filter(|n| n.side == role && n.territory.0 as usize != t_idx)
            .map(|n| (Vec2::from(n.pos_uv), atom_padding_radius_a(n.buried_sasa_a2, params)))
            .filter(|&(p, _)| p.distance_squared(centroid) <= 169.0)
            .collect();

        // Determine padded bounding box for the local Marching Squares SDF grid
        let mut min_bb = Vec2::splat(f32::INFINITY);
        let mut max_bb = Vec2::splat(f32::NEG_INFINITY);
        for &(p, r, _) in &my_atoms {
            let reach = r + params.smin_blend_a + 0.45;
            min_bb = min_bb.min(p - Vec2::splat(reach));
            max_bb = max_bb.max(p + Vec2::splat(reach));
        }

        let step = (max_bb - min_bb) / ((res - 1) as f32);
        let mut sdf = vec![0.0_f32; res * res];

        // Track the grid point with the deepest negative SDF value (Pole of Inaccessibility)
        let mut best_poi_uv = centroid;
        let mut min_sdf_val = f32::INFINITY;

        for iy in 0..res {
            for ix in 0..res {
                let p = Vec2::new(
                    min_bb.x + (ix as f32) * step.x,
                    min_bb.y + (iy as f32) * step.y,
                );
                let val = evaluate_territory_sdf(p, &my_atoms, &bridges, &other_atoms, params);
                sdf[iy * res + ix] = val;
                if val < min_sdf_val {
                    min_sdf_val = val;
                    best_poi_uv = p;
                }
            }
        }

        // Extract the closed counter-clockwise zero-contour loop via Marching Squares
        let raw_loop = extract_marching_squares_loop(&sdf, res, min_bb, step);

        // Apply 2 passes of Chaikin corner-cutting for an organic contour
        let smooth1 = chaikin_smooth(&raw_loop);
        let smooth2 = chaikin_smooth(&smooth1);

        // Drape every 2D contour vertex and the Pole of Inaccessibility onto the 3D atomic relief
        let contour_3d: Vec<[f32; 3]> = smooth2
            .iter()
            .map(|&uv| drape_uv_to_3d(uv, &my_atoms, origin, axis_u, axis_v, params).to_array())
            .collect();
        let label_center_3d =
            drape_uv_to_3d(best_poi_uv, &my_atoms, origin, axis_u, axis_v, params).to_array();

        // Triangulated once on computation.
        territories[t_idx].contour_triangles = triangulate_polygon(&smooth2);
        territories[t_idx].contour_uv = smooth2.into_iter().map(|v| v.to_array()).collect();
        territories[t_idx].contour_3d = contour_3d;
        territories[t_idx].label_center_uv = best_poi_uv.to_array();
        territories[t_idx].label_center_3d = label_center_3d;
        territories[t_idx].local_uv_anchor = centroid.to_array();
    }
}

/// Evaluates the Voronoi-clipped concave smooth-union signed distance phi(x) at 2D point `p`.
/// Negative inside the territory, zero on the boundary, positive outside.
fn evaluate_territory_sdf(
    p: Vec2,
    my_atoms: &[(Vec2, f32, Vec3)],
    bridges: &[(Vec2, Vec2)],
    other_atoms: &[(Vec2, f32)],
    params: &ContourParams,
) -> f32 {
    // 1. Smooth union of our residue's ΔSASA-scaled atom circles + spanning-tree bridges.
    let mut d_my = f32::INFINITY;
    let mut best_atom_pos = my_atoms[0].0;
    let mut best_atom_rad = my_atoms[0].1;
    let mut best_raw_sd = f32::INFINITY;

    for &(c, r, _) in my_atoms {
        let sd = p.distance(c) - r;
        if sd < best_raw_sd {
            best_raw_sd = sd;
            best_atom_pos = c;
            best_atom_rad = r;
        }
        d_my = if d_my.is_infinite() {
            sd
        } else {
            smin_poly(d_my, sd, params.smin_blend_a)
        };
    }

    for &(a, b) in bridges {
        let sd_cap = sd_capsule(p, a, b, params.bridge_capsule_radius_a);
        d_my = smin_poly(d_my, sd_cap, params.smin_blend_a * 0.7);
    }

    // Fast exit if already well outside our own smooth union.
    if d_my > 0.5 {
        return d_my;
    }

    // 2. Radius-weighted Voronoi half-plane clipping against neighboring same-side residues.
    let mut voronoi_clip = f32::NEG_INFINITY;
    for &(other_pos, other_rad) in other_atoms {
        let dir = other_pos - best_atom_pos;
        let dist = dir.length();
        if dist < 1e-4 {
            continue;
        }
        let unit_dir = dir / dist;
        let weight_fraction = best_atom_rad / (best_atom_rad + other_rad).max(1e-4);
        let max_proj = weight_fraction * dist - params.voronoi_gap_a;
        let cur_proj = (p - best_atom_pos).dot(unit_dir);
        let clip_sd = cur_proj - max_proj;
        if clip_sd > voronoi_clip {
            voronoi_clip = clip_sd;
        }
    }

    d_my.max(voronoi_clip)
}

/// Builds a Prim's minimal spanning tree of line segments connecting the atoms of a single residue
/// so that multi-atom territories remain a single contiguous shape without bridging across concave C-bays.
fn build_residue_spanning_bridges(atoms: &[(Vec2, f32, Vec3)]) -> Vec<(Vec2, Vec2)> {
    let n = atoms.len();
    if n <= 1 {
        return Vec::new();
    }
    let mut in_tree = vec![false; n];
    in_tree[0] = true;
    let mut bridges = Vec::with_capacity(n - 1);

    for _ in 0..(n - 1) {
        let mut best_pair = (0, 0);
        let mut best_d2 = f32::INFINITY;
        for i in 0..n {
            if !in_tree[i] {
                continue;
            }
            for j in 0..n {
                if in_tree[j] {
                    continue;
                }
                let d2 = atoms[i].0.distance_squared(atoms[j].0);
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_pair = (i, j);
                }
            }
        }
        in_tree[best_pair.1] = true;
        bridges.push((atoms[best_pair.0].0, atoms[best_pair.1].0));
    }

    bridges
}

/// Drapes a 2D projection-plane point `uv` onto the residue's 3D atomic relief at `flatten_t = 0.0`
/// using Gaussian-kernel interpolation of each atom's 3D displacement vector.
fn drape_uv_to_3d(
    uv: Vec2,
    my_atoms: &[(Vec2, f32, Vec3)],
    origin: Vec3,
    axis_u: Vec3,
    axis_v: Vec3,
    p: &ContourParams,
) -> Vec3 {
    let base_flat = origin + uv.x * axis_u + uv.y * axis_v;
    let inv_two_sigma_sq = 1.0 / (2.0 * p.relief_drape_sigma_a * p.relief_drape_sigma_a);

    let mut w_sum = 0.0_f32;
    let mut delta_sum = Vec3::ZERO;
    for &(atom_uv, _, delta_3d) in my_atoms {
        let d2 = uv.distance_squared(atom_uv);
        let w = (-d2 * inv_two_sigma_sq).exp().max(1e-5);
        w_sum += w;
        delta_sum += w * delta_3d;
    }

    if w_sum > 0.0 {
        base_flat + (delta_sum / w_sum)
    } else {
        base_flat
    }
}

/// Extracts the primary closed counter-clockwise zero-contour polygon from a `res x res`
/// scalar field using Marching Squares with linear edge interpolation.
fn extract_marching_squares_loop(sdf: &[f32], res: usize, min_bb: Vec2, step: Vec2) -> Vec<Vec2> {
    // Directed segments (from_edge_id, to_edge_id, to_pos)
    // Edge ID encoding (`vert` = res * res keeps the two ranges disjoint):
    //   Horizontal edge (ix, iy) -> (ix+1, iy):   id = iy * res + ix
    //   Vertical edge   (ix, iy) -> (ix, iy+1):   id = vert + iy * res + ix
    let vert = res * res;
    let mut directed_edges: Vec<(usize, usize, Vec2, Vec2)> = Vec::new();

    let interp = |p0: Vec2, p1: Vec2, v0: f32, v1: f32| -> Vec2 {
        let denom = v0 - v1;
        let t = if denom.abs() > 1e-6 {
            (v0 / denom).clamp(0.0, 1.0)
        } else {
            0.5
        };
        p0.lerp(p1, t)
    };

    for iy in 0..(res - 1) {
        for ix in 0..(res - 1) {
            let v00 = sdf[iy * res + ix];
            let v10 = sdf[iy * res + (ix + 1)];
            let v11 = sdf[(iy + 1) * res + (ix + 1)];
            let v01 = sdf[(iy + 1) * res + ix];

            // Bitmask: bit 1 = inside (sdf < 0.0)
            let mut mask = 0u8;
            if v00 < 0.0 {
                mask |= 1;
            }
            if v10 < 0.0 {
                mask |= 2;
            }
            if v11 < 0.0 {
                mask |= 4;
            }
            if v01 < 0.0 {
                mask |= 8;
            }
            if mask == 0 || mask == 15 {
                continue;
            }

            let p00 = Vec2::new(min_bb.x + (ix as f32) * step.x, min_bb.y + (iy as f32) * step.y);
            let p10 = Vec2::new(p00.x + step.x, p00.y);
            let p11 = Vec2::new(p00.x + step.x, p00.y + step.y);
            let p01 = Vec2::new(p00.x, p00.y + step.y);

            // 4 cell edges: Bottom (B), Right (R), Top (T), Left (L)
            let id_b = iy * res + ix;
            let id_r = vert + iy * res + (ix + 1);
            let id_t = (iy + 1) * res + ix;
            let id_l = vert + iy * res + ix;

            let pt_b = interp(p00, p10, v00, v10);
            let pt_r = interp(p10, p11, v10, v11);
            let pt_t = interp(p01, p11, v01, v11);
            let pt_l = interp(p00, p01, v00, v01);

            // Emit directed segments keeping inside (sdf < 0) on the left (counter-clockwise loop)
            match mask {
                1 => directed_edges.push((id_l, id_b, pt_l, pt_b)),
                2 => directed_edges.push((id_b, id_r, pt_b, pt_r)),
                3 => directed_edges.push((id_l, id_r, pt_l, pt_r)),
                4 => directed_edges.push((id_r, id_t, pt_r, pt_t)),
                5 => {
                    // Saddle case: connect keeping interior connected
                    directed_edges.push((id_l, id_t, pt_l, pt_t));
                    directed_edges.push((id_r, id_b, pt_r, pt_b));
                }
                6 => directed_edges.push((id_b, id_t, pt_b, pt_t)),
                7 => directed_edges.push((id_l, id_t, pt_l, pt_t)),
                8 => directed_edges.push((id_t, id_l, pt_t, pt_l)),
                9 => directed_edges.push((id_t, id_b, pt_t, pt_b)),
                10 => {
                    directed_edges.push((id_b, id_l, pt_b, pt_l));
                    directed_edges.push((id_t, id_r, pt_t, pt_r));
                }
                11 => directed_edges.push((id_t, id_r, pt_t, pt_r)),
                12 => directed_edges.push((id_r, id_l, pt_r, pt_l)),
                13 => directed_edges.push((id_r, id_b, pt_r, pt_b)),
                14 => directed_edges.push((id_b, id_l, pt_b, pt_l)),
                _ => {}
            }
        }
    }

    if directed_edges.is_empty() {
        return Vec::new();
    }

    // Stitch directed edges into closed loops and return the longest loop
    let mut used = vec![false; directed_edges.len()];
    let mut best_loop: Vec<Vec2> = Vec::new();

    for start_idx in 0..directed_edges.len() {
        if used[start_idx] {
            continue;
        }
        used[start_idx] = true;
        let mut poly = vec![directed_edges[start_idx].2];
        let start_node = directed_edges[start_idx].0;
        let mut cur_node = directed_edges[start_idx].1;

        while cur_node != start_node {
            let mut found_next = None;
            for (idx, &(u, v, _, pt_v)) in directed_edges.iter().enumerate() {
                if !used[idx] && u == cur_node {
                    used[idx] = true;
                    poly.push(pt_v);
                    found_next = Some(v);
                    break;
                }
            }
            match found_next {
                Some(next_node) => cur_node = next_node,
                None => break,
            }
        }

        if poly.len() > best_loop.len() {
            best_loop = poly;
        }
    }

    best_loop
}

/// Applies Chaikin's corner-cutting algorithm to smooth a closed polygon `poly`.
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

/// Ear-clipping triangulation of a simple (non-self-intersecting) polygon, convex or concave.
fn triangulate_polygon(points: &[Vec2]) -> Vec<[u32; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }

    // Area's sign gives the polygon's winding; ear-clipping needs it to tell convex corners from reflex ones.
    let signed_area_x2: f32 =
        (0..n).map(|i| { let (a, b) = (points[i], points[(i + 1) % n]); a.x * b.y - b.x * a.y }).sum();
    let ccw = signed_area_x2 > 0.0;
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let is_convex_corner = |prev: Vec2, cur: Vec2, next: Vec2| {
        let c = cross(prev, cur, next);
        if ccw { c > 0.0 } else { c < 0.0 }
    };
    let point_in_triangle = |p: Vec2, a: Vec2, b: Vec2, c: Vec2| {
        let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
        let (has_neg, has_pos) = (d1 < 0.0 || d2 < 0.0 || d3 < 0.0, d1 > 0.0 || d2 > 0.0 || d3 > 0.0);
        !(has_neg && has_pos)
    };

    let mut indices: Vec<u32> = (0..n as u32).collect();
    let mut triangles = Vec::with_capacity(n.saturating_sub(2));
    // Each successful clip removes one vertex; bail rather than loop forever on degenerate input.
    let mut remaining_attempts = n * n;
    while indices.len() > 3 && remaining_attempts > 0 {
        remaining_attempts -= 1;
        let m = indices.len();
        let mut clipped = false;
        for i in 0..m {
            let (ip, ic, inx) = (indices[(i + m - 1) % m], indices[i], indices[(i + 1) % m]);
            let (a, b, c) = (points[ip as usize], points[ic as usize], points[inx as usize]);
            if !is_convex_corner(a, b, c) {
                continue;
            }
            let is_ear = !indices
                .iter()
                .any(|&k| k != ip && k != ic && k != inx && point_in_triangle(points[k as usize], a, b, c));
            if is_ear {
                triangles.push([ip, ic, inx]);
                indices.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break;
        }
    }
    if indices.len() == 3 {
        triangles.push([indices[0], indices[1], indices[2]]);
    }
    triangles
}
