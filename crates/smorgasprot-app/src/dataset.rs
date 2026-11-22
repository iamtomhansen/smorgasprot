//! Render-ready snapshot of one loaded complex and its interface maps.

use serde::Serialize;
use smorgasprot_core::{
    AnalysisParams, AtomNode, AtomNodeIdx, GlobalChainIdx, ChainName, ComplexStructure, Element, GlobalAtomIdx, GlobalResidueIdx,
    InterfaceGeometry, InterfaceMap, InterfacePartition, InterfaceSide, ResidueTerritory, SurfaceMesh, TerritoryIdx,
    flatten::FlatteningStats, interface_geometry::build_interface_geometry, surface::build_chain_surface_mesh,
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

/// Precomputed visual & geometric data for one macromolecular interface partition. Many interfaces
/// can be precomputed for a single complex. They are stored in the [`ViewerDataset`].
#[derive(Serialize, Clone, Debug)]
pub struct InterfaceViewData {
    /// Interface partition demarcation rules & atom membership.
    pub partition: InterfacePartition,
    /// 2D metric flattening statistics & diagnostics.
    pub stats: FlatteningStats,
    /// 2D interface map (SVD plane, nodes, edges, Voronoi territories).
    pub map: InterfaceMap,
    /// 3D boundary meshes (envelope, separator sheet, negative space).
    pub geometry: InterfaceGeometry,
    /// User-inspectable atom metadata for interfacial atoms (1:1 with `map.nodes`).
    pub atom_meta: Vec<AtomMeta>,
    /// Interface side of each `geometry.envelope_mesh` vertex (render cache, not exported).
    #[serde(skip)]
    pub envelope_sides: Vec<InterfaceSide>,
}

impl InterfaceViewData {
    /// Computes flattening, boundary geometry, and metadata for a partition.
    pub fn compute(
        complex: &ComplexStructure,
        mut partition: InterfacePartition,
        chain_meshes: &[SurfaceMesh],
        params: &AnalysisParams,
    ) -> Result<Self, String> {
        partition.recompute_membership(complex);
        let (map, stats) = smorgasprot_core::flatten::build_interface_map(complex, &partition, params)?;
        let geometry = build_interface_geometry(complex, &partition, &map, chain_meshes, &params.geometry);
        let atom_sides = partition.atom_sides(complex);
        let envelope_sides = geometry
            .envelope_mesh
            .nearest_atom
            .iter()
            .map(|g| atom_sides.get(g.0 as usize).copied().unwrap_or(InterfaceSide::None))
            .collect();
        let atom_meta = map.nodes.iter().map(|n| atom_meta(complex, n)).collect();
        Ok(Self {
            partition,
            stats,
            map,
            geometry,
            atom_meta,
            envelope_sides,
        })
    }

    /// Recomputes 3D boundary meshes using updated parameters or chain meshes.
    pub fn recompute_geometry(&mut self, complex: &ComplexStructure, chain_meshes: &[SurfaceMesh], params: &AnalysisParams) {
        self.geometry = build_interface_geometry(complex, &self.partition, &self.map, chain_meshes, &params.geometry);
        let atom_sides = self.partition.atom_sides(complex);
        self.envelope_sides = self
            .geometry
            .envelope_mesh
            .nearest_atom
            .iter()
            .map(|g| atom_sides.get(g.0 as usize).copied().unwrap_or(InterfaceSide::None))
            .collect();
    }

    /// Re-evaluates partition rules, flattens, and rebuilds geometry and metadata.
    pub fn recompute_interface(&mut self, complex: &ComplexStructure, chain_meshes: &[SurfaceMesh], params: &AnalysisParams) -> Result<(), String> {
        self.partition.recompute_membership(complex);
        let (map, stats) = smorgasprot_core::flatten::build_interface_map(complex, &self.partition, params)?;
        self.atom_meta = map.nodes.iter().map(|n| atom_meta(complex, n)).collect();
        self.map = map;
        self.stats = stats;
        self.recompute_geometry(complex, chain_meshes, params);
        Ok(())
    }
}

/// The user-facing names and data for a [`ComplexStructure`]. All loaded data is identified and
/// labelled here.
#[derive(Serialize, Clone, Debug)]
pub struct ViewerDataset {
    /// Unique ID for this dataset.
    pub id: String,
    /// User-facing title for this dataset.
    pub title: String,
    /// Per-chain geometry (alpha carbon trace + molecular surface).
    pub chains: Vec<ChainVisualData>,
    /// Precomputed interfaces available for inspection.
    pub interfaces: Vec<InterfaceViewData>,
    /// Index of the active interface in `interfaces`.
    #[serde(skip)]
    pub active_interface: usize,
    /// Source complex structure (retained in-memory for live recomputation; not exported).
    #[serde(skip)]
    pub complex: Option<ComplexStructure>,
}

impl ViewerDataset {
    /// Builds chain surfaces once and evaluates all given interface partitions.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        complex: &ComplexStructure,
        partitions: Vec<InterfacePartition>,
        params: &AnalysisParams,
    ) -> Result<Self, String> {
        let chains: Vec<ChainVisualData> = complex
            .chains
            .iter()
            .enumerate()
            .map(|(c_idx, chain)| ChainVisualData {
                chain_id: chain.chain_id.clone(),
                residue_count: (chain.residue_range.1 - chain.residue_range.0) as usize,
                atom_count: (chain.atom_range.1 - chain.atom_range.0) as usize,
                ca_positions: ca_trace(complex, chain.residue_range),
                surface_mesh: build_chain_surface_mesh(complex, GlobalChainIdx(c_idx as u32), &params.surface),
            })
            .collect();

        let chain_meshes: Vec<SurfaceMesh> = chains.iter().map(|c| c.surface_mesh.clone()).collect();
        let mut interfaces = Vec::with_capacity(partitions.len());
        for part in partitions {
            interfaces.push(InterfaceViewData::compute(complex, part, &chain_meshes, params)?);
        }

        Ok(Self {
            id: id.into(),
            title: title.into(),
            chains,
            interfaces,
            active_interface: 0,
            complex: Some(complex.clone()),
        })
    }

    /// Adds a new interface partition, precomputing its map and geometry.
    pub fn add_interface(&mut self, partition: InterfacePartition, params: &AnalysisParams) -> Result<usize, String> {
        let Some(complex) = &self.complex else {
            return Err("No complex structure available for interface computation".into());
        };
        let chain_meshes: Vec<SurfaceMesh> = self.chains.iter().map(|c| c.surface_mesh.clone()).collect();
        let iface = InterfaceViewData::compute(complex, partition, &chain_meshes, params)?;
        self.interfaces.push(iface);
        Ok(self.interfaces.len() - 1)
    }

    /// Recomputes the chain surface meshes and interface geometry using updated parameters.
    pub fn recompute_surfaces(&mut self, params: &AnalysisParams) {
        let Some(complex) = &self.complex else { return };
        for (c_idx, chain) in self.chains.iter_mut().enumerate() {
            chain.surface_mesh = build_chain_surface_mesh(complex, GlobalChainIdx(c_idx as u32), &params.surface);
        }
        let chain_meshes: Vec<SurfaceMesh> = self.chains.iter().map(|c| c.surface_mesh.clone()).collect();
        for iface in &mut self.interfaces {
            iface.recompute_geometry(complex, &chain_meshes, params);
        }
    }

    /// Re-extracts and flattens the active interface, updating the map, derived meshes, and atom metadata.
    pub fn recompute_interface(&mut self, params: &AnalysisParams) -> Result<(), String> {
        let Some(complex) = &self.complex else { return Ok(()) };
        let chain_meshes: Vec<SurfaceMesh> = self.chains.iter().map(|c| c.surface_mesh.clone()).collect();
        if let Some(iface) = self.interfaces.get_mut(self.active_interface) {
            iface.recompute_interface(complex, &chain_meshes, params)?;
        }
        Ok(())
    }

    pub fn active_interface(&self) -> Option<&InterfaceViewData> {
        self.interfaces.get(self.active_interface)
    }

    pub fn active_interface_mut(&mut self) -> Option<&mut InterfaceViewData> {
        self.interfaces.get_mut(self.active_interface)
    }

    pub fn active_partition(&self) -> Option<&InterfacePartition> {
        self.active_interface().map(|i| &i.partition)
    }

    pub fn active_partition_mut(&mut self) -> Option<&mut InterfacePartition> {
        self.active_interface_mut().map(|i| &mut i.partition)
    }

    pub fn active_map(&self) -> Option<&InterfaceMap> {
        self.active_interface().map(|i| &i.map)
    }

    pub fn active_geometry(&self) -> Option<&InterfaceGeometry> {
        self.active_interface().map(|i| &i.geometry)
    }

    pub fn active_stats(&self) -> Option<&FlatteningStats> {
        self.active_interface().map(|i| &i.stats)
    }

    pub fn node(&self, idx: AtomNodeIdx) -> Option<&AtomNode> {
        self.active_interface().and_then(|i| i.map.nodes.get(idx.0 as usize))
    }

    pub fn meta(&self, idx: AtomNodeIdx) -> Option<&AtomMeta> {
        self.active_interface().and_then(|i| i.atom_meta.get(idx.0 as usize))
    }

    pub fn territory(&self, idx: TerritoryIdx) -> Option<&ResidueTerritory> {
        self.active_interface().and_then(|i| i.map.territories.get(idx.0 as usize))
    }

    /// Finds the active interface node corresponding to `atom`, if it is interfacial.
    pub fn node_by_global_atom(&self, atom: GlobalAtomIdx) -> Option<&AtomNode> {
        self.active_interface().and_then(|i| i.map.nodes.iter().find(|n| n.global_atom == atom))
    }

    /// Finds the active interface territory corresponding to `residue`, if it is interfacial.
    pub fn territory_by_global_residue(&self, residue: GlobalResidueIdx) -> Option<&ResidueTerritory> {
        self.active_interface()
            .and_then(|i| i.map.territories.iter().find(|t| t.global_residue == residue))
    }

    /// User-facing name of one interface side.
    pub fn group_name(&self, side: InterfaceSide) -> &str {
        self.active_partition()
            .map_or("Unassigned", |p| p.group_name(side))
    }

    /// Returns which interface side a chain belongs to under the active partition.
    #[inline]
    pub fn chain_side(&self, chain_id: &ChainName) -> InterfaceSide {
        self.active_partition()
            .map_or(InterfaceSide::None, |p| p.chain_side(chain_id.as_str()))
    }

    pub fn cross_contact_count(&self) -> usize {
        self.active_map()
            .map_or(0, |m| m.edges.iter().filter(|e| e.is_cross_interface).count())
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
