//! A smooth sheet lying midway between the van der Waals surfaces of `SideA` and `SideB`.
//! 
//! ## Summary
//! Generates a flat mesh and iteratively moves its vertices onto the bisector between the
//! two sides of the interface. This is all done in [`build_interface_sheet_mesh`]:
//! 1. Lay a regular grid of square cells on the interface's projection plane, as
//!    calculated by PCA.
//! 2. Iterate:
//!    a. Step each vertex toward the bisector along the plane normal.
//!    b. Puncture: remove triangles that do not lie on the bisector.
//!    c. Slide each vertex within the sheet toward its neighbours' average,
//!    so triangles stay evenly sized without changing the sheet's shape.
//!    d. Once the sheet has roughly settled, split every edge stretched past
//!    `SPLIT_EDGE_RATIO` × the target length, creating finer geometry to reach
//!    deeper into pockets.
//!
//!    Stop when no vertex moves more than `stop_tolerance_a` and no edge
//!    needed splitting.
//! 3. Puncture once more, strictly; trim the sheet to where it lies within
//!    `footprint_cutoff_a` of an atom surface; and split any vertex left
//!    joining two separate fans of triangles. The result is a 2-manifold that
//!    may have holes.
//!
//! ## Considerations
//! Why seeding and puncturing are needed: the bisector is a smooth surface,
//! but it can have several separate parts, such as the main interface plus a
//! tube around a loop of one partner that pokes into the other. If the
//! starting plane lies between two parts, some vertices converge onto one and
//! some onto the other. Triangles bridging between them look like branching.
//! Seeding along the plane normal cannot fold, and accepting only A-to-B
//! crossings skips wrongly facing parts.
//!
//! Puncturing runs inside the loop, not just at the end, because the final
//! pass can only remove triangles that are off the surface. Relaxing across a
//! tear drags vertices off the surface, fights the projection so the sheet
//! never settles, and can fold it into overlapping layers whose triangles each
//! lie on the surface and so survive any final check. Cutting tears before
//! each relaxation prevents that. It costs little: orientation is checked
//! against the surface normal each vertex recorded while being projected, and
//! only long triangles (possible bridges) need the field evaluated again.
//! 
//! Some constants are not parametrized because they are either purely geometric
//! or would serve no purpose to a user. e.g. `RELAX_RATE` is a compromise between
//! speed and stability, but other speed-based knobs already exist.
//!
//! ## TODO
//! (perf): a full build takes ~60 ms at default smoothing and ~300 ms at
//! the tightest (400 interface atoms, release build). Nearly all of it is
//! `soft_min_distance`, called once per vertex per iteration against every atom
//! on a side. In rough order of payoff:
//! - Warm start: when only parameters change, start from the previous mesh
//!   instead of a flat plane, so it converges in a few iterations, not dozens.
//! - Cheaper seeding: it searches the atoms' full extent along the normal for
//!   vertices with no crossing (mostly the padding outside the footprint),
//!   which a footprint check could skip.
//! - Active set: a split round puts new vertices off the surface, and the
//!   whole mesh re-settles for them (a deep pocket roughly doubles the build
//!   time). Only new vertices and their neighbours move appreciably, so
//!   skipping Newton steps for already-settled vertices would recover most of it.
//! - Coarse to fine: converge a grid with 2-4x the edge length first, then
//!   let edge splitting refine it, instead of starting at full resolution.
//! - Parallel vertices: each Newton step reads only the atoms, and relaxation
//!   reads only the previous positions, so both loops split across threads with
//!   no locking (`std::thread::scope` over chunks, no new dependency). So does
//!   the trimming search. Edge splitting stays serial; it is cheap.
//! - GPU: the Newton step (every vertex against every atom, independent
//!   outputs) maps directly onto a compute shader.
//! - Spatial grid: only helps at small smoothing, where each vertex needs atoms
//!   within footprint cutoff + largest radius + 8σ instead of the whole side.

use std::collections::HashMap;

use glam::{Vec2, Vec3};

use super::definition::InterfaceSide;
use super::model::{AtomNode, ProjectionPlane};
use crate::model::{ComplexStructure, GlobalAtomIdx, SurfaceMesh};

crate::param_group! {
    #[desc = "How to build the sheet that marks the shape of the interface."]
    pub struct InterfaceMeshParams("interface_mesh", "Interface Mesh") {
        /// Smoothing width of the distance field (Å). Must be > 0.
        #[desc = "How much fine detail is smoothed out of the sheet. Small: the sheet hugs every bump between the partners. Large: only the broad shape remains."]
        smoothing_a: f32 = 1.5, 0.4..=10.0, "Smoothing", "Å";
        /// Vertices farther than this from every atom surface are trimmed (Å).
        #[desc = "The sheet ends where the nearest atom surface is farther than this. Because the sheet sits midway, this is half the largest gap between the partners that still counts as interface."]
        footprint_cutoff_a: f32 = 4.0, 0.0..=10.0, "Footprint cutoff", "Å";
        /// Safety cap on project-and-relax iterations.
        #[desc = "Upper limit on iterations. The sheet normally stops earlier, once it settles (see Stop tolerance)."]
        max_iterations: usize = 100, 1..=1000, "Max iterations";
        /// Converged when no vertex moves more than this in one iteration (Å).
        #[desc = "The sheet counts as settled once no vertex moves more than this in one iteration. Lower: closer fit, more iterations. 0: always run Max iterations."]
        stop_tolerance_a: f32 = 0.01, 0.0..=1.0, "Stop tolerance", "Å";
        /// A projection step moves a vertex at most this fraction of an edge length.
        #[desc = "Largest move a vertex may make in one iteration, as a fraction of the mesh edge length. Lower: more robust against tangled triangles, but more iterations. It does not change the final shape."]
        max_step_per_edge: f32 = 0.5, 0.05..=1.0, "Max step per edge";
    }
}

/// Mesh edge length as a multiple of `smoothing_a`. Smoothing erases features
/// narrower than about sigma, and a feature needs ~2 edges to show, so sigma/2
/// resolves everything that survives the smoothing.
const EDGE_LENGTH_PER_SMOOTHING: f32 = 0.5;
/// Floor on edge length. Protein surface features below ~0.5 Å barely exist,
/// and vertex count grows as 1/h², so finer meshes only cost time (Å).
const MIN_EDGE_LENGTH_A: f32 = 0.5;
/// Fraction of the way each vertex slides toward its neighbours' average per
/// iteration. Must stay below 1: at 1, a checkerboard pattern of offsets
/// swaps back and forth forever instead of settling.
const RELAX_RATE: f32 = 0.5;
/// Split edges longer than this multiple of the target edge length. Must stay
/// above √2: the starting grid's diagonals are √2 × the target, and a lower
/// threshold would split every one of them before anything has moved. (The
/// usual isotropic-remeshing value, 4/3, is too low for that reason.)
const SPLIT_EDGE_RATIO: f32 = 1.5;
/// Only split once no vertex moved more than this fraction of the target edge
/// length in an iteration, so edges stretched only while the sheet is still
/// moving are not split needlessly.
const SPLIT_WHEN_SETTLED: f32 = 0.1;
/// Cap on total vertices, as a multiple of the starting grid's count, so a
/// runaway stretch can never grow the mesh without bound.
const MAX_VERTEX_GROWTH: usize = 4;
/// A triangle whose centroid is farther from the bisector than this fraction
/// of a reference length is bridging between two parts of the bisector, not
/// lying on it (see [`Stage`] for the reference). A bridge's centroid sits
/// about half its length off the surface; a chord of a curved surface sags far
/// less (a 1.5 h chord on a 2 Å radius sags ~0.1 h).
const MAX_CENTROID_OFFSET: f32 = 0.25;
/// Minimum cosine between a triangle's normal and the bisector's normal
/// (oriented to agree with the sheet as a whole). Below this the triangle is
/// tilted more than 60° from the surface or faces the wrong way.
const MIN_NORMAL_ALIGNMENT: f32 = 0.5;
/// An edge whose direction has a larger component than this along the
/// bisector's normal crosses the surface instead of running along it, so it is
/// not split. Chords of a curved surface run parallel to it at their midpoint,
/// so pocket edges are unaffected; bridging edges are what this stops.
const MAX_SPLIT_EDGE_TILT: f32 = 0.5;

/// Builds a triangle mesh lying midway between the vdW surfaces of the two sides.
///
/// `plane` is the interface's projection plane (shared with the 2D map, so the
/// sheet's (u, v) layout matches it). Mesh normals follow `axis_u × axis_v`.
/// Returns an empty mesh if either side has no atoms.
pub fn build_interface_sheet_mesh(
    complex: &ComplexStructure,
    nodes: &[AtomNode],
    plane: &ProjectionPlane,
    p: &InterfaceMeshParams,
) -> SurfaceMesh {
    debug_assert!(p.smoothing_a > 0.0, "smoothing_a must be positive, got {}", p.smoothing_a);

    // 1. Build the smoothed bisector field and seed a regular PCA-plane grid onto it.
    let Some(field) = BisectorField::new(complex, nodes, p.smoothing_a) else {
        return SurfaceMesh::default();
    };

    let h = (p.smoothing_a * EDGE_LENGTH_PER_SMOOTHING).max(MIN_EDGE_LENGTH_A);
    let max_step = p.max_step_per_edge * h;
    let mut sheet = SheetMesh::grid_on_plane(plane, field.atoms(), h, p.footprint_cutoff_a);
    sheet.seed_onto_bisector(&field, Vec3::from(plane.normal), h);
    let max_vertices = sheet.positions.len() * MAX_VERTEX_GROWTH;
    // Splitting waits for the sheet to settle, but never for less movement
    // than the stop tolerance, or the loop could stop before any split.
    let settled_below = (SPLIT_WHEN_SETTLED * h).max(p.stop_tolerance_a);

    // 2. Iteratively project toward f = 0, puncture bridges/tears, relax tangentially, and split
    //    stretched edges once settled.
    let mut converged = false;
    for _ in 0..p.max_iterations {
        let max_move = sheet.project(&field, max_step);
        // Cut tears before relaxing. Relaxation pulls each vertex toward its
        // neighbours' average; across a tear (two neighbours on different
        // parts of the bisector) that pull drags vertices off the surface,
        // fights the projection forever, and can fold the sheet into
        // overlapping layers. On the first pass this also cuts the cliffs
        // seeding leaves where neighbouring columns found different parts.
        sheet.puncture(&field, h, Stage::Fitting);
        // Relaxation moves vertices along the sheet, not off it, so it is left
        // out of the convergence test: the shape has settled once projection has.
        sheet.relax_tangentially();

        let splits = if max_move < settled_below {
            let budget = max_vertices.saturating_sub(sheet.positions.len());
            sheet.split_long_edges(SPLIT_EDGE_RATIO * h, &field, p.footprint_cutoff_a, budget)
        } else {
            0
        };
        if splits == 0 && max_move < p.stop_tolerance_a {
            converged = true;
            break;
        }
    }
    if !converged {
        // Vertices added by the last splits were never projected; measure
        // where everything now is so the final check judges current positions.
        sheet.sample_surface(&field);
    }

    // 3. Final strict puncture, footprint trim, manifold pinch-splitting, and area-weighted normals.
    sheet.puncture(&field, h, Stage::Final);
    let mut mesh = sheet.into_trimmed_mesh(&field, p.footprint_cutoff_a);
    split_pinched_vertices(&mut mesh);
    mesh.normals = vertex_normals(&positions_of(&mesh), &mesh.triangles).iter().map(Vec3::to_array).collect();
    mesh
}

/// How strictly [`SheetMesh::puncture`] judges triangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// During fitting. Vertices are still moving, so their own distance to
    /// the bisector is not held against them, and a long triangle's centroid
    /// offset is measured against its own longest edge: legitimate triangles
    /// on steep pocket walls are long until splitting refines them.
    Fitting,
    /// After fitting. Every vertex must lie on the bisector, and every
    /// offset is measured against the target edge length, since every
    /// legitimate edge is now at most `SPLIT_EDGE_RATIO` × target.
    Final,
}

/// Where the bisector is relative to a vertex, as of the vertex's last
/// projection: the distance to it (first order, |f| / |∇f|) and its unit normal.
#[derive(Debug, Clone, Copy, Default)]
struct SurfaceSample {
    offset: f32,
    normal: Vec3,
}

impl SurfaceSample {
    fn at(field: &BisectorField, x: Vec3) -> Self {
        let (offset, normal) = field.offset_and_normal(x);
        Self { offset, normal }
    }
}

/// The sheet while it is being fitted: an indexed triangle mesh, consistently
/// wound, with the vertices on its outer border flagged.
struct SheetMesh {
    /// Dense vector of vertices in 3D space.
    positions: Vec<Vec3>,
    /// Each triangle's three vertex indices in winding order.
    triangles: Vec<[u32; 3]>,
    /// Border vertices are projected but never relaxed, otherwise the sheet would shrink inward.
    is_border: Vec<bool>,
    /// Per vertex, the bisector as seen from where the vertex was last projected. Lets
    /// [`Self::puncture`] check every triangle's orientation without evaluating the field again.
    surface: Vec<SurfaceSample>,
}

impl SheetMesh {
    /// Square cells of side `h` covering the atoms' projection onto `plane`,
    /// plus `padding`, each cell cut into two triangles wound along `axis_u × axis_v`.
    fn grid_on_plane<'a>(
        plane: &ProjectionPlane,
        atoms: impl Iterator<Item = &'a Atom>,
        h: f32,
        padding: f32,
    ) -> Self {
        let origin = Vec3::from(plane.origin);
        let axis_u = Vec3::from(plane.axis_u);
        let axis_v = Vec3::from(plane.axis_v);

        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for atom in atoms {
            let rel = atom.pos - origin;
            let uv = Vec2::new(rel.dot(axis_u), rel.dot(axis_v));
            lo = lo.min(uv);
            hi = hi.max(uv);
        }
        lo -= Vec2::splat(padding);
        hi += Vec2::splat(padding);

        let n_u = ((hi.x - lo.x) / h).ceil() as usize + 1;
        let n_v = ((hi.y - lo.y) / h).ceil() as usize + 1;
        let idx = |iu: usize, iv: usize| (iu * n_v + iv) as u32;

        let mut positions = Vec::with_capacity(n_u * n_v);
        let mut is_border = Vec::with_capacity(n_u * n_v);
        for iu in 0..n_u {
            for iv in 0..n_v {
                let uv = lo + h * Vec2::new(iu as f32, iv as f32);
                positions.push(origin + uv.x * axis_u + uv.y * axis_v);
                is_border.push(iu == 0 || iv == 0 || iu == n_u - 1 || iv == n_v - 1);
            }
        }

        let mut triangles = Vec::with_capacity(2 * (n_u - 1) * (n_v - 1));
        for iu in 0..n_u - 1 {
            for iv in 0..n_v - 1 {
                let (v00, v10) = (idx(iu, iv), idx(iu + 1, iv));
                let (v01, v11) = (idx(iu, iv + 1), idx(iu + 1, iv + 1));
                triangles.push([v00, v10, v11]);
                triangles.push([v00, v11, v01]);
            }
        }

        let surface = vec![SurfaceSample::default(); positions.len()];
        Self { positions, triangles, is_border, surface }
    }

    /// Moves each vertex along the plane normal to the nearest point where
    /// the bisector crosses from side A to side B, searching in steps of
    /// `step` across the atoms' full extent along the normal. Vertices with no
    /// such crossing stay put (they end up outside the footprint).
    ///
    /// Moving only along one direction keeps the sheet a height field over
    /// the plane, which cannot fold over itself. Accepting only A-to-B
    /// crossings skips parts of the bisector that face the other way, such as
    /// the underside of a tube around a loop of side A, so a plane that starts
    /// between two parts of the bisector does not drape the sheet over both.
    fn seed_onto_bisector(&mut self, field: &BisectorField, plane_normal: Vec3, step: f32) {
        let a_to_b = field.a_to_b();
        let up = if plane_normal.dot(a_to_b) >= 0.0 { plane_normal } else { -plane_normal };
        let heights = field.atoms().map(|a| a.pos.dot(up));
        let (lo, hi) = heights.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), z| (lo.min(z), hi.max(z)));
        let steps = ((hi - lo) / step).ceil() as usize;

        for pos in &mut self.positions {
            let f_at = |t: f32| field.evaluate(*pos + t * up).0;
            let f0 = f_at(0.0);
            let (mut above, mut below) = (f0, f0);
            // Root of the linear interpolant on [t0, t1], where f goes from - to +.
            let crossing = |t0: f32, f0: f32, t1: f32, f1: f32| t0 + (t1 - t0) * f0 / (f0 - f1);
            for k in 1..=steps {
                let t = k as f32 * step;
                let f_up = f_at(t);
                if above < 0.0 && f_up >= 0.0 {
                    *pos += crossing(t - step, above, t, f_up) * up;
                    break;
                }
                above = f_up;
                let f_down = f_at(-t);
                if f_down < 0.0 && below >= 0.0 {
                    *pos += crossing(-t, f_down, -(t - step), below) * up;
                    break;
                }
                below = f_down;
            }
        }
    }

    /// One Newton step per vertex toward f = 0, each at most `max_step` long:
    /// x ← x - f ∇f / |∇f|². Records what each vertex saw of the bisector
    /// before stepping. Returns the longest step taken.
    fn project(&mut self, field: &BisectorField, max_step: f32) -> f32 {
        let mut max_move = 0.0_f32;
        for (pos, sample) in self.positions.iter_mut().zip(&mut self.surface) {
            let (f, grad) = field.evaluate(*pos);
            let len = grad.length();
            if len < f32::EPSILON {
                *sample = SurfaceSample { offset: f32::INFINITY, normal: Vec3::ZERO };
                continue;
            }
            *sample = SurfaceSample { offset: f.abs() / len, normal: grad / len };
            let step = (-f * grad / (len * len)).clamp_length_max(max_step);
            *pos += step;
            max_move = max_move.max(step.length());
        }
        max_move
    }

    /// Records what each vertex sees of the bisector, without moving it.
    fn sample_surface(&mut self, field: &BisectorField) {
        for (pos, sample) in self.positions.iter().zip(&mut self.surface) {
            *sample = SurfaceSample::at(field, *pos);
        }
    }

    /// Each vertex's neighbours: the vertices it shares an edge with.
    fn neighbours(&self) -> Vec<Vec<u32>> {
        let mut rings = vec![Vec::new(); self.positions.len()];
        for tri in &self.triangles {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                rings[a as usize].push(b);
                rings[b as usize].push(a);
            }
        }
        for ring in &mut rings {
            ring.sort_unstable();
            ring.dedup();
        }
        rings
    }

    /// Slides each interior vertex `RELAX_RATE` of the way toward the mean of
    /// its neighbours, keeping only the part of that move within the vertex's
    /// tangent plane, so spacing evens out while the shape stays put.
    fn relax_tangentially(&mut self) {
        let rings = self.neighbours();
        let normals = vertex_normals(&self.positions, &self.triangles);
        let old = self.positions.clone();
        for (i, ring) in rings.iter().enumerate() {
            if self.is_border[i] || ring.is_empty() {
                continue;
            }
            let mean = ring.iter().map(|&j| old[j as usize]).sum::<Vec3>() / ring.len() as f32;
            let delta = mean - old[i];
            let tangential = delta - delta.dot(normals[i]) * normals[i];
            self.positions[i] += RELAX_RATE * tangential;
        }
    }

    /// Splits edges longer than `max_len` at their midpoint, longest first.
    /// Adds at most `budget` vertices and returns how many it added. Skips
    /// edges whose midpoint is outside the footprint (`cutoff` from every atom
    /// surface): they would be trimmed anyway, and the bisector there can run
    /// off indefinitely. Also skips edges that cross the bisector instead of
    /// running along it (see `MAX_SPLIT_EDGE_TILT`): splitting a bridge only
    /// lengthens it.
    ///
    /// Splitting edge a–b at m turns each triangle (a, b, c) on it into
    /// (a, m, c) and (m, b, c), which keeps winding and leaves no T-junctions.
    /// Within one call, no two splits touch the same triangle, so every split
    /// sees an unchanged neighbourhood; edges skipped for that reason are
    /// still long next iteration and get split then.
    fn split_long_edges(&mut self, max_len: f32, field: &BisectorField, cutoff: f32, budget: usize) -> usize {
        let mut edge_triangles: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (t, tri) in self.triangles.iter().enumerate() {
            for k in 0..3 {
                edge_triangles.entry(edge_key(tri[k], tri[(k + 1) % 3])).or_default().push(t);
            }
        }

        let max_len_sq = max_len * max_len;
        let mut candidates: Vec<(f32, (u32, u32), SurfaceSample)> = edge_triangles
            .keys()
            .filter_map(|&(a, b)| {
                let (pa, pb) = (self.positions[a as usize], self.positions[b as usize]);
                let len_sq = pa.distance_squared(pb);
                if len_sq <= max_len_sq {
                    return None;
                }
                let mid = 0.5 * (pa + pb);
                let sample = SurfaceSample::at(field, mid);
                let along_surface = (pb - pa).normalize_or_zero().dot(sample.normal).abs() <= MAX_SPLIT_EDGE_TILT;
                (along_surface && field.nearest_surface(mid).0 <= cutoff).then_some((len_sq, (a, b), sample))
            })
            .collect();
        candidates.sort_unstable_by(|l, r| r.0.total_cmp(&l.0).then_with(|| l.1.cmp(&r.1)));

        let mut touched = vec![false; self.triangles.len()];
        let mut added = 0;
        for (_, (a, b), sample) in candidates {
            if added == budget {
                break;
            }
            let tris = &edge_triangles[&(a, b)];
            if tris.iter().any(|&t| touched[t]) {
                continue;
            }
            let m = self.positions.len() as u32;
            self.positions.push(self.midpoint(a, b));
            // An edge with one triangle lies on the border, and so does its midpoint.
            self.is_border.push(tris.len() == 1);
            self.surface.push(sample);
            for &t in tris {
                touched[t] = true;
                let [x, y, z] = rotate_to_edge(self.triangles[t], a, b);
                self.triangles[t] = [x, m, z];
                self.triangles.push([m, y, z]);
            }
            added += 1;
        }
        added
    }

    fn midpoint(&self, a: u32, b: u32) -> Vec3 {
        0.5 * (self.positions[a as usize] + self.positions[b as usize])
    }

    /// Removes triangles that do not lie on the bisector, then drops vertices
    /// no triangle uses and re-flags border vertices, so the new holes' rims
    /// are not relaxed inward. Returns how many triangles were removed.
    ///
    /// A triangle is kept if its normal is within 60° of the bisector's
    /// (oriented to agree with the sheet as a whole; see
    /// `MIN_NORMAL_ALIGNMENT`), and it lies on the bisector:
    /// - a long triangle (an edge over `SPLIT_EDGE_RATIO` × `h`) has its
    ///   centroid checked against the field directly, since a bridge between
    ///   two parts of the bisector has vertices on the surface but a centroid
    ///   far off it (see `MAX_CENTROID_OFFSET`);
    /// - a short triangle cannot sag far between vertices that are on the
    ///   surface, so at [`Stage::Final`] only its vertices are checked.
    ///
    /// Only long triangles cost a field evaluation; everything else uses what
    /// [`Self::project`] recorded, so this is cheap enough to run every iteration.
    fn puncture(&mut self, field: &BisectorField, h: f32, stage: Stage) -> usize {
        let long_edge = SPLIT_EDGE_RATIO * h;
        let max_offset = MAX_CENTROID_OFFSET * h;
        // Per triangle: area-weighted face normal, whether it lies on the
        // bisector, and the bisector's unit normal there.
        let probes: Vec<(Vec3, bool, Vec3)> = self
            .triangles
            .iter()
            .map(|&[a, b, c]| {
                let (pa, pb, pc) = (self.positions[a as usize], self.positions[b as usize], self.positions[c as usize]);
                let face = (pb - pa).cross(pc - pa);
                let longest = pa.distance(pb).max(pb.distance(pc)).max(pc.distance(pa));
                if longest > long_edge {
                    let centroid = SurfaceSample::at(field, (pa + pb + pc) / 3.0);
                    let reference = if stage == Stage::Final { h } else { longest };
                    return (face, centroid.offset <= MAX_CENTROID_OFFSET * reference, centroid.normal);
                }
                let [sa, sb, sc] = [a, b, c].map(|v| self.surface[v as usize]);
                let on_surface = stage == Stage::Fitting || sa.offset.max(sb.offset).max(sc.offset) <= max_offset;
                (face, on_surface, (sa.normal + sb.normal + sc.normal).normalize_or_zero())
            })
            .collect();
        // Orient the bisector's normal to agree with the sheet as a whole, so
        // the minority facing the other way is what gets removed.
        let orientation = probes.iter().map(|(n, _, g)| n.dot(*g)).sum::<f32>().signum();
        let mut keep = probes
            .iter()
            .map(|&(face, on_surface, normal)| {
                on_surface && orientation * face.normalize_or_zero().dot(normal) >= MIN_NORMAL_ALIGNMENT
            })
            .collect::<Vec<_>>()
            .into_iter();

        let before = self.triangles.len();
        self.triangles.retain(|_| keep.next().unwrap_or(false));
        let removed = before - self.triangles.len();
        if removed > 0 {
            self.drop_unused_vertices();
            self.is_border = self.border_flags();
        }
        removed
    }

    fn drop_unused_vertices(&mut self) {
        let mut remap = vec![u32::MAX; self.positions.len()];
        let (mut positions, mut surface) = (Vec::new(), Vec::new());
        for tri in &mut self.triangles {
            for v in tri.iter_mut() {
                if remap[*v as usize] == u32::MAX {
                    remap[*v as usize] = positions.len() as u32;
                    positions.push(self.positions[*v as usize]);
                    surface.push(self.surface[*v as usize]);
                }
                *v = remap[*v as usize];
            }
        }
        self.positions = positions;
        self.surface = surface;
    }

    /// A vertex is on the border if it lies on an edge with only one triangle.
    fn border_flags(&self) -> Vec<bool> {
        let mut edge_count: HashMap<(u32, u32), u32> = HashMap::new();
        for tri in &self.triangles {
            for k in 0..3 {
                *edge_count.entry(edge_key(tri[k], tri[(k + 1) % 3])).or_default() += 1;
            }
        }
        let mut border = vec![false; self.positions.len()];
        for ((a, b), n) in edge_count {
            if n == 1 {
                border[a as usize] = true;
                border[b as usize] = true;
            }
        }
        border
    }

    /// Keeps the triangles whose three vertices lie within `cutoff` of an atom
    /// surface, and the vertices they use. Normals are left for the caller,
    /// after vertex splitting.
    fn into_trimmed_mesh(self, field: &BisectorField, cutoff: f32) -> SurfaceMesh {
        let nearest: Vec<(f32, GlobalAtomIdx)> = self.positions.iter().map(|&x| field.nearest_surface(x)).collect();

        let mut mesh = SurfaceMesh::default();
        let mut remap = vec![u32::MAX; self.positions.len()];
        for tri in &self.triangles {
            if !tri.iter().all(|&v| nearest[v as usize].0 <= cutoff) {
                continue;
            }
            let kept = tri.map(|v| {
                if remap[v as usize] == u32::MAX {
                    remap[v as usize] = mesh.vertices.len() as u32;
                    mesh.vertices.push(self.positions[v as usize].to_array());
                    mesh.nearest_atom.push(nearest[v as usize].1);
                }
                remap[v as usize]
            });
            mesh.triangles.push(kept);
        }
        mesh
    }
}

/// Removing triangles can leave a vertex joining two fans of triangles that
/// share no edge around it, like the centre of a bow tie. That vertex makes
/// the mesh non-manifold, so it is duplicated: one copy per fan.
fn split_pinched_vertices(mesh: &mut SurfaceMesh) {
    let mut incident = vec![Vec::new(); mesh.vertices.len()];
    for (t, tri) in mesh.triangles.iter().enumerate() {
        for &v in tri {
            incident[v as usize].push(t);
        }
    }

    for (v, tris) in incident.iter().enumerate() {
        let v = v as u32;
        for fan in fans_around(&mesh.triangles, v, tris).into_iter().skip(1) {
            let copy = mesh.vertices.len() as u32;
            mesh.vertices.push(mesh.vertices[v as usize]);
            mesh.nearest_atom.push(mesh.nearest_atom[v as usize]);
            for t in fan {
                for corner in &mut mesh.triangles[t] {
                    if *corner == v {
                        *corner = copy;
                    }
                }
            }
        }
    }
}

/// Groups the triangles around `v` into fans: triangles connected through
/// edges that contain `v`.
fn fans_around(triangles: &[[u32; 3]], v: u32, around: &[usize]) -> Vec<Vec<usize>> {
    let mut fan_of = vec![usize::MAX; around.len()];
    let mut fans: Vec<Vec<usize>> = Vec::new();
    for start in 0..around.len() {
        if fan_of[start] != usize::MAX {
            continue;
        }
        let id = fans.len();
        fans.push(Vec::new());
        let mut stack = vec![start];
        fan_of[start] = id;
        while let Some(i) = stack.pop() {
            fans[id].push(around[i]);
            let shares_edge_through_v = |j: usize| {
                triangles[around[i]].iter().any(|&w| w != v && triangles[around[j]].contains(&w))
            };
            for (j, fan) in fan_of.iter_mut().enumerate() {
                if *fan == usize::MAX && shares_edge_through_v(j) {
                    *fan = id;
                    stack.push(j);
                }
            }
        }
    }
    fans
}

fn positions_of(mesh: &SurfaceMesh) -> Vec<Vec3> {
    mesh.vertices.iter().map(|&v| Vec3::from(v)).collect()
}

/// Undirected edge key: the same for a–b and b–a.
fn edge_key(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

/// Rotates `tri` so its first two entries are the edge a–b, in whichever
/// direction the triangle's winding runs along it. Rotation keeps the winding.
fn rotate_to_edge(tri: [u32; 3], a: u32, b: u32) -> [u32; 3] {
    let key = edge_key(a, b);
    (0..3)
        .map(|k| [tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]])
        .find(|r| edge_key(r[0], r[1]) == key)
        .expect("triangle contains the edge it is listed under")
}

/// Per-vertex normals: the sum of adjacent face normals, each weighted by face area.
fn vertex_normals(positions: &[Vec3], triangles: &[[u32; 3]]) -> Vec<Vec3> {
    let mut acc = vec![Vec3::ZERO; positions.len()];
    for &[a, b, c] in triangles {
        let (pa, pb, pc) = (positions[a as usize], positions[b as usize], positions[c as usize]);
        let n = (pb - pa).cross(pc - pa); // length = 2 × area, so the sum is area-weighted
        for v in [a, b, c] {
            acc[v as usize] += n;
        }
    }
    acc.into_iter().map(Vec3::normalize_or_zero).collect()
}

// ---------------------------------------------------------------------------
// Bisector field: "where should this vertex be?"
// ---------------------------------------------------------------------------

/// An interfacial atom as the field sees it.
#[derive(Debug, Clone, Copy)]
struct Atom {
    pos: Vec3,
    /// vdW radius (Å).
    radius: f32,
    idx: GlobalAtomIdx,
}

impl Atom {
    /// Signed distance from `x` to this atom's surface (negative inside).
    fn surface_distance(&self, x: Vec3) -> f32 {
        x.distance(self.pos) - self.radius
    }
}

/// The smoothed surface midway between the two sides' vdW surfaces.
/// Holds each side's atoms, split once up front.
struct BisectorField {
    side_a: Vec<Atom>,
    side_b: Vec<Atom>,
    sigma: f32,
}

impl BisectorField {
    /// `None` if either side has no atoms: there is no interface to bisect.
    fn new(complex: &ComplexStructure, nodes: &[AtomNode], sigma: f32) -> Option<Self> {
        let (mut side_a, mut side_b) = (Vec::new(), Vec::new());
        for node in nodes {
            let source = &complex.atoms[node.global_atom.0 as usize];
            let atom = Atom {
                pos: Vec3::from(source.pos_3d),
                radius: source.vdw_radius_a,
                idx: node.global_atom,
            };
            match node.side {
                InterfaceSide::SideA => side_a.push(atom),
                InterfaceSide::SideB => side_b.push(atom),
                InterfaceSide::None => {}
            }
        }
        (!side_a.is_empty() && !side_b.is_empty()).then_some(Self { side_a, side_b, sigma })
    }

    fn atoms(&self) -> impl Iterator<Item = &Atom> {
        self.side_a.iter().chain(&self.side_b)
    }

    /// From the centroid of side A's atoms to the centroid of side B's.
    fn a_to_b(&self) -> Vec3 {
        let centroid = |atoms: &[Atom]| atoms.iter().map(|a| a.pos).sum::<Vec3>() / atoms.len() as f32;
        centroid(&self.side_b) - centroid(&self.side_a)
    }

    /// f = d_A - d_B and its gradient. ∇f points from side A toward side B,
    /// and has length ≈ 2 where the two surfaces face each other.
    fn evaluate(&self, x: Vec3) -> (f32, Vec3) {
        let (d_a, grad_a) = soft_min_distance(&self.side_a, x, self.sigma);
        let (d_b, grad_b) = soft_min_distance(&self.side_b, x, self.sigma);
        (d_a - d_b, grad_a - grad_b)
    }

    /// Distance from `x` to the bisector (to first order, |f| / |∇f|) and the
    /// bisector's unit normal, from a single evaluation.
    fn offset_and_normal(&self, x: Vec3) -> (f32, Vec3) {
        let (f, grad) = self.evaluate(x);
        let len = grad.length();
        if len < f32::EPSILON {
            return (f32::INFINITY, Vec3::ZERO);
        }
        (f.abs() / len, grad / len)
    }

    /// Distance from `x` to the nearest atom surface on either side, and that atom.
    fn nearest_surface(&self, x: Vec3) -> (f32, GlobalAtomIdx) {
        self.atoms()
            .map(|a| (a.surface_distance(x), a.idx))
            .min_by(|l, r| l.0.total_cmp(&r.0))
            .expect("BisectorField always has atoms on both sides")
    }
}

/// Smoothed distance from `x` to the union of `atoms`' surfaces, and its gradient.
///
///   s_i  = distance from x to atom i's surface
///   d(x) = -σ ln( (1/N) Σ_i exp(-s_i / σ) )
///   ∇d   = Σ_i e_i û_i / Σ_i e_i,   e_i = exp(-s_i / σ),   û_i = unit vector from atom i to x
///
/// d is a soft minimum: it approaches min_i s_i as σ → 0 and blends nearby atoms
/// as σ grows. Dividing by N (log-mean-exp, not log-sum-exp) keeps atom count
/// out of the result: summing instead would make the side with more atoms read
/// as closer by σ ln(N_A / N_B) and push the sheet toward the smaller partner.
/// Exponents are shifted by min s_i so they never overflow.
///
/// `atoms` must be non-empty.
fn soft_min_distance(atoms: &[Atom], x: Vec3, sigma: f32) -> (f32, Vec3) {
    let s_min = atoms.iter().map(|a| a.surface_distance(x)).fold(f32::INFINITY, f32::min);
    // Contributions with (s_i - s_min) / sigma > 18 are < exp(-18) ≈ 1.5e-8 (< f32::EPSILON).
    let s_cutoff = s_min + 18.0 * sigma;
    let inv_sigma = -1.0 / sigma;

    let (mut e_sum, mut grad) = (0.0_f32, Vec3::ZERO);
    for atom in atoms {
        let max_r = s_cutoff + atom.radius;
        let offset = x - atom.pos;
        let d2 = offset.length_squared();
        if max_r > 0.0 && d2 > max_r * max_r {
            continue;
        }
        let dist = d2.sqrt().max(f32::EPSILON);
        let e = ((dist - atom.radius - s_min) * inv_sigma).exp();
        e_sum += e;
        grad += e * offset / dist;
    }
    let distance = s_min - sigma * (e_sum / atoms.len() as f32).ln();
    (distance, grad / e_sum)
}
