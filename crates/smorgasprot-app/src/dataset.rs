//! In-memory dataset structures for loaded macromolecular complexes and interface maps.

use serde::Serialize;
use smorgasprot_core::{
    ChainIdx, ComplexStructure, Element, InterfaceGeometry, InterfaceMap, SurfaceMesh,
    flatten::FlatteningStats,
    surface::build_chain_surface_mesh,
};

#[derive(Serialize, Clone, Debug)]
pub struct ChainVisualData {
    pub chain_id: String,
    pub residue_count: usize,
    pub atom_count: usize,
    pub ca_positions: Vec<[f32; 3]>,
    pub surface_mesh: SurfaceMesh,
}

#[derive(Serialize, Clone, Debug)]
pub struct AtomMeta {
    pub atom_name: String,
    pub res_name: String,
    pub seq_num: i32,
    pub chain_id: String,
    pub element: Element,
    pub vdw_radius_a: f32,
    pub b_factor: f32,
}

#[derive(Serialize, Clone, Debug)]
pub struct ViewerDataset {
    pub id: String,
    pub title: String,
    pub group_a_name: String,
    pub group_b_name: String,
    pub group_a_chains: Vec<String>,
    pub group_b_chains: Vec<String>,
    pub chains: Vec<ChainVisualData>,
    pub map: InterfaceMap,
    pub geometry: InterfaceGeometry,
    pub atom_meta: Vec<AtomMeta>,
    pub stats: FlatteningStats,
}

impl ViewerDataset {
    pub fn from_complex_and_map(
        id: impl Into<String>,
        title: impl Into<String>,
        complex: &ComplexStructure,
        map: InterfaceMap,
        stats: FlatteningStats,
    ) -> Self {
        let mut chains = Vec::with_capacity(complex.chains.len());
        let mut chain_meshes = Vec::with_capacity(complex.chains.len());
        for (c_idx, chain_rec) in complex.chains.iter().enumerate() {
            let mut ca_positions = Vec::new();
            for r_idx in chain_rec.residue_range.0..chain_rec.residue_range.1 {
                let res = &complex.residues[r_idx as usize];
                for a_idx in res.atom_range.0..res.atom_range.1 {
                    let atom = &complex.atoms[a_idx as usize];
                    if &atom.atom_name == b"CA  " {
                        ca_positions.push(atom.pos_3d);
                        break;
                    }
                }
            }

            let surface_mesh = build_chain_surface_mesh(complex, ChainIdx(c_idx as u32));
            chain_meshes.push(surface_mesh.clone());
            chains.push(ChainVisualData {
                chain_id: chain_rec.chain_id.clone(),
                residue_count: (chain_rec.residue_range.1 - chain_rec.residue_range.0) as usize,
                atom_count: (chain_rec.atom_range.1 - chain_rec.atom_range.0) as usize,
                ca_positions,
                surface_mesh,
            });
        }

        let geometry = smorgasprot_core::interface_geometry::build_interface_geometry(
            complex,
            &map,
            &chain_meshes,
        );

        let atom_meta = map
            .nodes
            .iter()
            .map(|n| {
                let atom = &complex.atoms[n.global_atom.0 as usize];
                let res = &complex.residues[atom.residue_idx.0 as usize];
                AtomMeta {
                    atom_name: std::str::from_utf8(&atom.atom_name)
                        .unwrap_or("?")
                        .trim()
                        .to_string(),
                    res_name: std::str::from_utf8(&res.res_name)
                        .unwrap_or("UNK")
                        .trim()
                        .to_string(),
                    seq_num: res.seq_num,
                    chain_id: res.chain_id.clone(),
                    element: atom.element,
                    vdw_radius_a: atom.vdw_radius_a,
                    b_factor: atom.b_factor,
                }
            })
            .collect();

        Self {
            id: id.into(),
            title: title.into(),
            group_a_name: complex.group_a_name.clone(),
            group_b_name: complex.group_b_name.clone(),
            group_a_chains: complex.group_a_chains.clone(),
            group_b_chains: complex.group_b_chains.clone(),
            chains,
            map,
            geometry,
            atom_meta,
            stats,
        }
    }
}
