# Architecture & Data Model

## 1. Workspace & Crate Structure

Smorgasprot uses a two-crate Rust workspace separating pure domain/physics/mesh-generation logic from the interactive tabbed viewport and application shell:

```
smorgasprot/
├── Cargo.toml                      # Workspace root
├── design.md                       # High-level overview & entry point
├── examples/                       # Built-in example structures
│   ├── 7JVB.cif
│   └── fold_r8s15_mp3_model_0.cif
├── docs/                           # Modular technical specifications
│   ├── architecture.md
│   ├── algorithms.md
│   ├── visual_grammar.md
│   └── roadmap.md
└── crates/
    ├── smorgasprot-core/           # Pure Rust domain, 3D mesh, layout & physics engine
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── model.rs            # Indices, ComplexStructure, PartnerGroup, SurfaceMesh, InterfaceMap
    │       ├── ingest.rs           # pdbtbx full-complex ingestion (all chains, canonical AA filter)
    │       ├── surface.rs          # 3D Marching Cubes molecular surface & inter-chain interface mesh
    │       ├── sasa.rs             # Shrake-Rupley Fibonacci sphere ΔSASA calculator
    │       ├── spatial.rs          # Uniform 3D voxel hash grid for fast contact/clash queries
    │       ├── flatten.rs          # Tangent-plane SVD basis + coupled 2D spring/repulsion solver
    │       ├── contour.rs          # Voronoi-clipped padded hull + Chaikin contour builder
    │       ├── physics.rs          # VdW clashes, H-bonds, salt bridges, buried polar detection
    │       ├── rotamers.rs         # Backbone-dependent Lovell/Dunbrack library & sidechain FK
    │       └── minimize.rs         # Local χ-angle torsion optimizer & point mutation engine
    └── smorgasprot-app/            # Tabbed [Complex | Interface] viewer & live Rust server/WASM bridge
        ├── Cargo.toml
        └── src/
            ├── main.rs             # CLI & live local Rust HTTP/RPC server (or WASM entry point)
            ├── state.rs            # AppState, ComplexTabConfig, InterfaceTabConfig
            ├── viewport.rs         # Tabbed HTML5 <canvas> / WebGL viewer & live RPC client
            └── ui.rs               # Inspector & workspace controls
```

### Live Rust Computation Boundary (Zero Interface Math in JS)

To guarantee scientific reproducibility and high performance, **zero** biophysical or geometric calculations run in JavaScript:
1. When a `.pdb` or `.cif` structure is loaded, `smorgasprot-core` ingests **all chains** into `ComplexStructure` and precomputes each chain's 3D molecular `SurfaceMesh` and `CartoonTrace`.
2. When the user toggles chains between **Group A**, **Group B**, and **Unassigned** in the **Complex** tab, the UI sends the updated `InterfaceSelection` (`group_a_chains`, `group_b_chains`) to the Rust engine (either in-process via `wasm32-unknown-unknown` or over localhost via `smorgasprot-app --serve`).
3. The Rust engine immediately computes:
   * Per-vertex interface membership flags on each chain's 3D `SurfaceMesh` (so interfacial surface vertices light up live in the **Complex** tab),
   * The approximated **Inter-Chain Interface Sheet Mesh** (drawn between Group A and Group B when viewing in **Cartoon** mode),
   * The interface normal vector $\hat{\mathbf{n}}$ (used by the **Complex** tab's 3D **"Explode" slider** to separate Group A and Group B), and
   * The unwrapped 2D/3D `InterfaceMap` ready for immediate inspection in the **Interface** tab.

---

## 2. Core Data Model (`smorgasprot-core::model`)

### Layer 1: Full Multi-Chain 3D Complex (`ComplexStructure`)

Unlike a static pair loader, `ComplexStructure` loads **every protein chain** in the file so the user can interactively explore the full biological assembly in the **Complex** tab and assign any subset of chains to **Group A** or **Group B** (leaving any remaining chains `Unassigned` and visually desaturated):

```rust
use serde::{Deserialize, Serialize};

/// Dense index into `ComplexStructure::atoms` (all heavy atoms across all chains)
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct GlobalAtomIdx(pub u32);

/// Dense index into `ComplexStructure::residues` (all residues across all chains)
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct GlobalResidueIdx(pub u32);

/// Dense index into `ComplexStructure::chains`
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ChainIdx(pub u32);

/// Dense index into `InterfaceMap::nodes`
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct PatchAtomIdx(pub u32);

/// Dense index into `InterfaceMap::territories`
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct TerritoryIdx(pub u32);

/// Symmetric identifier for the two opposing sides of an active interface sandwich.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum InterfaceSide {
    /// Primary / foreground group (+n side in Overlay mode; right page in Open-Book mode).
    GroupA,
    /// Secondary / opposing group (-n side in Overlay mode; hinged left page in Open-Book mode).
    GroupB,
}

/// Assignment state of a chain in the `Complex` workspace tab.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChainAssignment {
    GroupA,
    GroupB,
    Unassigned, // Rendered desaturated in the Complex tab; excluded from Interface tab
}

/// User-configurable name and chain membership for one side of the interface sandwich.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PartnerGroup {
    pub side: InterfaceSide,
    pub name: String,               // User-editable: e.g. "Binder", "Target", "Toxin", "Receptor"
    pub chain_ids: Vec<String>,     // Whole-chain selection for v1 (multi-chain supported)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComplexStructure {
    pub name: String,
    pub chains: Vec<ChainRecord>,
    pub residues: Vec<ResidueRecord>,
    pub atoms: Vec<AtomRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChainRecord {
    pub idx: ChainIdx,
    pub chain_id: String,
    pub residue_range: (u32, u32),  // Contiguous slice in `ComplexStructure::residues`
    pub atom_range: (u32, u32),     // Contiguous slice in `ComplexStructure::atoms`
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResidueRecord {
    pub idx: GlobalResidueIdx,
    pub chain_idx: ChainIdx,
    pub chain_id: String,
    pub seq_num: i32,
    pub ins_code: char,
    pub res_name: [u8; 3],          // e.g. *b"GLU"
    pub sec_struct: SecStruct,      // Helix | Sheet | Coil
    pub backbone_phi_psi: Option<(f32, f32)>,
    pub atom_range: (u32, u32),     // Contiguous slice in `ComplexStructure::atoms`
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AtomRecord {
    pub idx: GlobalAtomIdx,
    pub residue_idx: GlobalResidueIdx,
    pub chain_idx: ChainIdx,
    pub atom_name: [u8; 4],         // e.g. *b"OE1 "
    pub element: Element,
    pub vdw_radius_a: f32,
    pub pos_3d: [f32; 3],
    pub b_factor: f32,
}
```

### Layer 2: 3D Meshes for the `Complex` Workspace Tab (`surface.rs`)

To render the **Complex** tab with live vertex highlighting and independent representation alphas (**Surface**, **Cartoon**, **Spheres**):

```rust
/// 3D triangle mesh for a chain's molecular surface (or the inter-chain interface sheet in Cartoon mode).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SurfaceMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Nearest heavy atom index for each surface vertex (enables O(1) live interface vertex highlighting!)
    pub nearest_atom: Vec<GlobalAtomIdx>,
    /// Triangle index triplets `(i0, i1, i2)` into `vertices`
    pub triangles: Vec<[u32; 3]>,
}
```

> [!TIP]
> Storing `nearest_atom: Vec<GlobalAtomIdx>` on each precomputed chain `SurfaceMesh` means the expensive Marching Cubes triangulation only runs **once** when the `.cif`/`.pdb` file is loaded. Whenever the user reassigns chains between `GroupA` and `GroupB`, the Rust engine updates which `GlobalAtomIdx` atoms are interfacial in `< 2 ms`, and every surface vertex `v` immediately knows whether `nearest_atom[v]` is part of the active interface!

### Layer 3: Dual-Side Flattened Interface for the `Interface` Tab (`InterfaceMap`)

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InterfaceMap {
    pub group_a: PartnerGroup,
    pub group_b: PartnerGroup,
    pub frame: TangentFrame,
    pub territories: Vec<ResidueTerritory>,
    pub nodes: Vec<PatchAtomNode>,
    pub intra_edges: Vec<FirstShellEdge>,
    pub cross_contacts: Vec<CrossInterfaceContact>,
    /// Smooth 3D mid-surface mesh separating GroupA and GroupB (displayed in Cartoon mode in Complex tab)
    pub interface_sheet_mesh: SurfaceMesh,
}
```

*(Note: Self-interfaces within a single chain and residue-range selectors `chain A & resi 50–150`, as well as bookmarking multiple simultaneous interfaces in the `Complex` tab, are deferred to a future iteration so v1 remains focused on whole-chain group assignments.)*
