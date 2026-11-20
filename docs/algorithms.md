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
3. **Inclusion Criterion:**
   An atom $i \in \mathcal{S}_A \cup \mathcal{S}_B$ enters the `InterfaceMap` if:
   $$\Delta\text{SASA}_i > 0.15\text{ \AA}^2 \quad \text{or} \quad \min_{j \in \text{opposite}} \|\mathbf{p}_i - \mathbf{p}_j\|_2 \le 4.5\text{ \AA}$$

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
