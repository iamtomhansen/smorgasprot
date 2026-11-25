# Engineering & Rendering Roadmap

## Milestone Overview

```
Phases 1–3: Core Biophysical Extraction, Unwrapping & Initial Interactive Viewer (Done)
   │
   ▼
Phase 4: WGPU Pipeline Migration, Unified OIT & Cartography GPU Rendering (Done)
   │
   ▼
Phase 5: Garnish Workspace, Composable Presentation & MolViz Roadmap (Current)
   ├── 5.1 Composable PresentationStyle, Slice vs. Interface Split & Garnish Tab
   ├── 5.2 Expanded Visual Channel Embeddings (AaChemistry, B-factor Scaling/Color)
   ├── 5.3 Helix–Sheet–Loop DSSP Protein Cartoons
   ├── 5.4 Ball-and-Stick & Per-Selection Representation Layers
   └── 5.5 SVG Vector Export & Live Redesign Workbench
```

---

## Completed Foundation (Phases 1–4)

* [x] **Core Biophysical & Flattening Engine (`smorgasprot-core`):** `pdbtbx` PDB/mmCIF ingestion (with `.gz` & in-memory parsing), 3D `SpatialGrid`, 72-point Fibonacci $\Delta\text{SASA}$, clash/H-bond/salt-bridge physics, Jacobi SVD tangent frame, coupled 2D spring relaxation, Voronoi residue contours, Marching Cubes chain surfaces (`GeodesicPreset`), and derived interface boundary geometry (`SmoothedSdf`, `ContactEnvelope`, `NegativeSpace`).
* [x] **Unified `wgpu` Renderer (`smorgasprot-app::render`):** Migrated both Slice and Cartography ("Sandwich") workspaces onto one shared depth-tested offscreen target with CentroidSorted, WBOIT, and Depth Peeling transparency modes, linear-space blending, analytic 3D billboard sphere shading + `frag_depth`, `GeodesicPreset` unit spheres, calibrated 8-sided 3D prism cartoons, seamless polyline joins + dashed lines, and ChimeraX-style silhouette display borders (`outline.wgsl`). Graceful CPU `DrawList` fallback retained for Glow.
* [x] **App Configuration & WebAssembly:** Hot-reloadable `smorgasprot.ron` config (`ViewParams`, `AnalysisParams`, `RenderingSettings`), live Preferences editor (including `viewport_bg` and `grid_color`), and WebGL2-compatible `wasm32-unknown-unknown` target.

---

## Phase 5 — Composable Presentation & `Garnish` Architecture (Current)

### Core Architectural Principle: Composition Over Inheritance

Every visual representation composes three orthogonal concerns as flat Rust data structures:
1. **Target / Scope** — *What* geometry is drawn (`Dataset` chains/selections vs. `Interface` derived products).
2. **Channel Mapping (`Channels` popover)** — *How* biophysical data maps to visual properties (`Fill color`, `Radius / Width scale`, `Mobility mode`, `Labels`).
3. **Presentation Style (`Presentation` popover)** — *How* geometry is built and rasterized (`Geometry / Quality`, `Shading`, `Face Culling`, `Display Border`, `Wireframe`, and per-channel `AlphaChannels`). **Geometry and SDF meshing settings live inside Presentation.**

### Workspace Split: `1. Slice` | `2. Sandwich` | `3. Garnish`

* **`1. Slice` (Assembly & Interface Inspection):**
  * Focuses on chain partitioning, group naming, palette selection, explode sliders, and **Interface Analysis** layers (**3D Interface Boundary** + **Interface Spheres** with contact lenses like `Interactions` and `BuriedSasa`).
  * Retains lightweight visibility/master-opacity toggles for molecular layers (`Surface`, `Cartoon`, `Spheres`), automatically inheriting the full channel & presentation styling calibrated in **Garnish**.
* **`2. Sandwich` (2D/3D Interface Cartography):**
  * Unwrapped interface map with flatten/open-book morphing, residue territories, contact edges, and labels.
* **`3. Garnish` (PyMOL/ChimeraX-Style Presentation Studio):**
  * Dedicated workspace for calibrating rich molecular representations (`Surface`, `Cartoon`, `Atom Spheres`, and upcoming `Ball-and-Stick` / per-selection layers) without interface clutter.
  * Each representation row uses dedicated **`Channels`** and **`Presentation`** popovers instead of a single catch-all drawer.

### Composable State Model (`PresentationStyle<Q>`)

```rust
pub struct AlphaChannels {
    pub master: f32,    // Layer master fade multiplier
    pub fill: f32,      // Face / interior opacity
    pub border: f32,    // Silhouette outline opacity
    pub wireframe: f32, // Triangle edge overlay opacity
}

pub struct BorderStyle {
    pub width_px: f32,
    pub color: Color32,
}

pub struct WireframeStyle {
    pub enabled: bool,
    pub width_px: f32,
    pub color: Option<Color32>, // None = derived palette::edge_color(fill)
}

pub struct PresentationStyle<Q = ()> {
    pub quality: Q,
    pub shading: MeshShading,    // Flat | Smooth | Unlit
    pub culling: SurfaceCulling, // None | Backfaces | Frontfaces
    pub border: BorderStyle,
    pub wireframe: WireframeStyle,
    pub alphas: AlphaChannels,
}
```

Using a generic capability descriptor (`PresentationCaps`), one shared `presentation_popover` widget renders consistent **Presentation** controls across every Slice, Interface, and Garnish object.

---

### Milestone Checklist

* [x] **5.1 Domain Split, Composable Presentation & `Garnish` Tab:**
  * Extract `AlphaChannels`, `BorderStyle`, `WireframeStyle`, `PresentationStyle<Q>`, and `PresentationCaps`.
  * Thread independent `border_alpha` through `GpuMeshDraw`, `GpuLineDraw`, and `GpuDiscDraw`.
  * Separate **Interface Spheres** (Slice tab → Interface section) from **Complex Spheres** (all-atom molecular sphere representation).
  * Add `WorkspaceTab::Garnish` (`3. Garnish`) owning shared molecular representation styles that carry directly into `1. Slice`, and replace single "Customize" popovers with dedicated **Channels** and **Presentation** popovers.
* [x] **5.2 Expanded Visual Channel Embeddings:**
  * Support full per-atom/per-residue channel embeddings (`AaChemistry`, `Bfactor` / `pLDDT`, `Chain`, `InterfaceSide`, `InterfaceMembership`) across Surfaces, Cartoons (`Prism8` & `Capsule2D`), and Spheres.
  * Allow scalar channels (such as B-factor / mobility) to modulate cartoon ribbon thickness (putty/sausage style) in addition to sphere radii.
* [x] **5.3 Helix–Sheet–Loop Protein Cartoons:**
  * Renamed Cα trace to **Ribbon** and added procedural DSSP **Cartoon** (`Helix`, `Sheet`, `Loop`) with `Schematic` (cylinders & boxes) and `Detailed` (helical ribbons, β-strand arrows, spline-smoothed `Loop` tubes) plus parameterized smoothing/pleat/arrow controls.
* [x] **5.4 Ball-and-Stick, Upgraded 3D Sphere Impostors & Sidechain Scopes:**
  * Upgraded 2-triangle 3D sphere impostors (`disc.wgsl`) with world-locked key lighting, shadow-side bounce fill, rim sheen, Blinn-Phong speculars, and exact spherical `frag_depth`.
  * Added 2D/3D **Ball-and-Stick** (`Flat2D`, `Impostor3D`, `MeshCoarse`, `MeshFine`) with midpoint-split/gradient/uniform bond coloring, `BondOrder` double/triple/aromatic toggles, and sidechain/interface selection shortcuts (`AllAtoms`, `SidechainsAndLigands`, `InterfaceSidechains`, `InterfaceResidues`).
* [ ] **5.5 Image Export & Live Redesign Workbench:**
  * [x] **Viewport PNG Export:** Lossless cropped viewport PNG capture (`File → Export Viewport PNG…`).
  * [ ] *(Stretch goal)* Depth-sorted SVG vector exporter over shared scene descriptors + text labels, and general `Vec<RepresentationLayer>` per-selection stacking.
  * [ ] Complete live rotamer / $\chi$-angle minimization (`rotamers.rs`, `minimize.rs`) and `.pdb`/`.cif` structure export.
