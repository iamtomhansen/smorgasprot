//! Render-ready snapshot of one loaded complex and its interface map.
//! Serialized as-is into the HTML export, so serialized field names are a frozen contract with the JS viewer.

use serde::Serialize;
use smorgasprot_core::{
    AnalysisParams, AtomNode, AtomNodeIdx, ChainIdx, ChainName, ComplexStructure, Element, InterfaceGeometry, InterfaceMap,
    InterfacePartition, InterfaceSide, ResidueTerritory, SurfaceMesh, TerritoryIdx, flatten::FlatteningStats,
    interface_geometry::build_interface_geometry, surface::build_chain_surface_mesh,
};

/// Per-chain geometry for the Complex tab (alpha carbon trace + molecular surface).
#[derive(Serialize, Clone, Debug)]
pub struct ChainVisualData {
    /// PDB-style chain ID (e.g. "A", "B")
    pub chain_id: ChainName,
    /// Number of residues in this chain.
    pub residue_count: usize,
    /// Number of atoms in this chain.
    pub atom_count: usize,
    /// Alpha carbon positions in plane-local space.
    pub ca_positions: Vec<[f32; 3]>,
    /// Molecular surface mesh in plane-local space.
    pub surface_mesh: SurfaceMesh,
}

/// User inspectable information about a given atom.
#[derive(Serialize, Clone, Debug)]
pub struct AtomMeta {
    /// Atom name, e.g. "CA" or "OE1".
    pub atom_name: String,
    /// Atom serial number from the source PDB/mmCIF file (1-based).
    pub atom_serial: usize,
    /// Residue name, e.g. "GLU" or "ALA".
    pub res_name: String,
    /// Residue sequence number (1-based).
    pub res_num: i32,
    /// Chain ID, e.g. "A" or "B".
    pub chain_id: ChainName,
    /// Element type for this atom.
    pub element: Element,
    /// van der Waals radius in Ångströms.
    pub vdw_radius_a: f32,
    /// B-factor (temperature factor) or pLDDT for this atom.
    pub b_factor: f32,
    /// Occupancy for this atom (0.0–1.0).
    pub occupancy: f32,
}

impl AtomMeta {
    /// Compact residue:atom label, e.g. `GLU 42:OE1`.
    pub fn residue_atom_label(&self) -> String {
        format!("{} {}:{}", self.res_name, self.res_num, self.atom_name)
    }
}

/// The user-facing names and data for a [`ComplexStructure`].
#[derive(Serialize, Clone, Debug)]
pub struct ViewerDataset {
    /// Unique ID for this dataset.
    pub id: String,
    /// User-facing title for this dataset.
    pub title: String,
    pub group_a_name: String,
    pub group_b_name: String,
    pub group_a_chains: Vec<ChainName>,
    pub group_b_chains: Vec<ChainName>,
    pub chains: Vec<ChainVisualData>,
    pub map: InterfaceMap,
    pub geometry: InterfaceGeometry,
    pub atom_meta: Vec<AtomMeta>,
    pub stats: FlatteningStats,
    /// Interface side of each `geometry.envelope_mesh` vertex (render cache, not exported).
    #[serde(skip)]
    pub envelope_sides: Vec<InterfaceSide>,
    /// Source complex structure (retained in-memory for live recomputation and remeshing; not exported).
    #[serde(skip)]
    pub complex: Option<ComplexStructure>,
    /// Interface demarcation partition (retained in-memory for live recomputation; not exported).
    #[serde(skip)]
    pub partition: Option<InterfacePartition>,
}

impl ViewerDataset {
    /// Builds chain surfaces, interface geometry, and atom labels from a loaded complex.
    /// Expensive (surface meshing); call once per load or chain reassignment.
    pub fn from_complex_and_map(
        id: impl Into<String>,
        title: impl Into<String>,
        complex: &ComplexStructure,
        partition: &InterfacePartition,
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
            })
            .collect();

        // Derived interface meshes need every chain surface.
        let chain_meshes: Vec<SurfaceMesh> = chains.iter().map(|c| c.surface_mesh.clone()).collect();
        let geometry = build_interface_geometry(complex, partition, &map, &chain_meshes, &params.geometry);

        // Resolve envelope vertex sides once instead of per frame.
        let atom_sides = partition.atom_sides(complex);
        let envelope_sides = geometry
            .envelope_mesh
            .nearest_atom
            .iter()
            .map(|g| atom_sides.get(g.0 as usize).copied().unwrap_or(InterfaceSide::None))
            .collect();

        let atom_meta = map.nodes.iter().map(|n| atom_meta(complex, n)).collect();

        Self {
            id: id.into(),
            title: title.into(),
            group_a_name: partition.group_a_name.clone(),
            group_b_name: partition.group_b_name.clone(),
            group_a_chains: partition.group_a_chains.clone(),
            group_b_chains: partition.group_b_chains.clone(),
            chains,
            map,
            geometry,
            atom_meta,
            stats,
            envelope_sides,
            complex: Some(complex.clone()),
            partition: Some(partition.clone()),
        }
    }

    /// Recomputes the chain surface meshes and interface geometry using updated parameters.
    pub fn recompute_surfaces(&mut self, params: &AnalysisParams) {
        let (Some(complex), Some(partition)) = (&self.complex, &self.partition) else { return };
        for (c_idx, chain) in self.chains.iter_mut().enumerate() {
            chain.surface_mesh = build_chain_surface_mesh(complex, ChainIdx(c_idx as u32), &params.surface);
        }
        let chain_meshes: Vec<SurfaceMesh> = self.chains.iter().map(|c| c.surface_mesh.clone()).collect();
        self.geometry = build_interface_geometry(complex, partition, &self.map, &chain_meshes, &params.geometry);
        let atom_sides = partition.atom_sides(complex);
        self.envelope_sides = self
            .geometry
            .envelope_mesh
            .nearest_atom
            .iter()
            .map(|g| atom_sides.get(g.0 as usize).copied().unwrap_or(InterfaceSide::None))
            .collect();
    }

    /// Re-extracts and flattens the interface, updating the map, derived meshes, and atom metadata.
    pub fn recompute_interface(&mut self, params: &AnalysisParams) -> Result<(), String> {
        let (Some(complex), Some(partition)) = (&self.complex, &mut self.partition) else { return Ok(()) };
        partition.recompute_membership(complex);
        let (map, stats) = smorgasprot_core::flatten::build_interface_map(complex, partition, params)?;
        self.atom_meta = map.nodes.iter().map(|n| atom_meta(complex, n)).collect();
        self.map = map;
        self.stats = stats;
        self.recompute_surfaces(params);
        Ok(())
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

    /// Returns which interface side a chain belongs to under the active partition.
    #[inline]
    pub fn chain_side(&self, chain_id: &ChainName) -> InterfaceSide {
        self.partition
            .as_ref()
            .map_or(InterfaceSide::None, |p| p.chain_side(chain_id.as_str()))
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

/// Builds metadata for one atom from the complex.
fn atom_meta(complex: &ComplexStructure, node: &AtomNode) -> AtomMeta {
    let atom = &complex.atoms[node.global_atom.0 as usize];
    let res = &complex.residues[atom.residue_idx.0 as usize];
    AtomMeta {
        atom_name: std::str::from_utf8(&atom.atom_name).unwrap_or("?").trim().to_string(),
        atom_serial: atom.atom_serial,
        res_name: std::str::from_utf8(&res.res_name).unwrap_or("UNK").trim().to_string(),
        res_num: res.seq_num,
        chain_id: res.chain_id.clone(),
        element: atom.element,
        vdw_radius_a: atom.vdw_radius_a,
        b_factor: atom.b_factor,
        occupancy: atom.occupancy,
    }
}
