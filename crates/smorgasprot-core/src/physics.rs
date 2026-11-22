//! Biophysical contact scoring and local chemical environment density heuristics:
//! van der Waals clashes (MolProbity / Probe), hydrogen bonds (Baker–Hubbard / HBPLUS),
//! salt bridges, buried unsatisfied polars, and local aliphatic packing density (ChimeraX Fermi MLP).
//!
//! ## Works referenced
//! 1. **All-Atom Steric Clash Tolerances (`Probe` / `MolProbity` / `Phenix`):**
//!    Word, J. M., Lovell, S. C., LaBean, T. H., Taylor, H. C., Zalis, M. E., Presley, B. K.,
//!    Richardson, J. S., & Richardson, D. C. (1999). Visualizing and quantifying molecular
//!    goodness-of-fit: small-probe contact dots with explicit hydrogen atoms.
//!    *Journal of Molecular Biology*, 285(4), 1711–1733; Chen, V. B. et al. (2010).
//!    MolProbity: all-atom structure validation for macromolecular crystallography.
//!    *Acta Crystallographica Section D*, 66(1), 12–21.
//! 2. **Hydrogen Bond Geometric & Angular Criteria (Baker–Hubbard / McDonald–Thornton):**
//!    Baker, E. N., & Hubbard, R. E. (1984). Hydrogen bonding in globular proteins.
//!    *Progress in Biophysics and Molecular Biology*, 44(2), 97–179; McDonald, I. K., &
//!    Thornton, J. M. (1994). Satisfying hydrogen bonding potential in proteins (`HBPLUS`).
//!    *Journal of Molecular Biology*, 238(5), 777–793.
//! 3. **Molecular Lipophilicity Potential (MLP) & ChimeraX Fermi Distance-Decay Kernel:**
//!    Audry, E. et al. (1986). *Eur. J. Med. Chem.*, 21, 71–72; Gaillard, P., Carrupt, P.-A.,
//!    Testa, B., & Boudon, A. (1994). Molecular Lipophilicity Potential, a tool in 3D QSAR:
//!    Method and applications. *Journal of Computer-Aided Molecular Design*, 8(2), 83–96;
//!    Laguerre, M. et al. (1997). *Journal of Pharmaceutical Sciences*, 86(10), 1174–1182.
//! 4. **Protein Interface Hydrophobic Hotspots ("O-Ring" Architecture):**
//!    Bogan, A. A., & Thorn, K. S. (1998). Anatomy of hot spots in protein interfaces.
//!    *Journal of Molecular Biology*, 280(1), 1–9.

use crate::amino_acids::{ChemistryClass, ElementExt, print_three_letter_code};
use crate::interface::model::{
    AtomInteraction, AtomNode, AtomNodeIdx, EdgeKind, InterfaceEdge, ResidueTerritory, TerritoryIdx,
};
use crate::model::{
    AtomRecord, ComplexStructure, Element, GlobalResidueIdx, InterfaceSide, ResidueRecord, SecStruct,
};
use crate::sasa::{SasaParams, compute_complex_sasa, is_interfacial_atom};
use crate::spatial::SpatialGrid;
use glam::Vec3;
use std::collections::HashMap;

crate::param_group! {
    /// Distance and angle criteria for contacts, clashes, H-bonds, and salt bridges.
    pub struct ContactParams("contacts", "Contacts & Interactions") {
        /// First-shell neighbor cutoff for springs and contacts (Å).
        first_shell_cutoff_a: f32 = 4.5, 3.0..=8.0, "First-shell neighbor cutoff", "Å";
        /// Allowed van der Waals overlap before a pair counts as a clash (Å).
        clash_tolerance_a: f32 = 0.40, 0.0..=1.5, "vdW clash tolerance", "Å";
        /// Allowed overlap for H-bonded polar pairs before a clash (Å).
        hbond_clash_tolerance_a: f32 = 0.65, 0.0..=1.5, "H-bond clash tolerance", "Å";
        /// Minimum donor–acceptor heavy-atom distance for an H-bond (Å).
        hbond_min_dist_a: f32 = 2.35, 2.0..=3.0, "H-bond min distance", "Å";
        /// Maximum donor–acceptor heavy-atom distance for an H-bond (Å).
        hbond_max_dist_a: f32 = 3.50, 2.8..=4.5, "H-bond max distance", "Å";
        /// Minimum antecedent–donor–acceptor angle for an H-bond (degrees).
        hbond_min_antecedent_angle_deg: f32 = 100.0, 60.0..=180.0, "H-bond min angle", "°";
        /// Maximum distance between opposite charge centers for a salt bridge (Å).
        salt_bridge_max_dist_a: f32 = 4.0, 3.0..=6.0, "Salt bridge max distance", "Å";
        /// Neighborhood radius for the aliphatic packing score (Å).
        aliphatic_env_radius_a: f32 = 8.0, 4.0..=12.0, "Aliphatic neighborhood radius", "Å";
        /// Half-weight distance of the Fermi packing kernel (Å).
        fermi_half_dist_a: f32 = 4.5, 2.0..=8.0, "Fermi kernel half-distance", "Å";
        /// Steepness of the Fermi packing kernel (1/Å).
        fermi_steepness_inv_a: f32 = 1.5, 0.1..=5.0, "Fermi kernel steepness", "1/Å";
        /// A polar atom without H-bond or salt bridge is unsatisfied if it buries more than this (Å²) ...
        unsat_polar_min_buried_a2: f32 = 2.0, 0.0..=20.0, "Unsat polar min buried area", "Å²";
        /// ... and keeps less than this solvent-exposed area in the complex (Å²).
        unsat_polar_max_exposed_a2: f32 = 1.5, 0.0..=20.0, "Unsat polar max exposed area", "Å²";
    }
}

/// Unified biophysical classifier for any pair of interfacial heavy atoms (same- or cross-side).
///
/// `base_a` and `base_b` are the optional 3D coordinates of the covalently bonded antecedent ("base")
/// heavy atoms attached to `atom_a` and `atom_b`, used to enforce the Baker & Hubbard (1984) angular
/// criterion on hydrogen bonds.
///
/// Returns `Some((EdgeKind, overlap_a))` if the atoms lie within `p.first_shell_cutoff_a` (default `4.5 A`).
pub fn classify_pair_edge(
    atom_a: &AtomRecord,
    res_a: &ResidueRecord,
    base_a: Option<Vec3>,
    atom_b: &AtomRecord,
    res_b: &ResidueRecord,
    base_b: Option<Vec3>,
    dist_a: f32,
    p: &ContactParams,
) -> Option<(EdgeKind, f32)> {
    if dist_a > p.first_shell_cutoff_a {
        return None;
    }

    if atom_a.residue_idx == atom_b.residue_idx {
        return Some((EdgeKind::SameResidue, 0.0));
    }

    if is_adjacent_backbone_pair(res_a, res_b) {
        return Some((EdgeKind::VanDerWaalsContact, 0.0));
    }

    if let Some(overlap_a) = check_steric_clash(atom_a, atom_b, dist_a, p) {
        return Some((EdgeKind::Clash, overlap_a));
    }

    if is_salt_bridge(atom_a, res_a, atom_b, res_b, dist_a, p) {
        return Some((EdgeKind::SaltBridge, 0.0));
    }

    if is_hydrogen_bond_baker_hubbard(atom_a, base_a, atom_b, base_b, dist_a, p) {
        return Some((EdgeKind::HBond, 0.0));
    }

    Some((EdgeKind::VanDerWaalsContact, 0.0))
}

/// Precomputes the 3D position of each atom's closest covalently bonded antecedent ("base")
/// heavy atom within the same residue (`1.10 A <= d <= 1.68 A`).
///
/// Examples:
/// - For `Ser OG`, the covalent base is `CB`.
/// - For `Glu OE1/OE2`, the covalent base is `CD`.
/// - For backbone `O`, the covalent base is `C`; for backbone `N`, the covalent base is `CA`.
pub fn precompute_covalent_base_positions(complex: &ComplexStructure) -> Vec<Option<Vec3>> {
    let mut bases = vec![None; complex.atoms.len()];

    for res in &complex.residues {
        let start = res.atom_range.0 as usize;
        let end = res.atom_range.1 as usize;
        for i in start..end {
            let pi = Vec3::from(complex.atoms[i].pos_3d);
            let mut best_base = None;
            let mut best_d2 = f32::INFINITY;

            for j in start..end {
                if i == j {
                    continue;
                }
                let pj = Vec3::from(complex.atoms[j].pos_3d);
                let d2 = pi.distance_squared(pj);
                if (1.10 * 1.10..=1.72 * 1.72).contains(&d2) && d2 < best_d2 {
                    best_d2 = d2;
                    best_base = Some(pj);
                }
            }
            bases[i] = best_base;
        }
    }

    bases
}

/// Returns true if `res_a` and `res_b` are adjacent covalently linked polypeptide residues (`|Δseq| <= 1`) on the same chain.
#[inline]
pub fn is_adjacent_backbone_pair(res_a: &ResidueRecord, res_b: &ResidueRecord) -> bool {
    res_a.chain_idx == res_b.chain_idx
        && (res_a.seq_num - res_b.seq_num).abs() <= 1
        && res_a.kind.is_polypeptide()
        && res_b.kind.is_polypeptide()
}

/// Checks if two non-bonded heavy atoms violate van der Waals steric exclusion (Word et al. 1999).
/// Returns `Some(overlap_a)` when the penetration exceeds the allowed tolerance.
#[inline]
pub fn check_steric_clash(
    atom_a: &AtomRecord,
    atom_b: &AtomRecord,
    dist_a: f32,
    p: &ContactParams,
) -> Option<f32> {
    let is_hbond_pair = atom_a.element.is_h_bond_partner() && atom_b.element.is_h_bond_partner();
    let tol = if is_hbond_pair {
        p.hbond_clash_tolerance_a
    } else {
        p.clash_tolerance_a
    };
    let overlap_a = (atom_a.vdw_radius_a + atom_b.vdw_radius_a - tol) - dist_a;
    if overlap_a > f32::EPSILON {
        Some(overlap_a)
    } else {
        None
    }
}

/// Returns true if `atom_a` and `atom_b` carry opposite formal charges within `p.salt_bridge_max_dist_a`.
#[inline]
pub fn is_salt_bridge(
    atom_a: &AtomRecord,
    res_a: &ResidueRecord,
    atom_b: &AtomRecord,
    res_b: &ResidueRecord,
    dist_a: f32,
    p: &ContactParams,
) -> bool {
    dist_a <= p.salt_bridge_max_dist_a && is_opposite_charged_atoms(atom_a, res_a, atom_b, res_b)
}

/// Baker & Hubbard (1984) / McDonald & Thornton (1994) heavy-atom hydrogen bond detector.
///
/// Requires:
/// 1. Both atoms are polar N/O heavy atoms (`ElementExt::is_h_bond_partner`),
/// 2. Donor–acceptor distance 2.35 A < dA < 3.50 A, and
/// 3. Antecedent covalent bond angles angle(BaseA-A...B) >= 100° and angle(A...B-BaseB) >= 100°
///    (when covalent base atoms are present).
pub fn is_hydrogen_bond_baker_hubbard(
    atom_a: &AtomRecord,
    base_a: Option<Vec3>,
    atom_b: &AtomRecord,
    base_b: Option<Vec3>,
    dist_a: f32,
    p: &ContactParams,
) -> bool {
    if !atom_a.element.is_h_bond_partner()
        || !atom_b.element.is_h_bond_partner()
        || !(p.hbond_min_dist_a..=p.hbond_max_dist_a).contains(&dist_a)
    {
        return false;
    }

    let pa = Vec3::from(atom_a.pos_3d);
    let pb = Vec3::from(atom_b.pos_3d);
    // cos(100 deg) = -0.173648
    let max_cos = (p.hbond_min_antecedent_angle_deg.to_radians()).cos();

    if let Some(ba) = base_a {
        let u_ba = (ba - pa).normalize_or_zero();
        let u_ab = (pb - pa).normalize_or_zero();
        // Angle at A is obtuse (>= 100 deg) iff dot(u_ba, u_ab) <= cos(100 deg)
        if u_ba.dot(u_ab) > max_cos {
            return false;
        }
    }

    if let Some(bb) = base_b {
        let u_bb = (bb - pb).normalize_or_zero();
        let u_ba = (pa - pb).normalize_or_zero();
        if u_bb.dot(u_ba) > max_cos {
            return false;
        }
    }

    true
}

fn is_positive_sidechain_atom(atom: &AtomRecord, res: &ResidueRecord) -> bool {
    if let Some(aa) = res.kind.amino_acid() {
        let aname = std::str::from_utf8(&atom.atom_name).unwrap_or("");
        aa.is_positive_functional_atom(aname)
    } else {
        atom.element.is_metal_ion()
    }
}

fn is_negative_sidechain_atom(atom: &AtomRecord, res: &ResidueRecord) -> bool {
    if let Some(aa) = res.kind.amino_acid() {
        let aname = std::str::from_utf8(&atom.atom_name).unwrap_or("");
        aa.is_negative_functional_atom(aname)
    } else {
        false
    }
}

fn is_opposite_charged_atoms(
    atom_a: &AtomRecord,
    res_a: &ResidueRecord,
    atom_b: &AtomRecord,
    res_b: &ResidueRecord,
) -> bool {
    (is_positive_sidechain_atom(atom_a, res_a) && is_negative_sidechain_atom(atom_b, res_b))
        || (is_negative_sidechain_atom(atom_a, res_a) && is_positive_sidechain_atom(atom_b, res_b))
}

/// Returns the intrinsic apolar/aliphatic weight $w_j \in [0, 1]$ of an atom for local hydrophobic
/// packing density calculations:
/// - Sidechain carbon/sulfur on an Aliphatic residue (`Ala, Val, Leu, Ile, Met`): `1.0`
/// - Sidechain carbon on an Aromatic or Proline residue (`Phe, Tyr, Trp, Pro`): `0.85`
/// - Non-carbonyl sidechain carbon on a polar/charged residue (e.g. `CB, CG` methylene stem): `0.5`
/// - Backbone `CA`: `0.25`
/// - Polar `N, O`, carbonyl `C`, or charged functional carbons (`CZ` of Arg, `CG` of Asp, `CD` of Glu): `0.0`
pub fn intrinsic_aliphatic_weight(atom: &AtomRecord, res: &ResidueRecord) -> f32 {
    if !matches!(atom.element, Element::C | Element::S) {
        return 0.0;
    }
    let aname = std::str::from_utf8(&atom.atom_name).unwrap_or("").trim();
    if aname == "C" {
        return 0.0; // Backbone carbonyl carbon is partially positive/polar
    }
    if aname == "CA" {
        return 0.25;
    }

    if let Some(aa) = res.kind.amino_acid() {
        match aa.chemistry_class() {
            ChemistryClass::Aliphatic => 1.0,
            ChemistryClass::Aromatic => {
                // Exclude Tyr phenolic CZ adjacent to OH
                if aa == crate::amino_acids::AminoAcid::Tyr && aname == "CZ" {
                    0.3
                } else {
                    0.85
                }
            }
            ChemistryClass::SpecialGlyPro => 0.85,
            ChemistryClass::PolarNeutral | ChemistryClass::Positive | ChemistryClass::Negative => {
                // Terminal functional carbons directly bonded to O/N carry partial charge
                if matches!(
                    (aa, aname),
                    (crate::amino_acids::AminoAcid::Asp, "CG")
                        | (crate::amino_acids::AminoAcid::Asn, "CG")
                        | (crate::amino_acids::AminoAcid::Glu, "CD")
                        | (crate::amino_acids::AminoAcid::Gln, "CD")
                        | (crate::amino_acids::AminoAcid::Arg, "CZ")
                ) {
                    0.0
                } else {
                    0.50
                }
            }
        }
    } else {
        0.50
    }
}

/// Evaluates the normalized Laguerre et al. (1997) / UCSF ChimeraX `mlp` Fermi distance-decay function:
/// (1 + exp(-a * d0)) / (1 + exp(a * (d - d0)))
/// where d0 = `p.fermi_half_dist_a` (default 4.50 A) and a = `p.fermi_steepness_inv_a` (default 1.50 A^-1).
#[inline]
pub fn fermi_distance_kernel(dist_a: f32, p: &ContactParams) -> f32 {
    let num = 1.0 + (-p.fermi_steepness_inv_a * p.fermi_half_dist_a).exp();
    let den = 1.0 + (p.fermi_steepness_inv_a * (dist_a - p.fermi_half_dist_a)).exp();
    num / den
}

/// Computes the local 3D aliphatic/hydrophobic packing density score for every atom in `complex`.
///
/// Combines non-negative apolar carbon/sulfur weights [`intrinsic_aliphatic_weight`] (wj in [0, 1])
/// with the exact **UCSF ChimeraX / Laguerre et al. (1997) Fermi distance-decay kernel** [`fermi_distance_kernel`]
/// over all neighboring non-same-residue apolar atoms within `p.aliphatic_env_radius_a`:
pub fn compute_aliphatic_packing_scores(complex: &ComplexStructure, p: &ContactParams) -> Vec<f32> {
    let n_atoms = complex.atoms.len();
    let mut scores = vec![0.0_f32; n_atoms];
    let mut weights = vec![0.0_f32; n_atoms];

    let mut grid = SpatialGrid::new(p.aliphatic_env_radius_a);
    for (i, atom) in complex.atoms.iter().enumerate() {
        if !atom.role.is_assigned() {
            continue;
        }
        let res = &complex.residues[atom.residue_idx.0 as usize];
        let w = intrinsic_aliphatic_weight(atom, res);
        weights[i] = w;
        if w > 0.0 {
            grid.insert(i as u32, atom.pos_3d);
        }
    }

    for (i, atom) in complex.atoms.iter().enumerate() {
        if !atom.role.is_assigned() {
            continue;
        }
        let mut env_sum = weights[i];
        let res_i = atom.residue_idx;

        grid.query_radius(atom.pos_3d, p.aliphatic_env_radius_a, |j_u32, _pos_j, d2| {
            let j = j_u32 as usize;
            if complex.atoms[j].residue_idx == res_i {
                return;
            }
            let dist = d2.sqrt();
            env_sum += weights[j] * fermi_distance_kernel(dist, p);
        });

        scores[i] = env_sum;
    }

    scores
}

/// Extracts the 3D interfacial atom nodes, residue groups, and classified 3D interaction edges
/// from `complex`.
pub fn extract_3d_interface_graph(
    complex: &ComplexStructure,
    sasa_p: &SasaParams,
    p: &ContactParams,
) -> Result<(Vec<ResidueTerritory>, Vec<AtomNode>, Vec<InterfaceEdge>), String> {
    let sasa_vec = compute_complex_sasa(complex, sasa_p);
    let aliphatic_scores = compute_aliphatic_packing_scores(complex, p);

    let multi_chain_a = complex.group_a_chains.len() > 1;
    let multi_chain_b = complex.group_b_chains.len() > 1;

    let mut res_to_territory: HashMap<GlobalResidueIdx, TerritoryIdx> = HashMap::new();
    let mut territories: Vec<ResidueTerritory> = Vec::new();
    let mut nodes: Vec<AtomNode> = Vec::new();

    for (a_idx, atom) in complex.atoms.iter().enumerate() {
        let side = atom.role;
        if !side.is_assigned() {
            continue;
        }
        let sasa = &sasa_vec[a_idx];
        if !is_interfacial_atom(side, sasa, sasa_p) {
            continue;
        }

        let terr_idx = *res_to_territory.entry(atom.residue_idx).or_insert_with(|| {
            let t_idx = TerritoryIdx(territories.len() as u32);
            let res = &complex.residues[atom.residue_idx.0 as usize];
            let title_name = print_three_letter_code(&res.res_name);
            let ins_str = if res.ins_code == ' ' {
                String::new()
            } else {
                res.ins_code.to_string()
            };
            let prefix_chain = match side {
                InterfaceSide::SideA => multi_chain_a,
                InterfaceSide::SideB => multi_chain_b,
                InterfaceSide::None => false,
            };
            let label_top = if prefix_chain {
                format!("{}:{title_name} {}{ins_str}", res.chain_id, res.seq_num)
            } else {
                format!("{title_name} {}{ins_str}", res.seq_num)
            };
            let label_bottom = match res.sec_struct {
                SecStruct::Helix => "helix".to_string(),
                SecStruct::Sheet => "sheet".to_string(),
                SecStruct::Loop => "loop".to_string(),
            };
            territories.push(ResidueTerritory {
                idx: t_idx,
                global_residue: atom.residue_idx,
                role: side,
                chain_id: res.chain_id.clone(),
                label_top,
                label_bottom,
                patch_atoms: Vec::new(),
                buried_sasa_a2: 0.0,
                has_unsatisfied_polar: false,
                contour_uv: Vec::new(),
                contour_3d: Vec::new(),
                label_center_uv: [0.0, 0.0],
                label_center_3d: [0.0, 0.0, 0.0],
                local_uv_anchor: [0.0, 0.0],
            });
            t_idx
        });

        let p_idx = AtomNodeIdx(nodes.len() as u32);
        territories[terr_idx.0 as usize].patch_atoms.push(p_idx);
        territories[terr_idx.0 as usize].buried_sasa_a2 += sasa.buried_sasa_a2;

        nodes.push(AtomNode {
            idx: p_idx,
            global_atom: atom.idx,
            territory: terr_idx,
            role: side,
            b_factor: atom.b_factor,
            free_sasa_a2: sasa.free_sasa_a2,
            complex_sasa_a2: sasa.complex_sasa_a2,
            buried_sasa_a2: sasa.buried_sasa_a2,
            aliphatic_score: aliphatic_scores[a_idx],
            pos_3d: atom.pos_3d,
            pos_uv_raw: [0.0, 0.0],
            pos_uv: [0.0, 0.0],
            is_unsatisfied_buried_polar: false,
            interactions: Vec::new(),
        });
    }

    if nodes.is_empty() {
        return Err("No interfacial atoms found between Side A and Side B chains".to_string());
    }

    // Classify all 3D first-shell pairs and populate vectorized node.interactions
    let covalent_bases = precompute_covalent_base_positions(complex);
    let mut edges = Vec::new();

    for i in 0..nodes.len() {
        let pi = Vec3::from(nodes[i].pos_3d);
        let g_i = nodes[i].global_atom.0 as usize;
        let atom_i = &complex.atoms[g_i];
        let res_i = &complex.residues[atom_i.residue_idx.0 as usize];
        let base_i = covalent_bases[g_i];

        for j in (i + 1)..nodes.len() {
            let pj = Vec3::from(nodes[j].pos_3d);
            let dist = pi.distance(pj);
            if dist > p.first_shell_cutoff_a {
                continue;
            }

            let g_j = nodes[j].global_atom.0 as usize;
            let atom_j = &complex.atoms[g_j];
            let res_j = &complex.residues[atom_j.residue_idx.0 as usize];
            let base_j = covalent_bases[g_j];

            let Some((kind, overlap_a)) =
                classify_pair_edge(atom_i, res_i, base_i, atom_j, res_j, base_j, dist, p)
            else {
                continue;
            };

            let is_cross = nodes[i].role != nodes[j].role;
            let p_i = AtomNodeIdx(i as u32);
            let p_j = AtomNodeIdx(j as u32);

            edges.push(InterfaceEdge {
                a: p_i,
                b: p_j,
                is_cross_interface: is_cross,
                kind,
                dist_3d_a: dist,
                dist_2d_a: dist,
                strain_2d_a: 0.0,
                overlap_a,
            });

            nodes[i].interactions.push(AtomInteraction {
                partner: p_j,
                is_cross_interface: is_cross,
                kind,
                dist_3d_a: dist,
                overlap_a,
            });
            nodes[j].interactions.push(AtomInteraction {
                partner: p_i,
                is_cross_interface: is_cross,
                kind,
                dist_3d_a: dist,
                overlap_a,
            });
        }
    }

    // Mark buried unsatisfied polar atoms (polar N/O buried at interface with 0 H-bonds or salt bridges)
    for node in &mut nodes {
        let atom = &complex.atoms[node.global_atom.0 as usize];
        let has_polar_satisfaction = node
            .interactions
            .iter()
            .any(|inter| matches!(inter.kind, EdgeKind::HBond | EdgeKind::SaltBridge));

        if atom.element.is_h_bond_partner()
            && node.buried_sasa_a2 > p.unsat_polar_min_buried_a2
            && node.complex_sasa_a2 < p.unsat_polar_max_exposed_a2
            && !has_polar_satisfaction
        {
            node.is_unsatisfied_buried_polar = true;
            territories[node.territory.0 as usize].has_unsatisfied_polar = true;
        }
    }

    Ok((territories, nodes, edges))
}
