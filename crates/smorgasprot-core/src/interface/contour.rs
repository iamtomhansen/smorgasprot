//! Residue territory 2D/3D contour generation.
//! 
//! ## Summary
//! This is just a visualization layer tool. It draws borders around the 2D projections of residues
//! and/or molecules in the interface map. [`compute_territory_contours`] is the main entry point,
//! and it computes the model's [`ResidueTerritory`] s.
//! 
//! ## Considerations
//! The size of the contour is scaled by the buried surface area of each atom, giving a visual
//! sense of the relative contribution of each atom and residue to the interface. I considered
//! allowing other data to be embedded in the contour size channel, but found that these alternate
//! views were less useful at best, misleading at worst. The buried surface area is an intuitive
//! choice.
//! 
//! To make nice shapes, various smooth-union and smoothing techniques are used. The circle radii
//! are scaled by the buried surface area, and then a concave union is drawn, ensuring that
//! residues are contiguous. The corners are then smoothed with Chaikin's corner-cutting algorithm.

use super::definition::InterfaceSide;
use super::model::{AtomNode, ProjectionPlane, ResidueTerritory};
use crate::model::SurfaceMesh;
use glam::{Vec2, Vec3};

crate::param_group! {
    /// Padded, Voronoi-clipped residue territory contours on the 2D map.
    pub struct ContourParams("contour", "Residue Territories") {
        /// Blend between shrinkwrap (`0.0`) and true buried-SASA circle area `π r² = ΔSASA` (`1.0`).
        #[desc = "How accurately the padding radius reflects the buried surface area of each atom. 1 = true ΔSASA, but is often less readable."]
        sasa_padding_scale: f32 = 0.75, 0.0..=1.0, "ΔSASA area scaling (0=shrinkwrap, 1=SASA)", "";
        /// Smallest padding disc around an atom (Å).
        #[desc = "Minimum radius to draw around each atom to indicate its territory."]
        min_padding_radius_a: f32 = 0.1, 0.0..=3.0, "Min atom padding radius", "Å";
        /// Largest padding disc around an atom (Å).
        #[desc = "Maximum radius to draw around each atom to indicate its territory."]
        max_padding_radius_a: f32 = 10.0, 0.0..=10.0, "Max atom padding radius", "Å";
        /// Smooth-minimum blend radius joining atom discs (Å).
        #[desc = "Smooth-minimum blend radius for atom discs (0 = sharp corners, 1 = blobby contours)"]
        smin_blend_a: f32 = 0.5, 0.0..=3.0, "Smoothing radius", "Å";
        /// Radius of capsules bridging bonded atoms (Å).
        #[desc = "Adds bridges of this width to keep multi-atom surface patches contiguous."]
        bridge_capsule_radius_a: f32 = 0.2, 0.0..=2.0, "Territory bridge width", "Å";
        /// Inter-territory Voronoi gap (Å).
        #[desc = "Distance to enforce between neighbouring territories."]
        voronoi_gap_a: f32 = 0.1, 0.0..=1.0, "Inter-territory gap", "Å";
        /// Resolution of the per-residue SDF grid (cells per side).
        #[desc = "Resolution of the per-residue SDF grid (cells per side)."]
        sdf_grid_res: usize = 36, 8..=128, "Per-residue SDF grid resolution", "";
        /// Chaikin corner-cutting passes on each territory outline (0 = low-poly, 2 = ultra-smooth).
        chaikin_passes: usize = 1, 0..=3, "Contour smoothing passes", "";
        /// Spacing of interior Steiner vertices inside each territory mesh (Å; 0 = boundary-only ear clipping).
        interior_mesh_step_a: f32 = 1.5, 0.0..=5.0, "Interior mesh spacing", "Å";
        /// Gaussian width for draping contours onto 3D atoms (Å).
        relief_drape_sigma_a: f32 = 1.85, 0.1..=5.0, "3D relief drape Gaussian width", "Å";
    }
}

/// An atom's 2D territory disc radius from its buried surface area and `sasa_padding_scale`.
/// sasa_padding_scale of 0 = shrinkwrap, 1 = draw true ΔSASA.
#[inline]
pub fn atom_padding_radius_a(buried_sasa_a2: f32, p: &ContourParams) -> f32 {
    let r_sasa = (buried_sasa_a2.max(0.0) / std::f32::consts::PI).sqrt();
    let r_target = r_sasa.max(p.min_padding_radius_a);
    let r = p.min_padding_radius_a + p.sasa_padding_scale.clamp(0.0, 1.0) * (r_target - p.min_padding_radius_a);
    r.clamp(p.min_padding_radius_a, p.max_padding_radius_a)
}

/// Polynomial C^1 smooth minimum of two signed distances `a` and `b` with blend radius `k`.
#[inline]
fn smin_poly(a: f32, b: f32, k: f32) -> f32 {
    if k <= 1e-5 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// Polynomial C^1 smooth maximum of two signed distances `a` and `b` with blend radius `k`.
#[inline]
fn smax_poly(a: f32, b: f32, k: f32) -> f32 {
    -smin_poly(-a, -b, k)
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
    sheet: Option<(&SurfaceMesh, &[[f32; 2]])>,
    params: &ContourParams,
) {
    let n_terr = territories.len();
    let res = params.sdf_grid_res;
    let origin = Vec3::from(plane.origin);
    let axis_u = Vec3::from(plane.axis_u);
    let axis_v = Vec3::from(plane.axis_v);
    let plane_normal = Vec3::from(plane.normal);

    for t_idx in 0..n_terr {
        let role = territories[t_idx].side;
        // Collect (pos_uv, pad_radius, delta_3d) for each atom in this territory, where
        // delta_3d = pos_3d - (origin + u * axis_u + v * axis_v) captures both the atom's
        // 3D normal elevation (z-height) and its lateral relaxation displacement.
        let my_atoms: Vec<(Vec2, f32, Vec3)> = territories[t_idx]
            .member_atoms
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

        // Apply configurable Chaikin corner-cutting passes (0 = low-poly, 2 = ultra-smooth).
        let mut boundary = raw_loop;
        for _ in 0..params.chaikin_passes.min(3) {
            boundary = chaikin_smooth(&boundary);
        }

        // Build interior Steiner mesh vertices (boundary 0..B + interior grid points strictly inside).
        let mut mesh_uv = boundary.clone();
        if params.interior_mesh_step_a > 0.05 && boundary.len() >= 3 {
            let h = params.interior_mesh_step_a;
            let margin = -0.25 * h;
            let mut y = min_bb.y + 0.5 * h;
            while y < max_bb.y {
                let mut x = min_bb.x + 0.5 * h;
                while x < max_bb.x {
                    let pt = Vec2::new(x, y);
                    if evaluate_territory_sdf(pt, &my_atoms, &bridges, &other_atoms, params) < margin
                        && point_in_polygon(pt, &boundary)
                    {
                        mesh_uv.push(pt);
                    }
                    x += h;
                }
                y += h;
            }
        }

        let triangles = if mesh_uv.len() > boundary.len() {
            let delaunay = triangulate_delaunay_interior(&boundary, &mesh_uv);
            if delaunay.is_empty() {
                mesh_uv.truncate(boundary.len());
                triangulate_polygon(&boundary)
            } else {
                delaunay
            }
        } else {
            triangulate_polygon(&boundary)
        };

        // Drape both to atomic centers (blend = 0.0) and onto the curved uv_mesh sheet (blend = 1.0).
        let drape_atoms = |uv: Vec2| -> [f32; 3] {
            drape_uv_to_3d(uv, &my_atoms, origin, axis_u, axis_v, params).to_array()
        };
        let drape_sheet = |uv: Vec2| -> [f32; 3] {
            if let Some((sheet_mesh, sheet_uv)) = sheet {
                drape_uv_onto_sheet(uv, sheet_mesh, sheet_uv, role, plane_normal).to_array()
            } else {
                drape_atoms(uv)
            }
        };

        let n_boundary = boundary.len();
        let mesh_3d_atoms: Vec<[f32; 3]> = mesh_uv.iter().map(|&uv| drape_atoms(uv)).collect();
        let mesh_3d_sheet: Vec<[f32; 3]> = mesh_uv.iter().map(|&uv| drape_sheet(uv)).collect();

        territories[t_idx].contour_triangles = triangles;
        territories[t_idx].contour_uv = boundary.into_iter().map(|v| v.to_array()).collect();
        territories[t_idx].contour_3d = mesh_3d_atoms[..n_boundary].to_vec();
        territories[t_idx].contour_3d_sheet = mesh_3d_sheet[..n_boundary].to_vec();
        territories[t_idx].contour_mesh_uv = mesh_uv.into_iter().map(|v| v.to_array()).collect();
        territories[t_idx].contour_mesh_3d_atoms = mesh_3d_atoms;
        territories[t_idx].contour_mesh_3d_sheet = mesh_3d_sheet;
        territories[t_idx].label_center_uv = best_poi_uv.to_array();
        territories[t_idx].label_center_3d = drape_atoms(best_poi_uv);
        territories[t_idx].label_center_3d_sheet = drape_sheet(best_poi_uv);
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
    for &(c, r, _) in my_atoms {
        let sd = p.distance(c) - r;
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

    // Fast exit if already well outside our own smooth union or if no neighbor exists.
    if d_my > 0.5 || other_atoms.is_empty() {
        return d_my;
    }

    // 2. Smooth SDF Voronoi clipping against neighboring same-side residues:
    //    `d_other` is the exact signed distance to neighboring residues' discs, so
    //    `0.5 * (d_my - d_other + gap)` is C^1 continuous everywhere (zero switching seams)
    //    and only clips where territories actually meet. `smax_poly` rounds corners cleanly.
    let mut d_other = f32::INFINITY;
    for &(other_pos, other_rad) in other_atoms {
        let sd = p.distance(other_pos) - other_rad;
        if sd < d_other {
            d_other = sd;
        }
    }

    let voronoi_clip = 0.5 * (d_my - d_other + params.voronoi_gap_a);
    let corner_blend = (0.35 * params.smin_blend_a).clamp(0.05, 0.35);
    smax_poly(d_my, voronoi_clip, corner_blend)
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

/// Computes barycentric coordinates `(w0, w1, w2)` of `p` with respect to 2D triangle `(a, b, c)`
/// and returns `(clamped_bary, dist_sq_to_triangle)`.
fn closest_barycentric_2d(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> ([f32; 3], f32) {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return ([1.0, 0.0, 0.0], p.distance_squared(a));
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return ([0.0, 1.0, 0.0], p.distance_squared(b));
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3).max(1e-8);
        let q = a + ab * v;
        return ([1.0 - v, v, 0.0], p.distance_squared(q));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return ([0.0, 0.0, 1.0], p.distance_squared(c));
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6).max(1e-8);
        let q = a + ac * w;
        return ([1.0 - w, 0.0, w], p.distance_squared(q));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6)).max(1e-8);
        let q = b + (c - b) * w;
        return ([0.0, 1.0 - w, w], p.distance_squared(q));
    }
    let denom = 1.0 / (va + vb + vc).max(1e-8);
    let v = vb * denom;
    let w = vc * denom;
    ([1.0 - v - w, v, w], 0.0)
}

/// Drapes a 2D `(u, v)` contour vertex directly onto its containing (or nearest) 3D triangle of
/// `sheet_mesh` using barycentric coordinates, with a small `±0.08 Å` normal offset per side.
/// Because barycentric interpolation commutes with linear interpolation, the draped contour
/// stays synchronized with the deforming `uv_mesh` at every `flatten_t ∈ [0, 1]`.
fn drape_uv_onto_sheet(
    uv: Vec2,
    sheet_mesh: &SurfaceMesh,
    sheet_uv: &[[f32; 2]],
    _side: InterfaceSide,
    _plane_normal: Vec3,
) -> Vec3 {
    let n_verts = sheet_mesh.vertices.len().min(sheet_uv.len());
    let mut best_d2 = f32::INFINITY;
    let mut best_pos = Vec3::ZERO;

    for &[i0, i1, i2] in &sheet_mesh.triangles {
        let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
        if u0 >= n_verts || u1 >= n_verts || u2 >= n_verts {
            continue;
        }
        let a2 = Vec2::from(sheet_uv[u0]);
        let b2 = Vec2::from(sheet_uv[u1]);
        let c2 = Vec2::from(sheet_uv[u2]);
        let ([w0, w1, w2], d2) = closest_barycentric_2d(uv, a2, b2, c2);
        if d2 < best_d2 {
            best_d2 = d2;
            let v0 = Vec3::from(sheet_mesh.vertices[u0]);
            let v1 = Vec3::from(sheet_mesh.vertices[u1]);
            let v2 = Vec3::from(sheet_mesh.vertices[u2]);
            best_pos = w0 * v0 + w1 * v1 + w2 * v2;
            if d2 == 0.0 {
                break;
            }
        }
    }

    best_pos
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
        let mut poly = vec![directed_edges[start_idx].2, directed_edges[start_idx].3];
        let start_node = directed_edges[start_idx].0;
        let mut cur_node = directed_edges[start_idx].1;

        while cur_node != start_node {
            let mut found_next = None;
            for (idx, &(u, v, _, pt_v)) in directed_edges.iter().enumerate() {
                if !used[idx] && u == cur_node {
                    used[idx] = true;
                    if v != start_node {
                        poly.push(pt_v);
                    }
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

/// Ray-casting point-in-polygon test in 2D.
fn point_in_polygon(p: Vec2, poly: &[Vec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let pi = poly[i];
        let pj = poly[j];
        if ((pi.y > p.y) != (pj.y > p.y))
            && (p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Returns true if `p` lies strictly inside the circumcircle of CCW triangle `(a, b, c)`.
fn in_circumcircle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let (ax, ay) = ((a.x - p.x) as f64, (a.y - p.y) as f64);
    let (bx, by) = ((b.x - p.x) as f64, (b.y - p.y) as f64);
    let (cx, cy) = ((c.x - p.x) as f64, (c.y - p.y) as f64);
    let orient = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    let det = (ax * ax + ay * ay) * (bx * cy - cx * by)
        - (bx * bx + by * by) * (ax * cy - cx * ay)
        + (cx * cx + cy * cy) * (ax * by - bx * ay);
    if orient >= 0.0 { det > 1e-10 } else { det < -1e-10 }
}

/// Bowyer–Watson 2D Delaunay triangulation of `points` (boundary `0..boundary.len()` + interior
/// Steiner vertices), filtered to triangles whose centroid lies inside `boundary`.
fn triangulate_delaunay_interior(boundary: &[Vec2], points: &[Vec2]) -> Vec<[u32; 3]> {
    let n = points.len();
    if n < 3 || boundary.len() < 3 {
        return Vec::new();
    }

    let mut min_p = Vec2::splat(f32::INFINITY);
    let mut max_p = Vec2::splat(f32::NEG_INFINITY);
    for &p in points {
        min_p = min_p.min(p);
        max_p = max_p.max(p);
    }
    let span = (max_p.x - min_p.x).max(max_p.y - min_p.y).max(1.0);
    let mid = 0.5 * (min_p + max_p);

    // Super-triangle containing all points.
    let mut pts = Vec::with_capacity(n + 3);
    pts.extend_from_slice(points);
    let s0 = n;
    let s1 = n + 1;
    let s2 = n + 2;
    pts.push(Vec2::new(mid.x - 20.0 * span, mid.y - span));
    pts.push(Vec2::new(mid.x, mid.y + 20.0 * span));
    pts.push(Vec2::new(mid.x + 20.0 * span, mid.y - span));

    let mut tris: Vec<[usize; 3]> = vec![[s0, s1, s2]];

    for p_idx in 0..n {
        let p = pts[p_idx];
        let mut bad = Vec::new();
        for (ti, &[a, b, c]) in tris.iter().enumerate() {
            if in_circumcircle(p, pts[a], pts[b], pts[c]) {
                bad.push(ti);
            }
        }
        if bad.is_empty() {
            continue;
        }

        // Collect boundary edges of the polygonal hole (edges belonging to only 1 bad triangle).
        let mut edges: Vec<(usize, usize)> = Vec::with_capacity(bad.len() * 3);
        for &ti in &bad {
            let [a, b, c] = tris[ti];
            edges.push((a, b));
            edges.push((b, c));
            edges.push((c, a));
        }

        let mut poly_edges = Vec::new();
        for (ei, &(u, v)) in edges.iter().enumerate() {
            let shared = edges
                .iter()
                .enumerate()
                .any(|(ej, &(a, b))| ej != ei && ((a == u && b == v) || (a == v && b == u)));
            if !shared {
                poly_edges.push((u, v));
            }
        }

        // Remove bad triangles (in reverse index order).
        for &ti in bad.iter().rev() {
            tris.swap_remove(ti);
        }

        for (u, v) in poly_edges {
            tris.push([u, v, p_idx]);
        }
    }

    // Drop super-triangle vertices and keep only triangles strictly inside the concave boundary.
    let mut out = Vec::with_capacity(tris.len());
    for [a, b, c] in tris {
        if a >= n || b >= n || c >= n {
            continue;
        }
        let centroid = (pts[a] + pts[b] + pts[c]) / 3.0;
        if point_in_polygon(centroid, boundary) {
            // Ensure CCW winding.
            let cross = (pts[b] - pts[a]).perp_dot(pts[c] - pts[a]);
            if cross >= 0.0 {
                out.push([a as u32, b as u32, c as u32]);
            } else {
                out.push([a as u32, c as u32, b as u32]);
            }
        }
    }
    out
}
