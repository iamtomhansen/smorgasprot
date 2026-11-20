# Smorgasprot: Interactive 3D/2D Protein Interface Cartography, Inspection & Engineering

## 1. Vision & Workflow

Studying, auditing, and engineering protein–protein interfaces—whether from crystal/cryo-EM structures (toxin–receptor, antibody–antigen, multi-subunit assemblies) or generative design pipelines (ProteinMPNN, Latent-X, AlphaProteo, RFdiffusion)—requires both **3D structural context** (to understand how chains assemble into an interface sandwich) and **2D unwrapped cartography** (to audit every atomic contact simultaneously without back-face occlusion).

Inspired by Blender's workspace tabs, **Smorgasprot** organizes this workflow into two coupled workspaces powered by a **live Rust geometry & biophysics engine** (zero interface math in JS):

### Workspace Tab 1: `Complex` (3D Assembly & Interface Sandwich Setup)
1. **Full-Complex Ingestion:** Load any `.pdb` or `.cif` structure containing any number of polypeptide chains (e.g. a 4-chain biological assembly or multi-subunit complex).
2. **Interactive Group Assignment (`Group A` vs. `Group B`):** Assign one or more chains to **Group A** and **Group B**, and give each group an arbitrary user-defined name (e.g., `"Toxin"` vs. `"Receptor"`, `"Binder"` vs. `"Target"`, `"Heavy + Light"` vs. `"Antigen"`). Unassigned chains remain visible in 3D as a desaturated structural context.
3. **Live Rust Interface Highlighting:** Assigning chains triggers the Rust engine to immediately compute cross-group $\Delta\text{SASA}$ and contacts, highlighting the interfacial surface vertices in real time.
4. **Multi-Representation 3D Controls & "Explode" Slider:**
   * Independently toggle **Surface**, **Cartoon**, and **Spheres** representations, each with its own **Alpha (`0.0`–`1.0`)** slider.
   * In **Surface** mode (default), each chain renders as a 3D triangle mesh whose vertices light up where they participate in the active Group A $\leftrightarrow$ Group B interface.
   * In **Cartoon** mode, an approximated **inter-chain interface surface mesh** is drawn directly between the Group A and Group B ribbons to visualize the contact boundary.
   * Drag the **"Explode" Slider** to smoothly separate Group A and Group B along the interface normal in 3D space.

### Workspace Tab 2: `Interface` (3D $\leftrightarrow$ Bas-Relief $\leftrightarrow$ 2D Cartography)
Once the interface sandwich is configured in the **Complex** tab, switching to the **Interface** tab unwraps the curved 3D interface patches of Group A and Group B onto a shared 2D tangent plane while preserving first-shell interatomic distances ($d_{ij}^{3\text{D}} \le 4.5\text{ \AA}$):
1. **Continuous 3D $\leftrightarrow$ Bas-Relief $\leftrightarrow$ 2D Morphing ($t \in [0, 1]$):** Tween smoothly between true 3D atomic geometry ($t = 0$), an orbitable 3D bas-relief ($t = 0.5$), and a flat 2D cartographic map ($t = 1$).
2. **Overlay vs. Open-Book ("Butterfly") Views ($\theta_{\text{book}} \in [0, 1]$):** Inspect Group A and Group B overlaid in $(u, v)$ space or hinge them open by $180^\circ$ like a book with synchronized hover highlights and interaction arcs.
3. **Multi-Lens Audit & Redesign:** Toggle visual lenses (Interactions, Chemistry, Modern CPK + $\Delta\text{SASA}$, B-factor/pLDDT) inside Voronoi-clipped **residue territories**, and click any residue to inspect contacts, scan rotamers, minimize $\chi$ torsions, or introduce point mutations.

---

## 2. High-Level Architecture

All structural geometry, surface mesh generation, SASA, and 2D spring relaxation run in **`smorgasprot-core` (Rust)**. The frontend UI communicates with the live Rust engine (via WebAssembly or a local Rust RPC server) purely for rendering and user input:

```
┌──────────────────────────────────────────────────────────────────────┐
│                       smorgasprot-core (Rust)                        │
│  • Full-complex PDB / mmCIF ingestion (pdbtbx) -> ComplexStructure   │
│  • 3D Surface Mesh (Marching Cubes), Cartoon & Interface Sheet Mesh  │
│  • Live Group A / Group B chain partitioning & vertex highlighting   │
│  • Fast Shrake-Rupley ΔSASA & contact extraction (Group A <-> B)     │
│  • Shared tangent-plane basis & coupled 2D spring/repulsion solver   │
│  • Voronoi-clipped smooth residue territory contours                 │
│  • Physics & Redesign (clashes, H-bonds, salt bridges, rotamers)     │
└──────────────────────────────────┬───────────────────────────────────┘
                                   │ Live Rust Engine Calls (WASM / RPC)
┌──────────────────────────────────▼───────────────────────────────────┐
│                        smorgasprot-app (Rust)                        │
│  • Blender-style Tabbed Workspace: [1. Complex]  |  [2. Interface]   │
│  • Complex Tab: Surface/Cartoon/Spheres + Alpha, Chain Picker,       │
│    Live Interface Vertex Highlighting, Desaturated Unassigned Chains,│
│    and 3D "Explode" Slider                                           │
│  • Interface Tab: 3D <-> Bas-Relief <-> 2D + Open-Book Butterfly,    │
│    Visual Lenses, Resizable Sidebars, Orientation Gizmo, Context Menu│
└──────────────────────────────────────────────────────────────────────┘
```

---

## 3. Documentation Map

Detailed specifications are split into four modular documents under `docs/`:

* **[`docs/architecture.md`](file:///mnt/shared/code/smorgasprot/docs/architecture.md)** — Crate structure, full-complex data model (`ComplexStructure`, `ChainRecord`, `InterfaceSession`, `PartnerGroup`, `SurfaceMesh`, `InterfaceMap`), and live Rust engine communication.
* **[`docs/algorithms.md`](file:///mnt/shared/code/smorgasprot/docs/algorithms.md)** — Mathematical specifications for 3D molecular surface mesh generation (Gaussian density Marching Cubes), inter-chain interface sheet mesh, 3D Explode kinematics, $\Delta\text{SASA}$, tangent-plane SVD, and coupled 2D relaxation.
* **[`docs/visual_grammar.md`](file:///mnt/shared/code/smorgasprot/docs/visual_grammar.md)** — Blender-style `[Complex]` and `[Interface]` workspace tabs, Surface/Cartoon/Spheres representation controls, desaturated unassigned chains, multi-lens encodings, orientation gizmo, and right-click context menus.
* **[`docs/roadmap.md`](file:///mnt/shared/code/smorgasprot/docs/roadmap.md)** — Phased engineering milestones, module-by-module code audit checklist, and future extensions (multi-interface bookmarks, intra-chain self-interfaces).
