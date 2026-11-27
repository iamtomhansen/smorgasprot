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
use crate::interface::definition::{InterfacePartition, InterfaceSide};
use crate::interface::model::{
    AtomInteraction, AtomNode, AtomNodeIdx, EdgeKind, InterfaceEdge, ResidueTerritory, TerritoryIdx,
};
use crate::model::{
    AtomRecord, BondKind, ComplexStructure, Element, GlobalResidueIdx, ResidueRecord, SecStruct,
};
use crate::sasa::{SasaParams, compute_complex_sasa};
use crate::spatial::SpatialGrid;
use glam::Vec3;

use crate::interactions::h_bonds::is_hydrogen_bond_baker_hubbard;
use crate::interactions::salt_bridges::is_salt_bridge;
use crate::interactions::vdw_contacts::check_steric_clash;
use std::collections::HashMap;

crate::param_group! {
    /// Distance and angle criteria for contacts, clashes, H-bonds, and salt bridges.
    pub struct ContactParams("contacts", "Contacts & interactions") {
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
/// criterion on hydrogen bonds. `bonded` is the actual `BondKind` `complex.bonds`
/// (`bonds::detect_bonds`) found for this pair, if any — checked before non-covalent
/// classification, so e.g. a disulfide (~2.05 Å S-S) is never mistaken for a steric clash. This
/// reuses `BondKind` directly rather than re-deriving an equivalent classification here.
///
/// `one_bond_removed` is whether the pair shares a common bonded neighbor (1-3, e.g. backbone
/// N...C across Cα) without being bonded to each other directly — Probe/MolProbity (Word et al.
/// 1999, cited above) excludes both 1-2 *and* 1-3 pairs from clash/polar scoring, since they're
/// geometrically constrained by bond lengths and angles, not independent non-bonded contacts.
/// Skipping this would flag ordinary bonded-angle geometry as a severe steric clash.
///
/// Returns `Some((EdgeKind, overlap_a))` if the atoms lie within `p.first_shell_cutoff_a` (default `4.5 A`).
#[allow(clippy::too_many_arguments)]
pub fn classify_pair_edge(
    atom_a: &AtomRecord,
    res_a: &ResidueRecord,
    base_a: Option<Vec3>,
    atom_b: &AtomRecord,
    res_b: &ResidueRecord,
    base_b: Option<Vec3>,
    dist_a: f32,
    bonded: Option<BondKind>,
    one_bond_removed: bool,
    p: &ContactParams,
) -> Option<(EdgeKind, f32)> {
    if dist_a > p.first_shell_cutoff_a {
        return None;
    }

    if let Some(kind) = bonded {
        return Some((EdgeKind::Covalent(kind), 0.0));
    }

    if !one_bond_removed {
        if let Some(overlap_a) = check_steric_clash(atom_a, atom_b, dist_a, p) {
            return Some((EdgeKind::Clash, overlap_a));
        }

        if is_salt_bridge(atom_a, res_a, atom_b, res_b, dist_a, p) {
            return Some((EdgeKind::SaltBridge, 0.0));
        }

        if is_hydrogen_bond_baker_hubbard(atom_a, base_a, atom_b, base_b, dist_a, p) {
            return Some((EdgeKind::HBond, 0.0));
        }
    }

    Some((EdgeKind::VanDerWaalsContact, 0.0))
}

/// Precomputes the 3D position of each atom's closest covalently bonded antecedent ("base")
/// heavy atom within the same residue, derived from `complex.bonds` (filtered to same-residue
/// pairs directly — `BondKind::Backbone` alone isn't enough, since it also covers the
/// inter-residue peptide link).
///
/// Examples:
/// - For `Ser OG`, the covalent base is `CB`.
/// - For `Glu OE1/OE2`, the covalent base is `CD`.
/// - For backbone `O`, the covalent base is `C`; for backbone `N`, the covalent base is `CA`.
pub fn precompute_covalent_base_positions(complex: &ComplexStructure) -> Vec<Option<Vec3>> {
    let mut bases: Vec<Option<(Vec3, f32)>> = vec![None; complex.atoms.len()];
    let pos = |idx: u32| Vec3::from(complex.atoms[idx as usize].pos_3d);

    let mut consider = |i: u32, j: u32| {
        let (pi, pj) = (pos(i), pos(j));
        let d2 = pi.distance_squared(pj);
        let slot = &mut bases[i as usize];
        if slot.is_none_or(|(_, best_d2)| d2 < best_d2) {
            *slot = Some((pj, d2));
        }
    };
    for bond in &complex.bonds {
        let same_residue = complex.atoms[bond.a.0 as usize].residue_idx == complex.atoms[bond.b.0 as usize].residue_idx;
        if !same_residue {
            continue;
        }
        consider(bond.a.0, bond.b.0);
        consider(bond.b.0, bond.a.0);
    }

    bases.into_iter().map(|b| b.map(|(p, _)| p)).collect()
}

/// Returns true if `res_a` and `res_b` are adjacent covalently linked polypeptide residues (`|Δseq| <= 1`) on the same chain.
#[inline]
pub fn is_adjacent_backbone_pair(res_a: &ResidueRecord, res_b: &ResidueRecord) -> bool {
    res_a.chain_idx == res_b.chain_idx
        && (res_a.seq_num - res_b.seq_num).abs() <= 1
        && res_a.kind.is_polypeptide()
        && res_b.kind.is_polypeptide()
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
pub fn compute_aliphatic_packing_scores(
    complex: &ComplexStructure,
    partition: &InterfacePartition,
    p: &ContactParams,
) -> Vec<f32> {
    let n_atoms = complex.atoms.len();
    let mut scores = vec![0.0_f32; n_atoms];
    let mut weights = vec![0.0_f32; n_atoms];

    let atom_sides = partition.atom_sides(complex);

    let mut grid = SpatialGrid::new(p.aliphatic_env_radius_a);
    for (i, atom) in complex.atoms.iter().enumerate() {
        if !atom_sides[i].is_assigned() {
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
        if !atom_sides[i].is_assigned() {
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

/// Distance from each assigned atom to its nearest opposite-side atom, or `INFINITY` beyond `reach_a`.
/// Searched independently of SASA, whose neighbor query stops near 5 Å.
fn min_cross_distances(complex: &ComplexStructure, atom_sides: &[InterfaceSide], reach_a: Option<f32>) -> Vec<f32> {
    let mut dists = vec![f32::INFINITY; complex.atoms.len()];
    let Some(reach_a) = reach_a.filter(|r| *r > 0.0) else {
        return dists;
    };
    let mut grid = SpatialGrid::new(reach_a);
    for (i, atom) in complex.atoms.iter().enumerate() {
        if atom_sides[i].is_assigned() {
            grid.insert(i as u32, atom.pos_3d);
        }
    }
    for (i, atom) in complex.atoms.iter().enumerate() {
        let side = atom_sides[i];
        if !side.is_assigned() {
            continue;
        }
        let mut min_d2 = f32::INFINITY;
        grid.query_radius(atom.pos_3d, reach_a, |j, _, d2| {
            if atom_sides[j as usize] == side.opposite() && d2 < min_d2 {
                min_d2 = d2;
            }
        });
        dists[i] = min_d2.sqrt();
    }
    dists
}

/// Extracts the 3D interfacial atom nodes, residue groups, and classified 3D interaction edges
/// from `complex` using the active `partition`.
pub fn extract_3d_interface_graph(
    complex: &ComplexStructure,
    partition: &InterfacePartition,
    sasa_p: &SasaParams,
    p: &ContactParams,
) -> Result<(Vec<ResidueTerritory>, Vec<AtomNode>, Vec<InterfaceEdge>), String> {
    let sasa_vec = compute_complex_sasa(complex, partition, sasa_p);
    let aliphatic_scores = compute_aliphatic_packing_scores(complex, partition, p);

    let multi_chain_a = partition.group_a_chains.len() > 1;
    let multi_chain_b = partition.group_b_chains.len() > 1;
    let atom_sides = partition.atom_sides(complex);
    let cross_dists = min_cross_distances(complex, &atom_sides, partition.max_proximity_cutoff_a());

    let mut res_to_territory: HashMap<GlobalResidueIdx, TerritoryIdx> = HashMap::new();
    let mut territories: Vec<ResidueTerritory> = Vec::new();
    let mut nodes: Vec<AtomNode> = Vec::new();

    for (a_idx, atom) in complex.atoms.iter().enumerate() {
        let side = atom_sides[a_idx];
        if !side.is_assigned() {
            continue;
        }
        let sasa = &sasa_vec[a_idx];
        if !partition.is_interfacial(sasa.buried_sasa_a2, cross_dists[a_idx]) {
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
                side,
                chain_id: res.chain_id.clone(),
                label_top,
                label_bottom,
                patch_atoms: Vec::new(),
                buried_sasa_a2: 0.0,
                has_unsatisfied_polar: false,
                contour_uv: Vec::new(),
                contour_3d: Vec::new(),
                contour_triangles: Vec::new(),
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
            side,
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
    let bonded_pairs: HashMap<(u32, u32), BondKind> =
        complex.bonds.iter().map(|b| ((b.a.0.min(b.b.0), b.a.0.max(b.b.0)), b.kind)).collect();
    // Per-atom bonded neighbors, for the 1-3 exclusion test below.
    let mut neighbors: HashMap<u32, Vec<u32>> = HashMap::new();
    for bond in &complex.bonds {
        neighbors.entry(bond.a.0).or_default().push(bond.b.0);
        neighbors.entry(bond.b.0).or_default().push(bond.a.0);
    }
    let shares_bonded_neighbor = |a: u32, b: u32| {
        neighbors
            .get(&a)
            .is_some_and(|ns_a| neighbors.get(&b).is_some_and(|ns_b| ns_a.iter().any(|n| ns_b.contains(n))))
    };
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
            let bonded = bonded_pairs.get(&((g_i as u32).min(g_j as u32), (g_i as u32).max(g_j as u32))).copied();
            let one_bond_removed = bonded.is_none() && shares_bonded_neighbor(g_i as u32, g_j as u32);

            let Some((kind, overlap_a)) =
                classify_pair_edge(atom_i, res_i, base_i, atom_j, res_j, base_j, dist, bonded, one_bond_removed, p)
            else {
                continue;
            };

            let is_cross = nodes[i].side != nodes[j].side;
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

            // Every edge is stored twice, once in each node's interactions list, for fast access.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::definition::InterfaceCriterion;
    use crate::ingest::load_complex;
    use crate::params::AnalysisParams;

    /// Brute-force nearest opposite-side distance, to validate the grid search at any cutoff.
    fn brute_cross_dist(complex: &ComplexStructure, sides: &[InterfaceSide], i: usize) -> f32 {
        let pi = Vec3::from(complex.atoms[i].pos_3d);
        complex
            .atoms
            .iter()
            .enumerate()
            .filter(|(j, _)| sides[*j] == sides[i].opposite())
            .map(|(_, a)| pi.distance(Vec3::from(a.pos_3d)))
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn proximity_criterion_matches_brute_force_beyond_sasa_reach() {
        let complex = load_complex("../../examples/7JVB.cif").unwrap();
        let base = InterfacePartition::from_chains(&complex, "t", "A", vec!["A".into()], "B", vec!["C".into()]);
        let sides = base.atom_sides(&complex);
        let p = AnalysisParams::default();

        let mut counts = Vec::new();
        for cutoff_a in [4.5_f32, 8.0] {
            let part = base.clone().with_criteria(vec![InterfaceCriterion::Proximity { cutoff_a }]);
            let (_, nodes, _) = extract_3d_interface_graph(&complex, &part, &p.sasa, &p.contacts).unwrap();
            let included: std::collections::HashSet<usize> =
                nodes.iter().map(|n| n.global_atom.0 as usize).collect();
            for i in (0..complex.atoms.len()).filter(|&i| sides[i].is_assigned()) {
                let d = brute_cross_dist(&complex, &sides, i);
                // Skip pairs within float noise of the cutoff.
                if (d - cutoff_a).abs() > 1e-3 {
                    assert_eq!(included.contains(&i), d <= cutoff_a, "atom {i} at {d} Å, cutoff {cutoff_a} Å");
                }
            }
            counts.push(nodes.len());
        }
        assert!(counts[1] > counts[0]);
    }
}
