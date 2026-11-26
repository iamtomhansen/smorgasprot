# Algorithms & Mathematical Specifications

## 1. 3D Molecular Surface Mesh & Live Interface Vertex Highlighting (`Complex` Tab)

In the **Complex** tab, the default representation is a 3D molecular surface mesh for each chain whose vertices highlight live when their chain selection forms an interface between **Group A** and **Group B**.

### Fast Gaussian Electron-Density Marching Cubes (`surface.rs`)
1. **Per-Chain Scalar Field:**
   For each chain $c$ with heavy atoms $\{(\mathbf{p}_k, r_k^{\text{vdW}})\}$, define a smooth Gaussian molecular density on a 3D bounding grid (voxel spacing $h \approx 1.1\text{ \AA}$):
   $$\rho_c(\mathbf{x}) = \sum_{k \in c} \exp\!\left( -\beta \left( \frac{\|\mathbf{x} - \mathbf{p}_k\|_2^2}{(r_k^{\text{vdW}})^2} - 1 \right) \right)$$
   with smoothness decay $\beta \approx 1.6$.
2. **Iso-Surface Extraction & Vertex Normals:**
   Extract the solvent-accessible iso-surface $\rho_c(\mathbf{x}) = 1.0$ via Marching Cubes, computing smooth vertex normals $\hat{\mathbf{n}}_v = -\nabla \rho_c(\mathbf{v}) / \|\nabla \rho_c(\mathbf{v})\|_2$ by central differences.
3. **$O(1)$ Live Interface Vertex Mapping:**
   During mesh generation, assign each surface vertex $\mathbf{v}$ its closest heavy atom index:
   $$\text{nearest\_atom}(\mathbf{v}) = \arg\min_{k \in c} \big(\|\mathbf{v} - \mathbf{p}_k\|_2 - r_k^{\text{vdW}}\big)$$
   When the user changes which chains belong to **Group A** and **Group B**, the Rust engine recomputes the set of interfacial atoms $\mathcal{I}_{AB}$ and marks every surface vertex $\mathbf{v}$ with $\text{nearest\_atom}(\mathbf{v}) \in \mathcal{I}_{AB}$ as **highlighted interface surface** without regenerating the Marching Cubes mesh.

### Approximated Inter-Chain Interface Sheet Mesh (for Cartoon Mode)
When viewing chains in **Cartoon** mode in the **Complex** tab, highlighting the backbone ribbon itself is uninformative because sidechains mediate the actual contact surface. Instead, the Rust engine generates an **inter-chain interface sheet mesh** $\Sigma_{AB}$ between Group A and Group B:
1. Take the shared interface tangent frame $\Pi = (\mathbf{o}_\Pi, \hat{\mathbf{e}}_1, \hat{\mathbf{e}}_2, \hat{\mathbf{n}})$ and the midpoints $\mathbf{m}_{ij} = \frac{1}{2}(\mathbf{p}_i + \mathbf{p}_j)$ of all cross-interface contact pairs $(i \in \mathcal{S}_A, j \in \mathcal{S}_B)$ with $d_{ij} \le 5.0\text{ \AA}$.
2. Fit a smooth local radial-basis / kernel-smoothed height field $h_{\Sigma}(u, v)$ over the 2D contact footprint in $(u, v)$ space:
   $$h_{\Sigma}(u, v) = \frac{\sum_{m} w_m(u, v)\, (\mathbf{m}_m - \mathbf{o}_\Pi)\cdot\hat{\mathbf{n}}}{\sum_{m} w_m(u, v)}, \qquad w_m(u, v) = \exp\!\left(-\frac{(u - u_m)^2 + (v - v_m)^2}{2\sigma_\Sigma^2}\right)$$
3. Triangulate the footprint grid cells where $\sum_m w_m(u, v) \ge \tau_{\text{footprint}}$ to produce a smooth 3D contact sheet floating directly between the Group A and Group B cartoon ribbons.

### 3D "Explode" Slider Kinematics (`Complex` Tab)
The **Complex** tab features an **Explode slider** ($e \in [0, 1]$) that pulls **Group A** and **Group B** apart along the interface normal $\hat{\mathbf{n}}$ (oriented from Group B $\to$ Group A) with a maximum separation $D_{\max} \approx 24\text{ \AA}$:
$$\mathbf{p}^{\text{exploded}}(e) = \begin{cases} \mathbf{p}^{3\text{D}} + \frac{1}{2} e\, D_{\max}\, \hat{\mathbf{n}} & \text{if atom/vertex } \in \text{Group A} \\ \mathbf{p}^{3\text{D}} - \frac{1}{2} e\, D_{\max}\, \hat{\mathbf{n}} & \text{if atom/vertex } \in \text{Group B} \\ \mathbf{p}^{3\text{D}} & \text{if chain is Unassigned} \end{cases}$$

---

## 2. Dual-Side Interface Extraction ($\Delta\text{SASA}$ + Contact Shell)

Given the user's current chain assignment into **Group A** ($\mathcal{S}_A$) and **Group B** ($\mathcal{S}_B$) (ignoring `Unassigned` chains):

1. **Spatial Neighborhood Pre-Filter:**
   Query the 3D `SpatialGrid` for heavy atoms in $\mathcal{S}_A$ within $5.0\text{ \AA}$ of $\mathcal{S}_B$ (and vice versa).
2. **Shrake-Rupley Fibonacci Sphere $\Delta\text{SASA}$:**
   For each candidate atom $i$ with radius $R_i = r_i^{\text{vdW}} + r_p$ ($r_p = 1.4\text{ \AA}$), evaluate $N_{\text{pts}} = 72$ Fibonacci sphere points:
   * **Unbound SASA ($A_i^{\text{free}}$):** Occluded only by atoms in the *same* group ($\mathcal{S}_A$ or $\mathcal{S}_B$).
   * **Bound SASA ($A_i^{\text{bound}}$):** Occluded by atoms in *either* $\mathcal{S}_A$ or $\mathcal{S}_B$.
   * **Buried Area:** $\Delta\text{SASA}_i = \max(0, A_i^{\text{free}} - A_i^{\text{bound}})$.
3. **Inclusion Criteria (`InterfaceCriterion`, per interface):**
   Each `InterfacePartition` carries a list of criteria; an atom $i \in \mathcal{S}_A \cup \mathcal{S}_B$
   enters the `InterfaceMap` if it satisfies **any** of them:
   * `BuriedSasa { min_a2 }`: $\Delta\text{SASA}_i > \text{min\_a2}$ (the standard interface definition).
   * `Proximity { cutoff_a }`: $\min_{j \in \text{opposite}} \|\mathbf{p}_i - \mathbf{p}_j\|_2 \le \text{cutoff\_a}$
     (atom centers; searched independently of the SASA neighbor query, so any cutoff is honored).

   Interfaces created at load time use `[BuriedSasa { 0.15 Å² }, Proximity { 4.5 Å }]`:
   $$\Delta\text{SASA}_i > 0.15\text{ \AA}^2 \quad \text{or} \quad \min_{j \in \text{opposite}} \|\mathbf{p}_i - \mathbf{p}_j\|_2 \le 4.5\text{ \AA}$$
   The Slice tab's interface builder (New / Edit) edits the chain groups and criteria of one interface.

### Covalent Bond Detection & Edge Classification (`bonds.rs`, `physics.rs`)
Before classifying a first-shell atom pair as a clash, salt bridge, hydrogen bond, or generic
van der Waals contact, the engine first checks whether the pair is actually covalently bonded —
this matters because some real covalent bonds (disulfides especially) sit well inside the
distance range the other classifiers would otherwise flag as a severe steric clash.

1. **Distance-Based Bond Perception:**
   Computed once per structure (`bonds::detect_bonds`), independent of any single pair's
   interface membership. For every atom pair within a bounded search radius (via `SpatialGrid`),
   a covalent bond is detected if:
   $$\|\mathbf{p}_i - \mathbf{p}_j\|_2 \le r_i^{\text{cov}} + r_j^{\text{cov}} + \delta_0 + \min\!\big(k_B\sqrt{\tfrac{1}{2}(B_i + B_j)},\, \delta_B^{\max}\big)$$
   where $r^{\text{cov}}$ is each element's single-bond covalent radius, sourced via
   `pdbtbx::Element::atomic_radius().covalent_single` (see `pdbtbx`'s own documentation for which
   paper it's drawn from — we only wrap the value, so we don't re-cite its source here),
   $\delta_0 = 0.45\text{ \AA}$ is the base
   tolerance, and the second term widens it conservatively for higher-uncertainty atoms: since
   $B = 8\pi^2\langle u^2\rangle$ (Debye-Waller factor) relates to *mean-square* displacement, the
   extra allowance scales with $\sqrt{B}$ (not $B$ itself), with $k_B = 0.015\text{ \AA}/\sqrt{B}$
   and capped at $\delta_B^{\max} = 0.25\text{ \AA}$ — the cap specifically preserves the margin
   that keeps a severe steric clash from ever being misread as a bond (a clash would need over
   1 Å of van der Waals overlap between two carbons to reach bond range at default settings; see
   `bonds::detect_bonds`'s doc comment for the full derivation). All three constants
   (`BondParams`) are load-time-only for now — no live Preferences wiring exists yet. This is the
   conventional distance-based bond-perception heuristic used by most molecular toolkits (e.g.
   OpenBabel) when no explicit connectivity is given, and works uniformly across standard residues, peptide
   bonds, disulfides, and any hetero group or ligand with no per-residue bonding templates.
2. **Explicit Record Supplement:**
   Legacy `.pdb` files carrying `SSBOND` records contribute their declared disulfides directly
   (via `pdbtbx`), unioned with the geometric result — `pdbtbx` doesn't parse `CONECT` and its
   mmCIF reader doesn't populate bonds at all, so geometry remains the general-purpose method,
   not just a fallback.
3. **Classification (`BondKind`):**
   A detected bond is labeled `Backbone` (N/CA/C/O/OXT to N/CA/C/O/OXT, whether intra-residue or
   the inter-residue peptide link — geometrically confirmed, so a chain break with merely
   sequence-adjacent residue numbers is correctly excluded), `Sidechain` (any other intra-residue
   bond), `Disulfide` (both atoms sulfur), or `Other` (anything else geometry finds, e.g. glycan
   linkages or ligand attachment). `classify_pair_edge` checks bond status before any non-covalent
   classifier — via `EdgeKind::Covalent(BondKind)`, the same classification, not a second one — so
   a real disulfide (~2.05 Å S–S) is never misread as a van der Waals clash. It also excludes 1-3
   pairs (sharing a bonded neighbor, e.g. backbone N...C across Cα) from clash/salt-bridge/H-bond
   scoring, matching Probe/MolProbity's (Word et al. 1999, §2 above) own exclusion rule — without
   it, ordinary bonded-angle geometry gets misread as severe steric clashes.

---

## 3. Shared Interface Tangent Frame ($\Pi$) & Coupled 2D Relaxation (`Interface` Tab)

1. **Weighted SVD Tangent Frame:**
   Compute the $\Delta\text{SASA}$-weighted centroid $\mathbf{o}_\Pi$ and $3 \times 3$ covariance matrix $\mathbf{C}$ of all interfacial atoms $\mathcal{I} = \mathcal{I}_A \cup \mathcal{I}_B$.
   * Extract orthonormal eigenvectors $(\hat{\mathbf{e}}_1, \hat{\mathbf{e}}_2, \hat{\mathbf{n}})$ via Jacobi SVD, orienting $\hat{\mathbf{n}}$ from the Group B centroid toward the Group A centroid ($\hat{\mathbf{n}} \cdot (\bar{\mathbf{p}}_A - \bar{\mathbf{p}}_B) > 0$) and enforcing $\hat{\mathbf{e}}_1 \times \hat{\mathbf{e}}_2 = \hat{\mathbf{n}}$.
2. **Coupled 2D Spring + Repulsion Solver:**
   Unroll initial tangent projections $\mathbf{x}_i^{(0)} = (u_i^{(0)}, v_i^{(0)})$ via 160 iterations of damped relaxation combining:
   * Intra-group first-shell metric springs ($d_{ij}^{3\text{D}} \le 4.5\text{ \AA}$),
   * Same-group hard-core circle repulsion,
   * Residue territory cohesion,
   * Cross-interface contact registration springs (aligning interacting Group A $\leftrightarrow$ Group B pairs in $(u, v)$ space), and
   * Gentle anchor regularization to $\mathbf{x}_i^{(0)}$.
3. **Unified 3D $\leftrightarrow$ Bas-Relief $\leftrightarrow$ 2D & Open-Book Butterfly Kinematics:**
   * **Flatten slider $t \in [0, 1]$:** Linearly interpolates in 3D space between $\mathbf{p}_i^{3\text{D}}$ ($t = 0$), half-flattened 3D bas-relief ($t = 0.5$), and the planar sheet $\mathbf{p}_{\text{flat}}(i; \theta_{\text{book}})$ ($t = 1$).
   * **Open-Book Butterfly slider $\theta_{\text{book}} \in [0, 1]$:** Hinges **Group B** open by angle $\phi = \theta_{\text{book}} \cdot \pi$ around the spine $u = u_{\text{hinge}}$ so Group B unfolds onto the left page while Group A stays on the right page.
