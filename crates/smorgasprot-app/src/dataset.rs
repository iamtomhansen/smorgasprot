//! Render-ready snapshot of one loaded complex and its interface maps.

use crate::state::SurfaceCulling;
use serde::Serialize;
use smorgasprot_core::{
    AnalysisParams, AtomNode, AtomNodeIdx, ChainName, ComplexStructure, Element, GlobalAtomIdx, GlobalChainIdx,
    GlobalResidueIdx, InterfaceGeometry, InterfaceMap, InterfacePartition, InterfaceSide, ResidueTerritory,
    SurfaceMesh, TerritoryIdx,
    flatten::FlatteningStats,
    interface_geometry::{
        GeometryParams, build_interface_geometry, build_interface_negative_mesh, compute_mesh_volume_a3,
        extract_interface_envelope_mesh,
    },
    surface::{
        GeodesicPreset, SurfaceParams, build_chain_surface_mesh_with_preset, compute_chain_explode_offsets,
    },
};

/// Per-chain presentation geometry (alpha carbon trace + display molecular surface).
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
    /// Presentation-only molecular surface mesh (i.e. not used in core analysis) in plane-local space.
    #[serde(rename = "surface_mesh")]
    pub draw_surface_mesh: SurfaceMesh,
    /// Precomputed world-space subunit explode displacement (Å at full complex explode).
    #[serde(skip)]
    pub draw_explode_offset: [f32; 3],
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

/// Precomputed analytical data and presentation meshes for one macromolecular interface partition.
#[derive(Serialize, Clone, Debug)]
pub struct InterfaceViewData {
    /// Interface partition demarcation rules & atom membership.
    pub partition: InterfacePartition,
    /// 2D metric flattening statistics & diagnostics (analytical).
    pub stats: FlatteningStats,
    /// 2D interface map (SVD plane, nodes, edges, Voronoi territories).
    pub map: InterfaceMap,
    /// Presentation 3D boundary meshes (envelope, separator sheet, negative space).
    #[serde(rename = "geometry")]
    pub draw_geometry: InterfaceGeometry,
    /// Analytical sub-solvent void volume (Å³), computed strictly from `AnalysisParams`.
    #[serde(skip)]
    pub analysis_void_volume_a3: f32,
    /// User-inspectable atom metadata for interfacial atoms (1:1 with `map.nodes`).
    pub atom_meta: Vec<AtomMeta>,
    /// Interface side of each `draw_geometry.envelope_mesh` vertex (render cache, not exported).
    #[serde(skip)]
    pub draw_envelope_sides: Vec<InterfaceSide>,
}

impl InterfaceViewData {
    /// Returns the interface side of each vertex in the envelope mesh, or `InterfaceSide::None`.
    fn envelope_sides_for(
        partition: &InterfacePartition,
        complex: &ComplexStructure,
        envelope: &SurfaceMesh,
    ) -> Vec<InterfaceSide> {
        let atom_sides = partition.atom_sides(complex);
        envelope
            .nearest_atom
            .iter()
            .map(|g| atom_sides.get(g.0 as usize).copied().unwrap_or(InterfaceSide::None))
            .collect()
    }

    /// Computes analytical flattening, void volume, metadata, and initial display meshes for a partition.
    pub fn compute(
        complex: &ComplexStructure,
        mut partition: InterfacePartition,
        draw_chain_meshes: &[SurfaceMesh],
        analysis_params: &AnalysisParams,
    ) -> Result<Self, String> {
        partition.recompute_membership(complex);
        let (map, stats) = smorgasprot_core::flatten::build_interface_map(complex, &partition, analysis_params)?;
        let draw_geometry =
            build_interface_geometry(complex, &partition, &map, draw_chain_meshes, &analysis_params.geometry);
        let analysis_void_volume_a3 = draw_geometry.void_volume_a3;
        let draw_envelope_sides = Self::envelope_sides_for(&partition, complex, &draw_geometry.envelope_mesh);
        let atom_meta = map.nodes.iter().map(|n| atom_meta(complex, n)).collect();
        Ok(Self {
            partition,
            stats,
            map,
            draw_geometry,
            analysis_void_volume_a3,
            atom_meta,
            draw_envelope_sides,
        })
    }

    /// Re-extracts the interface's contact-envelope mesh from updated display chain surfaces.
    /// Display only; doesn't recompute analytical metrics or maps.
    pub fn recompute_draw_envelope(&mut self, complex: &ComplexStructure, draw_chain_meshes: &[SurfaceMesh]) {
        let envelope_mesh =
            extract_interface_envelope_mesh(draw_chain_meshes, complex, &self.partition, &self.map.nodes);
        self.draw_envelope_sides = Self::envelope_sides_for(&self.partition, complex, &envelope_mesh);
        self.draw_geometry.envelope_mesh = envelope_mesh;
    }

    /// Recomputes presentation 3D boundary meshes from `draw_params` without altering `analysis_void_volume_a3`.
    pub fn recompute_draw_geometry(
        &mut self,
        complex: &ComplexStructure,
        draw_chain_meshes: &[SurfaceMesh],
        draw_params: &GeometryParams,
    ) {
        let mut draw_geometry =
            build_interface_geometry(complex, &self.partition, &self.map, draw_chain_meshes, draw_params);
        draw_geometry.void_volume_a3 = self.analysis_void_volume_a3;
        self.draw_envelope_sides = Self::envelope_sides_for(&self.partition, complex, &draw_geometry.envelope_mesh);
        self.draw_geometry = draw_geometry;
    }

    /// Re-evaluates analytical partition rules, flattening, void volume, and metadata from `analysis_params`.
    pub fn recompute_analysis(
        &mut self,
        complex: &ComplexStructure,
        draw_chain_meshes: &[SurfaceMesh],
        analysis_params: &AnalysisParams,
    ) -> Result<(), String> {
        self.partition.recompute_membership(complex);
        let (map, stats) = smorgasprot_core::flatten::build_interface_map(complex, &self.partition, analysis_params)?;
        let analysis_negative = build_interface_negative_mesh(complex, &map.nodes, &analysis_params.geometry);
        self.analysis_void_volume_a3 = compute_mesh_volume_a3(&analysis_negative);
        self.atom_meta = map.nodes.iter().map(|n| atom_meta(complex, n)).collect();
        self.map = map;
        self.stats = stats;
        self.recompute_draw_geometry(complex, draw_chain_meshes, &analysis_params.geometry);
        Ok(())
    }
}

static NEXT_DATASET_INSTANCE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

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
    /// Precomputed interfaces available for inspection (may be empty for a bare complex).
    pub interfaces: Vec<InterfaceViewData>,
    /// Index of the active interface in `interfaces`, or `None` when viewing a bare complex with no active interface.
    #[serde(skip)]
    pub active_interface: Option<usize>,
    /// Geometric centroid of the complex in world coordinates. Used as the camera origin when no interface is active.
    #[serde(skip)]
    pub default_origin: [f32; 3],
    /// Source complex structure (retained in-memory for live recomputation; not exported).
    #[serde(skip)]
    pub complex: Option<ComplexStructure>,
    /// Immutable analytical parameters used to compute scientific maps, stats, and void volumes.
    #[serde(skip)]
    analysis_params: AnalysisParams,
    /// Unit tessellation preset; decided at construction by `display_defaults_for`, debounced
    /// into `chains[*].draw_surface_mesh` by `recompute_draw_surfaces`/`poll_draw_surface_remesh`.
    #[serde(skip)]
    pub draw_preset: GeodesicPreset,
    /// Presentation meshing parameters, same debounce as above.
    #[serde(skip)]
    pub draw_mesh: SurfaceParams,
    /// Face-culling mode; decided at construction by `display_defaults_for`.
    #[serde(skip)]
    pub culling: SurfaceCulling,
    /// Last presentation tessellation preset baked into `chains[*].draw_surface_mesh`.
    #[serde(skip)]
    applied_draw_preset: GeodesicPreset,
    /// Last presentation `SurfaceParams` baked into `chains[*].draw_surface_mesh`.
    #[serde(skip)]
    applied_draw_surface: SurfaceParams,
    /// Pending debounced presentation surface remesh deadline (`egui` time in seconds).
    #[serde(skip)]
    draw_remesh_due: Option<f64>,
    /// Unique per-instance key for GPU buffer caching (survives file reloads with the same `id`).
    #[serde(skip)]
    draw_instance_id: u64,
    /// Monotonic generation bumped whenever presentation chain surfaces are remeshed.
    #[serde(skip)]
    draw_mesh_gen: u64,
}

/// Above this many heavy atoms, new loads start at the coarsest preset and backface culling.
const LARGE_COMPLEX_ATOM_THRESHOLD: usize = 15_000;

struct DisplayDefaults {
    draw_preset: GeodesicPreset,
    culling: SurfaceCulling,
}

fn display_defaults_for(atom_count: usize) -> DisplayDefaults {
    if atom_count > LARGE_COMPLEX_ATOM_THRESHOLD {
        DisplayDefaults { draw_preset: GeodesicPreset::Octahedron8, culling: SurfaceCulling::Backfaces }
    } else {
        DisplayDefaults { draw_preset: GeodesicPreset::Octahedron18, culling: SurfaceCulling::None }
    }
}

impl ViewerDataset {
    /// Builds initial chain display surfaces and evaluates all given analytical interface partitions.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        complex: &ComplexStructure,
        partitions: Vec<InterfacePartition>,
        analysis_params: &AnalysisParams,
    ) -> Result<Self, String> {
        let defaults = display_defaults_for(complex.atoms.len());
        let initial_preset = defaults.draw_preset;
        let initial_draw_surface = SurfaceParams::display();
        let explode_offsets = compute_chain_explode_offsets(complex);
        let chains: Vec<ChainVisualData> = complex
            .chains
            .iter()
            .enumerate()
            .map(|(c_idx, chain)| ChainVisualData {
                chain_id: chain.chain_id.clone(),
                residue_count: (chain.residue_range.1 - chain.residue_range.0) as usize,
                atom_count: (chain.atom_range.1 - chain.atom_range.0) as usize,
                ca_positions: ca_trace(complex, chain.residue_range),
                draw_surface_mesh: build_chain_surface_mesh_with_preset(
                    complex,
                    GlobalChainIdx(c_idx as u32),
                    initial_preset,
                    &initial_draw_surface,
                ),
                draw_explode_offset: explode_offsets.get(c_idx).copied().unwrap_or([0.0; 3]),
            })
            .collect();

        let draw_chain_meshes: Vec<SurfaceMesh> = chains.iter().map(|c| c.draw_surface_mesh.clone()).collect();
        let mut interfaces = Vec::with_capacity(partitions.len());
        for part in partitions {
            interfaces.push(InterfaceViewData::compute(complex, part, &draw_chain_meshes, analysis_params)?);
        }
        let active_interface = if interfaces.is_empty() { None } else { Some(0) };

        Ok(Self {
            id: id.into(),
            title: title.into(),
            chains,
            interfaces,
            active_interface,
            default_origin: complex_centroid(complex),
            complex: Some(complex.clone()),
            analysis_params: analysis_params.clone(),
            draw_preset: initial_preset,
            draw_mesh: initial_draw_surface.clone(),
            culling: defaults.culling,
            applied_draw_preset: initial_preset,
            applied_draw_surface: initial_draw_surface,
            draw_remesh_due: None,
            draw_instance_id: NEXT_DATASET_INSTANCE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            draw_mesh_gen: 1,
        })
    }

    /// Unique `(instance_id, mesh_generation)` pair for GPU surface buffer invalidation.
    pub fn draw_cache_key(&self) -> (u64, u64) {
        (self.draw_instance_id, self.draw_mesh_gen)
    }

    /// Average full-explode subunit displacement across all chains (Å).
    pub fn mean_chain_explode_distance_a(&self) -> f32 {
        if self.chains.is_empty() {
            return 0.0;
        }
        let sum: f32 = self
            .chains
            .iter()
            .map(|c| glam::Vec3::from(c.draw_explode_offset).length())
            .sum();
        sum / (self.chains.len() as f32)
    }

    /// Returns the presentation surface mesh for each chain, in the same order as `chains`.
    fn draw_chain_meshes(&self) -> Vec<SurfaceMesh> {
        self.chains.iter().map(|c| c.draw_surface_mesh.clone()).collect()
    }

    /// Read-only access to the analytical parameters that produced this dataset's scientific metrics.
    pub fn analysis_params(&self) -> &AnalysisParams {
        &self.analysis_params
    }

    /// Heavy-atom count, for `display_defaults_for`.
    pub fn atom_count(&self) -> usize {
        self.complex.as_ref().map_or(0, |c| c.atoms.len())
    }

    /// Presentation tessellation preset currently reflected by `chains[*].draw_surface_mesh`.
    pub fn applied_draw_preset(&self) -> GeodesicPreset {
        self.applied_draw_preset
    }

    /// Presentation surface parameters currently reflected by `chains[*].draw_surface_mesh`.
    pub fn applied_draw_surface(&self) -> &SurfaceParams {
        &self.applied_draw_surface
    }

    /// Whether `culling` still matches this dataset's size-based recommendation (recomputed).
    pub fn is_default_culling(&self) -> bool {
        self.culling == display_defaults_for(self.atom_count()).culling
    }

    /// Resets `culling` to this dataset's size-based recommendation (recomputed).
    pub fn reset_culling(&mut self) {
        self.culling = display_defaults_for(self.atom_count()).culling;
    }

    /// Whether `draw_preset`/`draw_mesh` still match this dataset's size-based recommendation
    /// (recomputed) and the presentation meshing defaults.
    pub fn is_default_draw_mesh(&self) -> bool {
        self.draw_preset == display_defaults_for(self.atom_count()).draw_preset
            && self.draw_mesh == SurfaceParams::display()
    }

    /// Resets `draw_preset`/`draw_mesh` to their defaults. Callers that care whether this
    /// actually changed anything (to decide whether to schedule a remesh) should check
    /// `is_default_draw_mesh` first, as `complex_controls.rs` does.
    pub fn reset_draw_mesh(&mut self) {
        self.draw_preset = display_defaults_for(self.atom_count()).draw_preset;
        self.draw_mesh = SurfaceParams::display();
    }

    /// Adds a new interface partition, precomputing its analytical map and presentation geometry.
    pub fn add_interface(
        &mut self,
        partition: InterfacePartition,
        analysis_params: &AnalysisParams,
    ) -> Result<usize, String> {
        let Some(complex) = &self.complex else {
            return Err("No complex structure available for interface computation".into());
        };
        let draw_chain_meshes = self.draw_chain_meshes();
        let iface = InterfaceViewData::compute(complex, partition, &draw_chain_meshes, analysis_params)?;
        self.interfaces.push(iface);
        let idx = self.interfaces.len() - 1;
        self.active_interface = Some(idx);
        Ok(idx)
    }

    /// Removes the interface at `idx` and keeps `active_interface` valid.
    pub fn remove_interface(&mut self, idx: usize) {
        if idx >= self.interfaces.len() {
            return;
        }
        self.interfaces.remove(idx);
        self.active_interface = match self.active_interface {
            Some(active) if active == idx => {
                (!self.interfaces.is_empty()).then(|| idx.min(self.interfaces.len() - 1))
            }
            Some(active) if active > idx => Some(active - 1),
            other => other,
        };
    }

    /// Removes all computed interface partitions, leaving the bare 3D complex.
    pub fn remove_all_interfaces(&mut self) {
        self.interfaces.clear();
        self.active_interface = None;
    }

    /// Schedules a debounced presentation surface remesh after `delay_s` seconds of inactivity.
    pub fn schedule_draw_surface_remesh(&mut self, now: f64, delay_s: f64) {
        self.draw_remesh_due = Some(now + delay_s);
    }

    /// Runs any pending presentation surface remesh whose deadline has arrived, or returns remaining wait seconds.
    pub fn poll_draw_surface_remesh(&mut self, now: f64) -> Option<f64> {
        let due = self.draw_remesh_due?;
        if now >= due {
            self.draw_remesh_due = None;
            self.recompute_draw_surfaces();
            None
        } else {
            Some(due - now)
        }
    }

    /// Recomputes presentation chain surfaces and interface contact envelopes from `draw_preset`
    /// and `draw_mesh`. Analytical metrics, maps, void volumes, and `analysis_params` remain untouched.
    pub fn recompute_draw_surfaces(&mut self) {
        self.draw_remesh_due = None;
        self.applied_draw_preset = self.draw_preset;
        self.applied_draw_surface = self.draw_mesh.clone();
        self.draw_mesh_gen = self.draw_mesh_gen.wrapping_add(1);
        let Some(complex) = &self.complex else { return };
        for (c_idx, chain) in self.chains.iter_mut().enumerate() {
            chain.draw_surface_mesh = build_chain_surface_mesh_with_preset(
                complex,
                GlobalChainIdx(c_idx as u32),
                self.draw_preset,
                &self.draw_mesh,
            );
        }
        let draw_chain_meshes = self.draw_chain_meshes();
        for iface in &mut self.interfaces {
            iface.recompute_draw_envelope(complex, &draw_chain_meshes);
        }
    }

    /// Re-runs analytical flattening and geometry for the active interface using `analysis_params`.
    pub fn recompute_analysis_interface(&mut self, analysis_params: &AnalysisParams) -> Result<(), String> {
        self.analysis_params = analysis_params.clone();
        let Some(complex) = &self.complex else { return Ok(()) };
        let Some(active_idx) = self.active_interface else { return Ok(()) };
        let draw_chain_meshes = self.draw_chain_meshes();
        if let Some(iface) = self.interfaces.get_mut(active_idx) {
            iface.recompute_analysis(complex, &draw_chain_meshes, analysis_params)?;
        }
        Ok(())
    }

    /// Returns the active interface's presentation-level data, if it exists.
    pub fn active_interface(&self) -> Option<&InterfaceViewData> {
        self.active_interface.and_then(|idx| self.interfaces.get(idx))
    }

    /// Returns a mutable reference to the active interface's presentation-level data, if it exists.
    pub fn active_interface_mut(&mut self) -> Option<&mut InterfaceViewData> {
        self.active_interface.and_then(|idx| self.interfaces.get_mut(idx))
    }

    /// Returns the active interface's analytical partition, if it exists.
    pub fn active_partition(&self) -> Option<&InterfacePartition> {
        self.active_interface().map(|i| &i.partition)
    }

    /// Returns a mutable reference to the active interface's analytical partition, if it exists.
    pub fn active_partition_mut(&mut self) -> Option<&mut InterfacePartition> {
        self.active_interface_mut().map(|i| &mut i.partition)
    }

    /// Returns the active interface's analytical map, if it exists.
    pub fn active_map(&self) -> Option<&InterfaceMap> {
        self.active_interface().map(|i| &i.map)
    }

    /// Returns the analytical interface geometry, if it exists.
    pub fn active_draw_geometry(&self) -> Option<&InterfaceGeometry> {
        self.active_interface().map(|i| &i.draw_geometry)
    }

    /// Returns the analytical statistics related to flatteing the active interface, if they exist.
    pub fn active_stats(&self) -> Option<&FlatteningStats> {
        self.active_interface().map(|i| &i.stats)
    }

    /// Returns the node on the interface map corresponding to the global `idx`, if it exists.
    pub fn node(&self, idx: AtomNodeIdx) -> Option<&AtomNode> {
        self.active_interface().and_then(|i| i.map.nodes.get(idx.0 as usize))
    }

    /// Returns the annotated metadata for the atom corresponding to the global `idx`, if it exists.
    pub fn meta(&self, idx: AtomNodeIdx) -> Option<&AtomMeta> {
        self.active_interface().and_then(|i| i.atom_meta.get(idx.0 as usize))
    }

    /// Returns the residue territory of the map corresponding to the global `idx`, if it exists.
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

    /// Returns the number of cross-interface contacts in the active interface, or 0 if none is active.
    pub fn cross_contact_count(&self) -> usize {
        self.active_map()
            .map_or(0, |m| m.edges.iter().filter(|e| e.is_cross_interface).count())
    }
}

/// Computes the world-space geometric center of all heavy atoms in `complex`.
fn complex_centroid(complex: &ComplexStructure) -> [f32; 3] {
    if complex.atoms.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    let mut sum = [0.0_f64; 3];
    for a in &complex.atoms {
        sum[0] += a.pos_3d[0] as f64;
        sum[1] += a.pos_3d[1] as f64;
        sum[2] += a.pos_3d[2] as f64;
    }
    let n = complex.atoms.len() as f64;
    [(sum[0] / n) as f32, (sum[1] / n) as f32, (sum[2] / n) as f32]
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
