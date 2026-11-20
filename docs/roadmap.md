# Engineering Roadmap

## Milestone Overview

```
Phase 1: Core Ingestion, Spatial Grid & ΔSASA Extraction (Done)
   │
   ▼
Phase 2: Tangent-Plane SVD, Coupled 2D Relaxation & Voronoi Territory Contours (Done)
   │
   ▼
Phase 3: Initial Interactive HTML5 Canvas Viewer, Gizmo, Sidebars & Context Menu (Done)
   │
   ▼
Phase 4: Module-by-Module Code Audit + Tabbed [Complex | Interface] Live Rust Workspace (Current)
   │
   ▼
Phase 5: Live Redesign Workbench (Rotamers, χ-Minimization, Mutation & PDB/mmCIF Export)
```

---

## Phase 1–3 — Initial Prototype & Core Engine (Completed)

* [x] **1.1–1.5 Core Biophysical Extraction:** `pdbtbx` loader, 3D `SpatialGrid`, 72-point Fibonacci sphere $\Delta\text{SASA}$, and VdW clash / H-bond / salt bridge / buried unsatisfied polar classifier.
* [x] **2.1–2.4 Metric Surface Unwrapping:** Weighted $3 \times 3$ Jacobi SVD tangent frame, 160-step coupled 2D spring + hard-core repulsion + cross-contact registration solver, and Voronoi-clipped Chaikin residue territory contours.
* [x] **3.1–3.4 Interactive `<canvas>` Viewport:** Continuous $t \in [0, 1]$ (3D $\leftrightarrow$ Bas-Relief $\leftrightarrow$ 2D) and $\theta_{\text{book}} \in [0, 1]$ (Overlay $\leftrightarrow$ Open-Book Butterfly) sliders, Blender-style 3D orientation gizmo, collapsible/resizable sidebars, and custom right-click context menu.

---

## Phase 4 — Line-by-Line Code Audit & Tabbed `[Complex | Interface]` Workspace (In Progress)

**Goal:** Audit every module in the codebase together while refactoring from static `Binder`/`Target` loading to the full multi-chain **`[Complex]` $\to$ `[Interface]`** tabbed workflow powered by live Rust computation.

* [ ] **4.1 Module-by-Module Code Audit & Refactor:**
  1. **[`model.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-core/src/model.rs) (In Review):** Replace `PartnerRole::Target | Binder` with symmetric `InterfaceSide::GroupA | GroupB` + arbitrary-named `PartnerGroup { side, name, chain_ids }`, add `ChainRecord` / `ChainIdx` so `ComplexStructure` holds *all* chains in the file, and add `SurfaceMesh`.
  2. **[`ingest.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-core/src/ingest.rs):** Audit `pdbtbx` ingestion and update `load_complex` to load all chains in a structure without pre-filtering to two chains.
  3. **[`spatial.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-core/src/spatial.rs):** Audit the 3D voxel hash grid.
  4. **[`sasa.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-core/src/sasa.rs) & [`physics.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-core/src/physics.rs):** Audit Shrake-Rupley $\Delta\text{SASA}$ and interaction classification parameterized by active `(GroupA, GroupB)` chain membership.
  5. **[`flatten.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-core/src/flatten.rs) & [`contour.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-core/src/contour.rs):** Audit SVD tangent plane, coupled 2D spring relaxation, and Voronoi-clipped territory contours.
  6. **`surface.rs` (New Core Module):** Implement 3D Marching Cubes molecular surface mesh per chain (with `nearest_atom` vertex mapping for instant interface vertex highlighting) and the approximated inter-chain interface sheet mesh for Cartoon mode.
  7. **[`main.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-app/src/main.rs) & [`viewport.rs`](file:///mnt/shared/code/smorgasprot/crates/smorgasprot-app/src/viewport.rs):** Audit and upgrade the app to a live Rust server / WASM bridge supporting the **`[1. Complex]`** tab (chain assignment, group naming, Surface/Cartoon/Spheres + independent Alphas, desaturated unassigned chains, live vertex highlighting, and 3D Explode slider) and **`[2. Interface]`** tab.

---

## Phase 5 — Live Redesign Workbench (Rotamers, $\chi$-Minimization, Mutation & Export)

* [ ] **5.1 Sidechain Forward Kinematics & Rotamer Table (`rotamers.rs`):** Embed ideal amino acid sidechain internal coordinates and backbone-dependent Lovell/Dunbrack rotamer table.
* [ ] **5.2 Local $\chi$-Angle Minimizer (`minimize.rs`):** Coordinate-descent / Nelder-Mead torsion optimization on either group of the interface.
* [ ] **5.3 Pinned Territory Updates & Structure Export:** Live preview inside pinned 2D territories, **"Re-relax 2D Map"** trigger, and `.pdb`/`.cif` export.

---

## Deferred / Future v2 Extensions

* **Multiple Bookmarked Interfaces in `Complex` Tab:** Define multiple named interface pairs (e.g. `Interface 1: A <-> C`, `Interface 2: A <-> B`) in the `Complex` tab and jump between them in the `Interface` tab.
* **Intra-Chain Self-Interfaces & Residue-Range Selectors:** Support sub-chain range syntax (`Chain A & resi 50–150`) with covalent linker clash exclusion to unwrap domain–domain interfaces within a single polypeptide chain.
