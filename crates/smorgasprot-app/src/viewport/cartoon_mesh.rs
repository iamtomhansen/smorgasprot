//! Procedural DSSP Helix–Sheet–Loop protein cartoon mesh builder.

use crate::state::{CartoonParams, CartoonQuality};
use egui::Color32;
use glam::Vec3;
use smorgasprot_core::{SecStruct, SurfaceMesh};

/// Per-residue input sample along a polypeptide chain's Cα trace.
#[derive(Clone, Copy)]
pub struct DsspResidueSample {
    pub ca: Vec3,
    pub co: Vec3,
    pub sec: SecStruct,
    pub color: Color32,
}

#[derive(Clone, Copy)]
struct SpineSample {
    pos: Vec3,
    up: Vec3,
    width: f32,
    thickness: f32,
    color: Color32,
}

/// Builds a combined [`SurfaceMesh`] + per-vertex colors for one chain's DSSP cartoon.
pub fn build_dssp_cartoon_mesh(
    samples: &[DsspResidueSample],
    quality: CartoonQuality,
    params: &CartoonParams,
) -> (SurfaceMesh, Vec<Color32>) {
    if samples.len() < 2 {
        return (SurfaceMesh::default(), Vec::new());
    }

    // Partition into contiguous SecStruct runs.
    let mut runs: Vec<(SecStruct, usize, usize)> = Vec::new();
    let mut start = 0;
    for i in 1..samples.len() {
        if samples[i].sec != samples[start].sec {
            runs.push((samples[start].sec, start, i - 1));
            start = i;
        }
    }
    runs.push((samples[start].sec, start, samples.len() - 1));

    // Precompute smoothed/schematic endpoints for Helix and Sheet runs so adjacent Loops bridge cleanly.
    let mut run_endpoints: Vec<(Vec3, Vec3)> = runs
        .iter()
        .map(|&(_, s, e)| (samples[s].ca, samples[e].ca))
        .collect();

    let mut mesh_verts = Vec::new();
    let mut mesh_norms = Vec::new();
    let mut mesh_tris = Vec::new();
    let mut mesh_cols = Vec::new();

    // Build Helix and Sheet runs first to record exact endpoints, then Loop runs.
    let mut deferred_loops = Vec::new();
    for (ri, &(kind, s, e)) in runs.iter().enumerate() {
        if kind == SecStruct::Loop {
            deferred_loops.push((ri, s, e));
            continue;
        }
        let seg = &samples[s..=e];
        if seg.len() < 2 {
            deferred_loops.push((ri, s, e));
            continue;
        }
        let (ep_start, ep_end) = match (kind, quality) {
            (SecStruct::Helix, CartoonQuality::Schematic) => {
                build_helix_cylinder(seg, params, &mut mesh_verts, &mut mesh_norms, &mut mesh_tris, &mut mesh_cols)
            }
            (SecStruct::Sheet, CartoonQuality::Schematic) => {
                build_sheet_box(seg, params, &mut mesh_verts, &mut mesh_norms, &mut mesh_tris, &mut mesh_cols)
            }
            (SecStruct::Helix, CartoonQuality::Detailed) => {
                build_helix_ribbon(seg, params, &mut mesh_verts, &mut mesh_norms, &mut mesh_tris, &mut mesh_cols)
            }
            (SecStruct::Sheet, CartoonQuality::Detailed) => {
                build_sheet_arrow_ribbon(seg, params, &mut mesh_verts, &mut mesh_norms, &mut mesh_tris, &mut mesh_cols)
            }
            (SecStruct::Loop, _) => unreachable!(),
        };
        run_endpoints[ri] = (ep_start, ep_end);
    }

    for (ri, s, e) in deferred_loops {
        let mut pts: Vec<(Vec3, Color32)> = Vec::new();
        if ri > 0 {
            pts.push((run_endpoints[ri - 1].1, samples[s.saturating_sub(1)].color));
        }
        for sample in &samples[s..=e] {
            pts.push((sample.ca, sample.color));
        }
        if ri + 1 < runs.len() {
            pts.push((run_endpoints[ri + 1].0, samples[(e + 1).min(samples.len() - 1)].color));
        }
        build_loop_tube(
            &pts,
            params,
            &mut mesh_verts,
            &mut mesh_norms,
            &mut mesh_tris,
            &mut mesh_cols,
        );
    }

    (
        SurfaceMesh {
            vertices: mesh_verts,
            normals: mesh_norms,
            triangles: mesh_tris,
            nearest_atom: Vec::new(),
        },
        mesh_cols,
    )
}

fn fit_axis(seg: &[DsspResidueSample]) -> (Vec3, Vec3) {
    let mid = seg.len() / 2;
    let first_half = &seg[..mid.max(1)];
    let second_half = &seg[mid..];
    let c0 = first_half.iter().map(|s| s.ca).sum::<Vec3>() / first_half.len() as f32;
    let c1 = second_half.iter().map(|s| s.ca).sum::<Vec3>() / second_half.len() as f32;
    let dir = (c1 - c0).normalize_or_zero();
    let dir = if dir.length_squared() > 0.5 {
        dir
    } else {
        (seg.last().unwrap().ca - seg[0].ca).normalize_or(Vec3::Z)
    };
    let origin = seg.iter().map(|s| s.ca).sum::<Vec3>() / seg.len() as f32;
    (origin, dir)
}

fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

fn build_loop_tube(
    pts: &[(Vec3, Color32)],
    params: &CartoonParams,
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) {
    if pts.len() < 2 {
        return;
    }
    let subdiv = params.spline_subdiv.max(1);
    let mut spine = Vec::with_capacity((pts.len() - 1) * subdiv + 1);
    for i in 0..(pts.len() - 1) {
        let p0 = pts[i.saturating_sub(1)].0;
        let p1 = pts[i].0;
        let p2 = pts[i + 1].0;
        let p3 = pts[(i + 2).min(pts.len() - 1)].0;
        let steps = if i + 2 == pts.len() { subdiv + 1 } else { subdiv };
        for step in 0..steps {
            let t = step as f32 / subdiv as f32;
            let pos = catmull_rom(p0, p1, p2, p3, t);
            let color = if t < 0.5 { pts[i].1 } else { pts[i + 1].1 };
            spine.push(SpineSample {
                pos,
                up: Vec3::ZERO,
                width: params.loop_radius_a * 2.0,
                thickness: params.loop_radius_a * 2.0,
                color,
            });
        }
    }
    sweep_round_tube(&spine, 8, verts, norms, tris, cols);
}

fn build_helix_cylinder(
    seg: &[DsspResidueSample],
    params: &CartoonParams,
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) -> (Vec3, Vec3) {
    let (origin, dir) = fit_axis(seg);
    let mut t_min = f32::INFINITY;
    let mut t_max = f32::NEG_INFINITY;
    for s in seg {
        let t = (s.ca - origin).dot(dir);
        t_min = t_min.min(t);
        t_max = t_max.max(t);
    }
    let p_start = origin + dir * t_min;
    let p_end = origin + dir * t_max;
    let d = params.helix_radius_a * 2.0;
    let spine: Vec<SpineSample> = (0..seg.len())
        .map(|i| {
            let f = i as f32 / (seg.len() - 1).max(1) as f32;
            SpineSample {
                pos: p_start.lerp(p_end, f),
                up: Vec3::ZERO,
                width: d,
                thickness: d,
                color: seg[i].color,
            }
        })
        .collect();
    sweep_round_tube(&spine, 12, verts, norms, tris, cols);
    (p_start, p_end)
}

fn build_sheet_box(
    seg: &[DsspResidueSample],
    params: &CartoonParams,
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) -> (Vec3, Vec3) {
    let (_, dir) = fit_axis(seg);
    let p_start = seg[0].ca;
    let p_end = seg.last().unwrap().ca;
    let avg_up = aligned_sheet_normals(seg).into_iter().sum::<Vec3>().normalize_or(dir.any_orthogonal_vector());
    let spine: Vec<SpineSample> = (0..seg.len())
        .map(|i| {
            let f = i as f32 / (seg.len() - 1).max(1) as f32;
            SpineSample {
                pos: p_start.lerp(p_end, f),
                up: avg_up,
                width: params.sheet_width_a,
                thickness: params.sheet_thickness_a,
                color: seg[i].color,
            }
        })
        .collect();
    sweep_rectangular_ribbon(&spine, verts, norms, tris, cols);
    (p_start, p_end)
}

fn build_helix_ribbon(
    seg: &[DsspResidueSample],
    params: &CartoonParams,
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) -> (Vec3, Vec3) {
    let (origin, dir) = fit_axis(seg);
    let mut ctrl_pts = Vec::with_capacity(seg.len());
    let mut ctrl_ups = Vec::with_capacity(seg.len());
    for s in seg {
        let proj = origin + dir * (s.ca - origin).dot(dir);
        let rad = (s.ca - proj).normalize_or(dir.any_orthogonal_vector());
        let ideal = proj + rad * params.helix_radius_a;
        ctrl_pts.push(s.ca.lerp(ideal, params.helix_smoothing));
        ctrl_ups.push(rad);
    }

    let subdiv = params.spline_subdiv.max(1);
    let mut spine = Vec::with_capacity((seg.len() - 1) * subdiv + 1);
    let total_steps = (seg.len() - 1) * subdiv;
    let arrow_steps = if params.helix_arrow { subdiv.max(2) } else { 0 };

    for i in 0..(seg.len() - 1) {
        let p0 = ctrl_pts[i.saturating_sub(1)];
        let p1 = ctrl_pts[i];
        let p2 = ctrl_pts[i + 1];
        let p3 = ctrl_pts[(i + 2).min(seg.len() - 1)];
        let steps = if i + 2 == seg.len() { subdiv + 1 } else { subdiv };
        for step in 0..steps {
            let t = step as f32 / subdiv as f32;
            let idx = i * subdiv + step;
            let pos = catmull_rom(p0, p1, p2, p3, t);
            let up = ctrl_ups[i].lerp(ctrl_ups[i + 1], t).normalize_or(dir.any_orthogonal_vector());
            let mut width = 1.85;
            if arrow_steps > 0 && idx + arrow_steps >= total_steps {
                let rem = (total_steps - idx) as f32 / arrow_steps as f32;
                width *= (rem * 1.45).max(0.08);
            }
            let color = if t < 0.5 { seg[i].color } else { seg[i + 1].color };
            spine.push(SpineSample {
                pos,
                up,
                width,
                thickness: 0.40,
                color,
            });
        }
    }
    sweep_rectangular_ribbon(&spine, verts, norms, tris, cols);
    (ctrl_pts[0], *ctrl_pts.last().unwrap())
}

fn aligned_sheet_normals(seg: &[DsspResidueSample]) -> Vec<Vec3> {
    let mut normals = Vec::with_capacity(seg.len());
    let mut prev = Vec3::Y;
    for (i, s) in seg.iter().enumerate() {
        let next = seg[(i + 1).min(seg.len() - 1)].ca;
        let p_prev = seg[i.saturating_sub(1)].ca;
        let t = (next - p_prev).normalize_or(Vec3::Z);
        let mut n = if s.co.length_squared() > 0.1 {
            // Peptide C=O lies in the sheet plane; t × (C=O) points normal to the sheet (along the Cα pleat).
            let cross = t.cross(s.co);
            if cross.length_squared() > 1e-4 {
                cross.normalize()
            } else {
                t.any_orthogonal_vector()
            }
        } else {
            t.any_orthogonal_vector()
        };
        if i > 0 && n.dot(prev) < 0.0 {
            n = -n;
        }
        prev = n;
        normals.push(n);
    }
    normals
}

fn build_sheet_arrow_ribbon(
    seg: &[DsspResidueSample],
    params: &CartoonParams,
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) -> (Vec3, Vec3) {
    let mut ctrl_pts: Vec<Vec3> = seg.iter().map(|s| s.ca).collect();
    if !params.sheet_zigzag && ctrl_pts.len() >= 3 {
        let raw = ctrl_pts.clone();
        for i in 1..(ctrl_pts.len() - 1) {
            ctrl_pts[i] = 0.25 * raw[i - 1] + 0.5 * raw[i] + 0.25 * raw[i + 1];
        }
    }
    let ctrl_ups = aligned_sheet_normals(seg);
    let subdiv = params.spline_subdiv.max(1);
    let total_steps = (seg.len() - 1) * subdiv;
    let arrow_steps = subdiv.max(2);
    let mut spine = Vec::with_capacity(total_steps + 1);

    for i in 0..(seg.len() - 1) {
        let p0 = ctrl_pts[i.saturating_sub(1)];
        let p1 = ctrl_pts[i];
        let p2 = ctrl_pts[i + 1];
        let p3 = ctrl_pts[(i + 2).min(seg.len() - 1)];
        let steps = if i + 2 == seg.len() { subdiv + 1 } else { subdiv };
        for step in 0..steps {
            let t = step as f32 / subdiv as f32;
            let idx = i * subdiv + step;
            let pos = catmull_rom(p0, p1, p2, p3, t);
            let up = ctrl_ups[i].lerp(ctrl_ups[i + 1], t).normalize_or(Vec3::Y);
            let mut width = params.sheet_width_a;
            if idx + arrow_steps >= total_steps {
                let rem = (total_steps - idx) as f32 / arrow_steps as f32;
                width *= (rem * params.sheet_arrow_scale).max(0.06);
            }
            let color = if t < 0.5 { seg[i].color } else { seg[i + 1].color };
            spine.push(SpineSample {
                pos,
                up,
                width,
                thickness: params.sheet_thickness_a,
                color,
            });
        }
    }
    sweep_rectangular_ribbon(&spine, verts, norms, tris, cols);
    (ctrl_pts[0], *ctrl_pts.last().unwrap())
}

fn compute_tangents(spine: &[SpineSample]) -> Vec<Vec3> {
    let n = spine.len();
    let mut tangents = Vec::with_capacity(n);
    for i in 0..n {
        let raw = if i == 0 {
            spine[1].pos - spine[0].pos
        } else if i + 1 == n {
            spine[n - 1].pos - spine[n - 2].pos
        } else {
            spine[i + 1].pos - spine[i - 1].pos
        };
        let t = raw.normalize_or_zero();
        tangents.push(if t.length_squared() > 0.5 { t } else { tangents.last().copied().unwrap_or(Vec3::Z) });
    }
    tangents
}

fn sweep_round_tube(
    spine: &[SpineSample],
    sides: usize,
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) {
    let n = spine.len();
    if n < 2 || sides < 3 {
        return;
    }
    let tangents = compute_tangents(spine);
    let base_v = verts.len() as u32;
    let mut u = tangents[0].any_orthogonal_vector();

    for (i, (s, &t)) in spine.iter().zip(&tangents).enumerate() {
        if i > 0 {
            let proj = u - t * u.dot(t);
            u = if proj.length_squared() > 1e-6 { proj.normalize() } else { t.any_orthogonal_vector() };
        }
        let v = t.cross(u);
        let r = 0.5 * s.width.max(0.05);
        for k in 0..sides {
            let (sa, ca) = ((k as f32 / sides as f32) * std::f32::consts::TAU).sin_cos();
            let dir = (u * ca + v * sa).normalize();
            verts.push((s.pos + dir * r).into());
            norms.push(dir.into());
            cols.push(s.color);
        }
    }

    for i in 0..(n - 1) {
        let r0 = base_v + (i * sides) as u32;
        let r1 = base_v + ((i + 1) * sides) as u32;
        for k in 0..sides {
            let kn = (k + 1) % sides;
            tris.push([r0 + k as u32, r0 + kn as u32, r1 + kn as u32]);
            tris.push([r0 + k as u32, r1 + kn as u32, r1 + k as u32]);
        }
    }

    let cap_s = verts.len() as u32;
    verts.push(spine[0].pos.into());
    norms.push((-tangents[0]).into());
    cols.push(spine[0].color);
    for k in 0..sides {
        let kn = (k + 1) % sides;
        tris.push([cap_s, base_v + kn as u32, base_v + k as u32]);
    }

    let cap_e = verts.len() as u32;
    let last_r = base_v + ((n - 1) * sides) as u32;
    verts.push(spine[n - 1].pos.into());
    norms.push(tangents[n - 1].into());
    cols.push(spine[n - 1].color);
    for k in 0..sides {
        let kn = (k + 1) % sides;
        tris.push([cap_e, last_r + k as u32, last_r + kn as u32]);
    }
}

/// Sweeps a 4-face beveled/rectangular ribbon cross-section with crisp face normals.
fn sweep_rectangular_ribbon(
    spine: &[SpineSample],
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) {
    let n = spine.len();
    if n < 2 {
        return;
    }
    let tangents = compute_tangents(spine);
    let base_v = verts.len() as u32;
    // 8 vertices per ring (4 corners × 2 split normals for sharp edges).
    const RING: usize = 8;

    for (s, &t) in spine.iter().zip(&tangents) {
        let mut up = s.up - t * s.up.dot(t);
        if up.length_squared() < 1e-5 {
            up = t.any_orthogonal_vector();
        } else {
            up = up.normalize();
        }
        let side = t.cross(up).normalize();
        let hw = 0.5 * s.width.max(0.04);
        let ht = 0.5 * s.thickness.max(0.04);
        let corners = [
            s.pos + side * hw + up * ht,
            s.pos - side * hw + up * ht,
            s.pos - side * hw - up * ht,
            s.pos + side * hw - up * ht,
        ];
        // Face pairs: top (+up), left (-side), bottom (-up), right (+side)
        let ring_data = [
            (corners[0], up),
            (corners[1], up),
            (corners[1], -side),
            (corners[2], -side),
            (corners[2], -up),
            (corners[3], -up),
            (corners[3], side),
            (corners[0], side),
        ];
        for (p, nm) in ring_data {
            verts.push(p.into());
            norms.push(nm.into());
            cols.push(s.color);
        }
    }

    for i in 0..(n - 1) {
        let r0 = base_v + (i * RING) as u32;
        let r1 = base_v + ((i + 1) * RING) as u32;
        for f in 0..4 {
            let a = (f * 2) as u32;
            let b = a + 1;
            tris.push([r0 + a, r0 + b, r1 + b]);
            tris.push([r0 + a, r1 + b, r1 + a]);
        }
    }

    // Start cap (corners 0,1,2,3) and end cap
    let push_cap = |pos: Vec3, hw: f32, ht: f32, t: Vec3, up_raw: Vec3, col: Color32, flip: bool,
                    verts: &mut Vec<[f32; 3]>, norms: &mut Vec<[f32; 3]>, tris: &mut Vec<[u32; 3]>, cols: &mut Vec<Color32>| {
        let mut up = up_raw - t * up_raw.dot(t);
        up = if up.length_squared() < 1e-5 { t.any_orthogonal_vector() } else { up.normalize() };
        let side = t.cross(up).normalize();
        let nm = if flip { -t } else { t };
        let base = verts.len() as u32;
        for c in [
            pos + side * hw + up * ht,
            pos - side * hw + up * ht,
            pos - side * hw - up * ht,
            pos + side * hw - up * ht,
        ] {
            verts.push(c.into());
            norms.push(nm.into());
            cols.push(col);
        }
        if flip {
            tris.push([base, base + 2, base + 1]);
            tris.push([base, base + 3, base + 2]);
        } else {
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);
        }
    };

    push_cap(spine[0].pos, 0.5 * spine[0].width.max(0.04), 0.5 * spine[0].thickness.max(0.04), tangents[0], spine[0].up, spine[0].color, true, verts, norms, tris, cols);
    let last = &spine[n - 1];
    push_cap(last.pos, 0.5 * last.width.max(0.04), 0.5 * last.thickness.max(0.04), tangents[n - 1], last.up, last.color, false, verts, norms, tris, cols);
}
