//! Render-ready snapshot of one loaded complex and its interface map.
//! Serialized as-is into the HTML export, so serialized field names are a frozen contract with the JS viewer.

use serde::Serialize;
use smorgasprot_core::{
    AnalysisParams, AtomNode, AtomNodeIdx, ChainIdx, ComplexStructure, Element, InterfaceGeometry, InterfaceMap, InterfaceSide,
    ResidueTerritory, SurfaceMesh, TerritoryIdx, flatten::FlatteningStats, interface_geometry::build_interface_geometry,
    surface::build_chain_surface_mesh,
};

/// Per-chain geometry for the Complex tab (Cα trace + molecular surface).
#[derive(Serialize, Clone, Debug)]
pub struct ChainVisualData {
    pub chain_id: String,
    pub residue_count: usize,
    pub atom_count: usize,
    pub ca_positions: Vec<[f32; 3]>,
    pub surface_mesh: SurfaceMesh,
    /// Interface side of this chain (render cache, not exported).
    #[serde(skip)]
    pub side: InterfaceSide,
}

/// Display labels for one interface node, parallel to `InterfaceMap::nodes`.
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

impl AtomMeta {
    /// Compact residue:atom label, e.g. `GLU 42:OE1`.
    pub fn residue_atom_label(&self) -> String {
        format!("{} {}:{}", self.res_name, self.seq_num, self.atom_name)
    }
}

/// Everything the viewer needs for one complex: chains, interface map, derived geometry, and stats.
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
    /// Interface side of each `geometry.envelope_mesh` vertex (render cache, not exported).
    #[serde(skip)]
    pub envelope_sides: Vec<InterfaceSide>,
}

impl ViewerDataset {
    /// Builds chain surfaces, interface geometry, and atom labels from a loaded complex.
    /// Expensive (surface meshing); call once per load or chain reassignment.
    pub fn from_complex_and_map(
        id: impl Into<String>,
        title: impl Into<String>,
        complex: &ComplexStructure,
        map: InterfaceMap,
        stats: FlatteningStats,
        params: &AnalysisParams,
    ) -> Self {
        let chains: Vec<ChainVisualData> = complex
            .chains
            .iter()
            .enumerate()
            .map(|(c_idx, chain)| ChainVisualData {
                chain_id: chain.chain_id.clone(),
                residue_count: (chain.residue_range.1 - chain.residue_range.0) as usize,
                atom_count: (chain.atom_range.1 - chain.atom_range.0) as usize,
                ca_positions: ca_trace(complex, chain.residue_range),
                surface_mesh: build_chain_surface_mesh(complex, ChainIdx(c_idx as u32), &params.surface),
                side: complex.chain_side(&chain.chain_id),
            })
            .collect();

        // Derived interface meshes need every chain surface.
        let chain_meshes: Vec<SurfaceMesh> = chains.iter().map(|c| c.surface_mesh.clone()).collect();
        let geometry = build_interface_geometry(complex, &map, &chain_meshes, &params.geometry);

        // Resolve envelope vertex sides once instead of per frame.
        let envelope_sides = geometry
            .envelope_mesh
            .nearest_atom
            .iter()
            .map(|g| complex.atoms.get(g.0 as usize).map_or(InterfaceSide::None, |a| a.role))
            .collect();

        let atom_meta = map.nodes.iter().map(|n| atom_meta(complex, n)).collect();

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
            envelope_sides,
        }
    }

    pub fn node(&self, idx: AtomNodeIdx) -> Option<&AtomNode> {
        self.map.nodes.get(idx.0 as usize)
    }

    pub fn meta(&self, idx: AtomNodeIdx) -> Option<&AtomMeta> {
        self.atom_meta.get(idx.0 as usize)
    }

    pub fn territory(&self, idx: TerritoryIdx) -> Option<&ResidueTerritory> {
        self.map.territories.get(idx.0 as usize)
    }

    /// User-facing name of one interface side.
    pub fn group_name(&self, side: InterfaceSide) -> &str {
        match side {
            InterfaceSide::SideA => &self.group_a_name,
            InterfaceSide::SideB => &self.group_b_name,
            InterfaceSide::None => "Unassigned",
        }
    }

    pub fn cross_contact_count(&self) -> usize {
        self.map.edges.iter().filter(|e| e.is_cross_interface).count()
    }
}

/// Collects one Cα position per residue in `residue_range` (residues without Cα are skipped).
fn ca_trace(complex: &ComplexStructure, residue_range: (u32, u32)) -> Vec<[f32; 3]> {
    complex.residues[residue_range.0 as usize..residue_range.1 as usize]
        .iter()
        .filter_map(|res| {
            complex.atoms[res.atom_range.0 as usize..res.atom_range.1 as usize]
                .iter()
                .find(|a| &a.atom_name == b"CA  ")
                .map(|a| a.pos_3d)
        })
        .collect()
}

/// Builds display labels for one interface node from its source atom and residue.
fn atom_meta(complex: &ComplexStructure, node: &AtomNode) -> AtomMeta {
    let atom = &complex.atoms[node.global_atom.0 as usize];
    let res = &complex.residues[atom.residue_idx.0 as usize];
    AtomMeta {
        atom_name: std::str::from_utf8(&atom.atom_name).unwrap_or("?").trim().to_string(),
        res_name: std::str::from_utf8(&res.res_name).unwrap_or("UNK").trim().to_string(),
        seq_num: res.seq_num,
        chain_id: res.chain_id.clone(),
        element: atom.element,
        vdw_radius_a: atom.vdw_radius_a,
        b_factor: atom.b_factor,
    }
}
