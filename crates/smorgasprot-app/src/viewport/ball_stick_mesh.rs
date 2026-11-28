//! Covalent Ball-and-Stick geometry builder (atom selection masks, split/gradient bond cylinders,
//! double/triple/aromatic bond multiplicity, and 2D/3D impostor or geodesic atom spheres).

use crate::dataset::ViewerDataset;
use crate::render::{GpuDiscDraw, GpuLineDraw};
use crate::state::{
    AtomDrawParams, BFactorMode, BallStickQuality, BallStickRepStyle, BallStickScope, BondColorMode, LensParams,
};
use crate::style::{Palette, lenses, sizing};
use egui::Color32;
use glam::Vec3;
use smorgasprot_core::surface::{GeodesicPreset, unit_geodesic_sphere};
use smorgasprot_core::{BondKind, BondOrder, ComplexStructure, InterfaceSide, SurfaceMesh};

pub struct ChainBallStickMeshes {
    pub chain_idx: usize,
    pub bond_mesh: SurfaceMesh,
    pub bond_colors: Vec<Color32>,
    pub sphere_mesh: Option<(SurfaceMesh, Vec<Color32>)>,
}

fn is_polypeptide_mainchain_non_ca(atom_name: &[u8; 4]) -> bool {
    let n = std::str::from_utf8(atom_name).unwrap_or("").trim();
    matches!(n, "N" | "C" | "O" | "OXT")
}

/// Computes which atoms in `complex` are included by `scope`.
pub fn select_ball_stick_atoms(ds: &ViewerDataset, complex: &ComplexStructure, scope: BallStickScope) -> Vec<bool> {
    if scope == BallStickScope::AllAtoms {
        return vec![true; complex.atoms.len()];
    }
    let need_iface = matches!(scope, BallStickScope::InterfaceSidechains | BallStickScope::InterfaceResidues);
    let sidechains_only = matches!(scope, BallStickScope::SidechainsAndLigands | BallStickScope::InterfaceSidechains);

    let mut iface_res = vec![false; complex.residues.len()];
    if need_iface {
        if let Some(iface) = ds.active_interface() {
            for node in &iface.map.nodes {
                if let Some(atom) = complex.atoms.get(node.global_atom.0 as usize)
                    && let Some(slot) = iface_res.get_mut(atom.residue_idx.0 as usize)
                {
                    *slot = true;
                }
            }
        } else {
            return vec![false; complex.atoms.len()];
        }
    }

    let mut mask = vec![false; complex.atoms.len()];
    for res in &complex.residues {
        if need_iface && !iface_res[res.idx.0 as usize] {
            continue;
        }
        let range = res.atom_range.0 as usize..res.atom_range.1 as usize;
        if !sidechains_only || !res.kind.is_polypeptide() {
            for idx in range {
                mask[idx] = true;
            }
            continue;
        }
        // Polypeptide sidechain mode: include sidechain atoms + CA anchor if at least one sidechain atom exists.
        let has_sidechain = complex.atoms[range.clone()]
            .iter()
            .any(|a| !is_polypeptide_mainchain_non_ca(&a.atom_name) && &a.atom_name != b"CA  ");
        if !has_sidechain {
            continue;
        }
        for idx in range {
            if !is_polypeptide_mainchain_non_ca(&complex.atoms[idx].atom_name) {
                mask[idx] = true;
            }
        }
    }
    mask
}

/// Appends a single uncapped or capped 8-sided cylinder segment between `pa` and `pb`.
pub(super) fn push_cylinder_segment(
    pa: Vec3,
    pb: Vec3,
    radius: f32,
    col_a: Color32,
    col_b: Color32,
    sides: usize,
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) {
    let axis = pb - pa;
    let len = axis.length();
    if len < 1e-4 || radius <= 0.0 {
        return;
    }
    let dir = axis / len;
    let u = dir.any_orthogonal_vector();
    let v = dir.cross(u);
    let base = verts.len() as u32;

    for &(p, col) in &[(pa, col_a), (pb, col_b)] {
        for s in 0..sides {
            let (sa, ca) = ((s as f32 / sides as f32) * std::f32::consts::TAU).sin_cos();
            let n = (u * ca + v * sa).normalize();
            verts.push((p + n * radius).into());
            norms.push(n.into());
            cols.push(col);
        }
    }

    let r0 = base;
    let r1 = base + sides as u32;
    for s in 0..sides {
        let sn = (s + 1) % sides;
        tris.push([r0 + s as u32, r0 + sn as u32, r1 + sn as u32]);
        tris.push([r0 + s as u32, r1 + sn as u32, r1 + s as u32]);
    }
}

fn push_colored_bond_3d(
    pa: Vec3,
    pb: Vec3,
    radius: f32,
    col_a: Color32,
    col_b: Color32,
    mode: BondColorMode,
    fixed_col: Color32,
    verts: &mut Vec<[f32; 3]>,
    norms: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    cols: &mut Vec<Color32>,
) {
    const SIDES: usize = 8;
    match mode {
        BondColorMode::Uniform => {
            push_cylinder_segment(pa, pb, radius, fixed_col, fixed_col, SIDES, verts, norms, tris, cols);
        }
        BondColorMode::Gradient => {
            push_cylinder_segment(pa, pb, radius, col_a, col_b, SIDES, verts, norms, tris, cols);
        }
        BondColorMode::SplitMidpoint => {
            let mid = 0.5 * (pa + pb);
            push_cylinder_segment(pa, mid, radius, col_a, col_a, SIDES, verts, norms, tris, cols);
            push_cylinder_segment(mid, pb, radius, col_b, col_b, SIDES, verts, norms, tris, cols);
        }
    }
}

fn build_sphere_mesh(atoms: &[(Vec3, f32, Color32)], preset: GeodesicPreset) -> (SurfaceMesh, Vec<Color32>) {
    let (unit_verts, unit_tris) = unit_geodesic_sphere(preset);
    let mut vertices = Vec::with_capacity(atoms.len() * unit_verts.len());
    let mut normals = Vec::with_capacity(atoms.len() * unit_verts.len());
    let mut triangles = Vec::with_capacity(atoms.len() * unit_tris.len());
    let mut colors = Vec::with_capacity(atoms.len() * unit_verts.len());

    for &(c, r, col) in atoms {
        let base = vertices.len() as u32;
        for &uv in &unit_verts {
            let n = Vec3::new(uv.x, uv.y, uv.z);
            vertices.push((c + n * r).into());
            normals.push([n.x, n.y, n.z]);
            colors.push(col);
        }
        for &[i0, i1, i2] in &unit_tris {
            triangles.push([base + i0, base + i1, base + i2]);
        }
    }
    (SurfaceMesh { vertices, normals, triangles, nearest_atom: Vec::new() }, colors)
}

/// Evaluates Ball-and-Stick 3D meshes and/or 2D/impostor draws across all chains.
#[allow(clippy::too_many_arguments)]
pub fn build_ball_stick_scene(
    ds: &ViewerDataset,
    rep: &BallStickRepStyle,
    master_alpha: f32,
    zoom: f32,
    bfactor_mode: BFactorMode,
    atom_draw: &AtomDrawParams,
    lenses_params: &LensParams,
    pal: &Palette,
    to_local_with_offset: impl Fn(usize, [f32; 3]) -> Vec3,
    line_draws: &mut Vec<GpuLineDraw>,
    disc_draws: &mut Vec<GpuDiscDraw>,
) -> Vec<ChainBallStickMeshes> {
    let Some(complex) = &ds.complex else {
        return Vec::new();
    };
    let mask = select_ball_stick_atoms(ds, complex, rep.params.scope);
    if !mask.iter().any(|&b| b) {
        return Vec::new();
    }

    let iface_opt = ds.active_interface();
    let mut iface_lookup = vec![None; complex.atoms.len()];
    if let Some(iface) = iface_opt {
        for (ni, node) in iface.map.nodes.iter().enumerate() {
            if let Some(slot) = iface_lookup.get_mut(node.global_atom.0 as usize) {
                *slot = Some(ni);
            }
        }
    }

    // Precompute per-atom color and radius (Å) for selected atoms.
    let mut atom_colors = vec![Color32::TRANSPARENT; complex.atoms.len()];
    let mut atom_radii_a = vec![0.0_f32; complex.atoms.len()];
    for atom in &complex.atoms {
        let idx = atom.idx.0 as usize;
        if !mask[idx] {
            continue;
        }
        let chain_idx = atom.chain_idx.0 as usize;
        let side = ds
            .chains
            .get(chain_idx)
            .map_or(InterfaceSide::None, |ch| ds.chain_side(&ch.chain_id));
        let node = iface_lookup[idx].and_then(|ni| iface_opt.map(|i| &i.map.nodes[ni]));
        let res_name = complex
            .residues
            .get(atom.residue_idx.0 as usize)
            .and_then(|r| std::str::from_utf8(&r.res_name).ok())
            .unwrap_or("UNK");
        atom_colors[idx] = lenses::complex_atom_color(
            atom,
            res_name,
            chain_idx,
            side,
            node,
            rep.atom_color.lens,
            rep.atom_color.fixed,
            bfactor_mode,
            lenses_params,
            pal,
        );
        let scale = sizing::complex_atom_scale(
            rep.atom_scale.lens,
            rep.atom_scale.fixed,
            atom,
            node,
            bfactor_mode,
            lenses_params,
        );
        atom_radii_a[idx] = (atom.vdw_radius_a * scale).max(0.06);
    }

    let quality = rep.presentation.quality;
    let pres = &rep.presentation;
    let p = &rep.params;
    let fill_alpha = master_alpha * pres.alphas.fill;
    let border_pt = pres.border.effective_width_px(zoom);
    let border_alpha = master_alpha * pres.alphas.border;
    let sidechain_filter = matches!(p.scope, BallStickScope::SidechainsAndLigands | BallStickScope::InterfaceSidechains);

    let chain_count = ds.chains.len().max(1);
    let mut bond_verts: Vec<Vec<[f32; 3]>> = vec![Vec::new(); chain_count];
    let mut bond_norms: Vec<Vec<[f32; 3]>> = vec![Vec::new(); chain_count];
    let mut bond_tris: Vec<Vec<[u32; 3]>> = vec![Vec::new(); chain_count];
    let mut bond_cols: Vec<Vec<Color32>> = vec![Vec::new(); chain_count];
    let mut sphere_lists: Vec<Vec<(Vec3, f32, Color32)>> = vec![Vec::new(); chain_count];

    // Emit atoms (2D discs, 3D impostor spheres, or 3D geodesic mesh lists).
    if rep.atom_color.lens != crate::state::ColorLens::Off && rep.atom_scale.lens != crate::state::AtomSizeMode::Off {
        for atom in &complex.atoms {
            let idx = atom.idx.0 as usize;
            if !mask[idx] {
                continue;
            }
            let ci = (atom.chain_idx.0 as usize).min(chain_count - 1);
            let r_a = atom_radii_a[idx];
            let col = atom_colors[idx];
            if quality.geodesic_preset().is_some() {
                sphere_lists[ci].push((Vec3::from(atom.pos_3d), r_a, col));
            } else {
                let local_pos = to_local_with_offset(ci, atom.pos_3d);
                let radius_pt = (r_a * zoom).max(atom_draw.min_atom_radius_px * 0.5);
                disc_draws.push(GpuDiscDraw {
                    center: local_pos,
                    radius_pt,
                    outline_pt: border_pt,
                    fill_color: col,
                    fill_alpha,
                    outline_color: pres.border.color,
                    outline_alpha: border_alpha,
                    shaded_3d: quality == BallStickQuality::Impostor3D,
                });
            }
        }
    }

    // Emit covalent bonds.
    for bond in &complex.bonds {
        let (ia, ib) = (bond.a.0 as usize, bond.b.0 as usize);
        if !mask[ia] || !mask[ib] {
            continue;
        }
        if sidechain_filter && bond.kind == BondKind::Backbone {
            continue;
        }
        let atom_a = &complex.atoms[ia];
        let atom_b = &complex.atoms[ib];
        let ca = (atom_a.chain_idx.0 as usize).min(chain_count - 1);
        let cb = (atom_b.chain_idx.0 as usize).min(chain_count - 1);
        let col_a = atom_colors[ia];
        let col_b = atom_colors[ib];

        if quality == BallStickQuality::Flat2D {
            let la = to_local_with_offset(ca, atom_a.pos_3d);
            let lb = to_local_with_offset(cb, atom_b.pos_3d);
            let axis = lb - la;
            let perp = axis.normalize_or(Vec3::X).any_orthogonal_vector();
            let push_2d = |line_draws: &mut Vec<GpuLineDraw>, a: Vec3, b: Vec3, rad_a: f32, dash: Option<[f32; 2]>| {
                let width_pt = (2.0 * rad_a * zoom).max(1.2);
                match p.bond_color_mode {
                    BondColorMode::SplitMidpoint => {
                        let mid = 0.5 * (a + b);
                        for (s0, s1, c, prv, nxt) in [(a, mid, col_a, None, Some(b)), (mid, b, col_b, Some(a), None)] {
                            line_draws.push(GpuLineDraw {
                                a: s0,
                                b: s1,
                                prev: prv,
                                next: nxt,
                                color: c,
                                alpha: fill_alpha,
                                width_pt,
                                dash,
                                border_pt,
                                border_color: pres.border.color,
                                border_alpha,
                            });
                        }
                    }
                    BondColorMode::Gradient | BondColorMode::Uniform => {
                        let c = if p.bond_color_mode == BondColorMode::Uniform {
                            p.bond_color
                        } else {
                            crate::style::palette::lerp_rgb(col_a, col_b, 0.5)
                        };
                        line_draws.push(GpuLineDraw {
                            a,
                            b,
                            prev: None,
                            next: None,
                            color: c,
                            alpha: fill_alpha,
                            width_pt,
                            dash,
                            border_pt,
                            border_color: pres.border.color,
                            border_alpha,
                        });
                    }
                }
            };

            let r = p.bond_radius_a;
            match bond.order {
                BondOrder::Double if p.show_double_bonds => {
                    let off = perp * (0.85 * r);
                    push_2d(line_draws, la + off, lb + off, 0.58 * r, None);
                    push_2d(line_draws, la - off, lb - off, 0.58 * r, None);
                }
                BondOrder::Triple if p.show_double_bonds => {
                    let off = perp * (1.15 * r);
                    push_2d(line_draws, la + off, lb + off, 0.46 * r, None);
                    push_2d(line_draws, la, lb, 0.46 * r, None);
                    push_2d(line_draws, la - off, lb - off, 0.46 * r, None);
                }
                BondOrder::Aromatic if p.show_aromatic_rings => {
                    let off = perp * (1.05 * r);
                    push_2d(line_draws, la, lb, 0.72 * r, None);
                    push_2d(line_draws, la.lerp(lb, 0.15) + off, la.lerp(lb, 0.85) + off, 0.48 * r, Some([3.0, 2.5]));
                }
                _ => push_2d(line_draws, la, lb, r, None),
            }
        } else {
            if ca != cb {
                // Skip cross-chain 3D mesh bonds when chains explode independently unless co-located.
                let delta = to_local_with_offset(cb, [0.0; 3]) - to_local_with_offset(ca, [0.0; 3]);
                if delta.length_squared() > 1e-4 {
                    continue;
                }
            }
            let pa = Vec3::from(atom_a.pos_3d);
            let pb = Vec3::from(atom_b.pos_3d);
            let perp = (pb - pa).normalize_or(Vec3::X).any_orthogonal_vector();
            let r = p.bond_radius_a;
            let (vs, ns, ts, cs) = (&mut bond_verts[ca], &mut bond_norms[ca], &mut bond_tris[ca], &mut bond_cols[ca]);

            match bond.order {
                BondOrder::Double if p.show_double_bonds => {
                    let off = perp * (0.85 * r);
                    push_colored_bond_3d(pa + off, pb + off, 0.58 * r, col_a, col_b, p.bond_color_mode, p.bond_color, vs, ns, ts, cs);
                    push_colored_bond_3d(pa - off, pb - off, 0.58 * r, col_a, col_b, p.bond_color_mode, p.bond_color, vs, ns, ts, cs);
                }
                BondOrder::Triple if p.show_double_bonds => {
                    let off = perp * (1.15 * r);
                    for k in [-1.0_f32, 0.0, 1.0] {
                        push_colored_bond_3d(pa + off * k, pb + off * k, 0.46 * r, col_a, col_b, p.bond_color_mode, p.bond_color, vs, ns, ts, cs);
                    }
                }
                BondOrder::Aromatic if p.show_aromatic_rings => {
                    let off = perp * (1.05 * r);
                    push_colored_bond_3d(pa, pb, 0.72 * r, col_a, col_b, p.bond_color_mode, p.bond_color, vs, ns, ts, cs);
                    for (t0, t1) in [(0.16, 0.34), (0.42, 0.58), (0.66, 0.84)] {
                        push_colored_bond_3d(
                            pa.lerp(pb, t0) + off,
                            pa.lerp(pb, t1) + off,
                            0.46 * r,
                            col_a,
                            col_b,
                            p.bond_color_mode,
                            p.bond_color,
                            vs,
                            ns,
                            ts,
                            cs,
                        );
                    }
                }
                _ => {
                    push_colored_bond_3d(pa, pb, r, col_a, col_b, p.bond_color_mode, p.bond_color, vs, ns, ts, cs);
                }
            }
        }
    }

    let mut out = Vec::new();
    for ci in 0..chain_count {
        let sphere_mesh = quality
            .geodesic_preset()
            .filter(|_| !sphere_lists[ci].is_empty())
            .map(|preset| build_sphere_mesh(&sphere_lists[ci], preset));
        if bond_tris[ci].is_empty() && sphere_mesh.is_none() {
            continue;
        }
        out.push(ChainBallStickMeshes {
            chain_idx: ci,
            bond_mesh: SurfaceMesh {
                vertices: std::mem::take(&mut bond_verts[ci]),
                normals: std::mem::take(&mut bond_norms[ci]),
                triangles: std::mem::take(&mut bond_tris[ci]),
                nearest_atom: Vec::new(),
            },
            bond_colors: std::mem::take(&mut bond_cols[ci]),
            sphere_mesh,
        });
    }
    out
}
