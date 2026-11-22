//! Direct `.pdb` and `.cif` structure ingestion and group assignment via `pdbtbx`.

use crate::amino_acids::{ElementExt, ResidueKind};
use crate::dssp::{SecStructMode, assign_secondary_structure};
use crate::model::{
    AtomRecord, ChainIdx, ChainName, ChainRecord, ComplexStructure, Element, GlobalAtomIdx, GlobalResidueIdx,
    InterfacePartition, InterfaceSide, ResidueRecord, SecStruct,
};
use std::path::Path;

/// Loads all chains from a `.pdb` or `.cif` structure into an unpartitioned [`ComplexStructure`].
pub fn load_structure(path: impl AsRef<Path>) -> Result<ComplexStructure, String> {
    load_complex(path, &[], &[])
}

/// Loads all chains from a `.pdb` or `.cif` structure, classifying canonical amino acids,
/// modified residues, coordinating metal ions, and ligands via [`ResidueKind`].
///
/// Sets initial `SideA` (`group_a_chains`) and `SideB` (`group_b_chains`) partition;
/// any remaining chains in the file are loaded with [`InterfaceSide::None`].
pub fn load_complex(
    path: impl AsRef<Path>,
    group_a_chains: &[ChainName],
    group_b_chains: &[ChainName],
) -> Result<ComplexStructure, String> {
    let path_ref = path.as_ref();
    let path_str = path_ref
        .to_str()
        .ok_or_else(|| "Invalid UTF-8 path".to_string())?;

    let (pdb, _errors) =
        pdbtbx::open(path_str).map_err(|errs| format!("Failed to open {path_str}: {errs:?}"))?;

    let model = pdb
        .models()
        .next()
        .ok_or_else(|| format!("No models found in {path_str}"))?;

    let name = path_ref
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("complex")
        .to_string();
    
    let mut chains = Vec::new();
    let mut residues = Vec::new();
    let mut atoms = Vec::new();

    for chain in model.chains() {
        let cid = chain.id().to_string();
        let role = if group_a_chains.iter().any(|c| c.as_str() == cid) {
            InterfaceSide::SideA
        } else if group_b_chains.iter().any(|c| c.as_str() == cid) {
            InterfaceSide::SideB
        } else {
            InterfaceSide::None
        };

        let chain_idx = ChainIdx(chains.len() as u32);
        let chain_res_start = residues.len() as u32;
        let chain_atom_start = atoms.len() as u32;

        for res in chain.residues() {
            let raw_name = res.name().unwrap_or("");
            let has_metal = res
                .atoms()
                .filter_map(|a| a.element().copied())
                .any(ElementExt::is_metal_ion);

            let kind = ResidueKind::classify(raw_name, has_metal);
            if kind.is_solvent_or_buffer() {
                continue;
            }

            let start_atom = atoms.len() as u32;
            let res_idx = GlobalResidueIdx(residues.len() as u32);

            for atom in res.atoms() {
                let element = atom
                    .element()
                    .copied()
                    .or_else(|| Element::from_symbol(atom.name().trim()))
                    .unwrap_or(Element::C);

                if element.is_hydrogen_atom() {
                    continue;
                }

                let mut atom_name = [b' '; 4];
                for (i, b) in atom.name().trim().bytes().take(4).enumerate() {
                    atom_name[i] = b;
                }

                let atom_idx = GlobalAtomIdx(atoms.len() as u32);
                atoms.push(AtomRecord {
                    idx: atom_idx,
                    residue_idx: res_idx,
                    chain_idx,
                    role,
                    atom_serial: atom.serial_number(),
                    atom_name,
                    element,
                    vdw_radius_a: element.vdw_radius_a(),
                    pos_3d: [atom.x() as f32, atom.y() as f32, atom.z() as f32],
                    b_factor: atom.b_factor() as f32,
                    occupancy: atom.occupancy() as f32,
                });
            }

            let end_atom = atoms.len() as u32;
            if end_atom == start_atom {
                continue;
            }

            let mut res_name = [b' '; 3];
            for (i, b) in raw_name.trim().bytes().take(3).enumerate() {
                res_name[i] = b;
            }

            let ins_code = res
                .insertion_code()
                .and_then(|s| s.chars().next())
                .unwrap_or(' ');

            residues.push(ResidueRecord {
                idx: res_idx,
                chain_idx,
                role,
                kind,
                chain_id: ChainName::new(&cid),
                seq_num: res.serial_number() as i32,
                ins_code,
                res_name,
                sec_struct: SecStruct::Loop,
                backbone_phi_psi: None,
                atom_range: (start_atom, end_atom),
            });
        }

        let chain_res_end = residues.len() as u32;
        let chain_atom_end = atoms.len() as u32;
        if chain_res_end > chain_res_start {
            chains.push(ChainRecord {
                idx: chain_idx,
                chain_id: ChainName::new(cid),
                residue_range: (chain_res_start, chain_res_end),
                atom_range: (chain_atom_start, chain_atom_end),
            });
        }
    }

    if residues.is_empty() {
        return Err(format!("No protein or ligand residues found in {path_str}"));
    }

    let mut complex = ComplexStructure {
        name,
        group_a_name: "Group A".to_string(),
        group_b_name: "Group B".to_string(),
        group_a_chains: group_a_chains.to_vec(),
        group_b_chains: group_b_chains.to_vec(),
        chains,
        residues,
        atoms,
    };

    assign_secondary_structure(
        &mut complex,
        Some(path_ref),
        SecStructMode::FromFileOrComputeDssp,
    );
    Ok(complex)
}

/// Updates the active [`InterfaceSide`] partition in-place using an [`InterfacePartition`].
pub fn set_active_partition(complex: &mut ComplexStructure, partition: &InterfacePartition) {
    complex.group_a_name = partition.group_a_name.clone();
    complex.group_b_name = partition.group_b_name.clone();
    complex.group_a_chains = partition.group_a_chains.clone();
    complex.group_b_chains = partition.group_b_chains.clone();

    for res in &mut complex.residues {
        let side = partition.chain_side(res.chain_id.as_str());
        res.role = side;
        for a_idx in res.atom_range.0..res.atom_range.1 {
            complex.atoms[a_idx as usize].role = side;
        }
    }
}

/// Updates the active [`InterfaceSide`] partition in-place on an already-loaded [`ComplexStructure`].
pub fn set_active_groups(
    complex: &mut ComplexStructure,
    group_a_name: impl Into<String>,
    group_a_chains: &[ChainName],
    group_b_name: impl Into<String>,
    group_b_chains: &[ChainName],
) {
    let partition = InterfacePartition::new(
        &complex.name,
        group_a_name,
        group_b_name,
        group_a_chains.to_vec(),
        group_b_chains.to_vec(),
    );
    set_active_partition(complex, &partition);
}
