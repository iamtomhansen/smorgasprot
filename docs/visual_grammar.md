# Visual Grammar, Workspace Tabs & UX

## 1. Blender-Style Tabbed Workspace Header

At the top center of the application bar, a Blender-style workspace switcher lets the user flip between two dedicated environments:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Smorgasprot    [ 🧊 1. Complex (Setup) ]   [ 🗺️ 2. Interface (2D/3D Map) ] │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Tab 1: `Complex` Workspace (3D Sandwich Setup & Live Rust Feedback)

When a user first loads a `.pdb` or `.cif` structure, they enter the **`Complex`** workspace to inspect the full 3D assembly and define the two sides of their interface sandwich:

### Chain & Group Assignment Panel (Left Sidebar)
* **Group A Name & Chain Selector:** Editable group name text input (e.g., `"Binder"`, `"Toxin"`, `"Group A"`) + pill toggle buttons for every chain in the structure (`[A]`, `[B]`, `[C]`, `[D]`, ...).
* **Group B Name & Chain Selector:** Editable group name text input (e.g., `"Target"`, `"Receptor"`, `"Group B"`) + pill toggle buttons for every chain.
* **Mutual Exclusion & Unassigned Chains:** Clicking a chain for Group A removes it from Group B (and vice versa). Any chain not selected in either group is **Unassigned** and automatically rendered in a **desaturated neutral slate (`#94A3B8`)** in the 3D viewport so the active sandwich stands out clearly.
* **Live Rust Engine Feedback:** Changing any chain assignment calls the Rust engine to recompute the interface in real time, updating the highlighted interfacial surface vertices and reporting live summary metrics ($\Delta\text{SASA}$, interfacial residue count, clashes, H-bonds).

### 3D Representations & Independent Alpha Controls (Left Sidebar)
Each representation can be toggled on/off independently and has its own dedicated **Alpha (`0.00`–`1.00`)** slider:

| Representation | Toggle & Alpha | Visual Behavior in `Complex` Tab |
| --- | --- | --- |
| **Surface** *(Default: ON)* | `[x] Surface` + $\alpha_{\text{surf}} \in [0, 1]$ | Renders a 3D molecular surface mesh for each chain (e.g. Cool Teal/Blue for Group A, Warm Coral/Slate for Group B, Desaturated Grey for Unassigned). **Vertices belonging to the active interface light up in bright gold/cyan** (or by interaction type: crimson clash / cobalt H-bond). |
| **Cartoon** *(Default: OFF)* | `[ ] Cartoon` + $\alpha_{\text{cartoon}} \in [0, 1]$ | Renders smooth 3D backbone ribbons/tubes for all chains. When Cartoon mode is active (especially if Surface is hidden or transparent), an **approximated inter-chain interface surface mesh** is drawn *between* the Group A and Group B chains to visualize the contact sheet. |
| **Spheres** *(Default: OFF)* | `[ ] Spheres` + $\alpha_{\text{spheres}} \in [0, 1]$ | Renders van der Waals heavy-atom spheres (either interfacial atoms only or all heavy atoms) inside the translucent surface/cartoon context. |

### 3D "Explode" Slider (`explode_t` $\in [0, 1]$)
* **Explode Complex (`0.00` $\leftrightarrow$ `1.00`):** Smoothly translates **Group A** along $+\frac{1}{2} e D_{\max}\hat{\mathbf{n}}$ and **Group B** along $-\frac{1}{2} e D_{\max}\hat{\mathbf{n}}$ (with spring-tween quick buttons `Closed (0.0)` and `Exploded (1.0)`), letting the user pull the complex apart in 3D to inspect the highlighted interface footprint on both molecular surfaces before jumping to the **Interface** tab.

---

## 3. Tab 2: `Interface` Workspace (3D $\leftrightarrow$ Bas-Relief $\leftrightarrow$ 2D Cartography)

Clicking the **`Interface`** tab focuses exclusively on the interfacial atoms and residue territories of **Group A** and **Group B** configured in the **Complex** tab:

| Control | Range / Options | Effect |
| --- | --- | --- |
| **Flatten Interface ($t$)** | `0.00` (3D) $\leftrightarrow$ `0.50` (Bas-Relief) $\leftrightarrow$ `1.00` (2D Map) | Smoothly interpolates interfacial atoms from true 3D coordinates to the relaxed tangent plane $\Pi$. |
| **Open-Book Butterfly ($\theta_{\text{book}}$)** | `0.00` (Overlay) $\leftrightarrow$ `1.00` (Side-by-Side Open-Book) | Hinges **Group B** open by $180^\circ$ around the left spine so **Group B** (left page) and **Group A** (right page) sit side-by-side or overlaid. |
| **Group A Layer (`group_a.name`)** | Show / Hide + Opacity $\alpha_A \in [0, 1]$ | Controls visibility and transparency of **Group A** atoms and residue territories. |
| **Group B Layer (`group_b.name`)** | Show / Hide + Opacity $\alpha_B \in [0, 1]$ | Controls visibility and transparency of **Group B** atoms and residue territories. |
| **Multi-Lens Encoding** | `Interactions` / `Chemistry` / `Modern CPK` / `B-Factor` | Switches atom fill colors and sizing ($\sqrt{\Delta\text{SASA}}$, Uniform, Clash Overlap) across Voronoi-clipped residue territories. |

---

## 4. Shared Viewport Navigation & Ergonomics (Both Tabs)

* **Blender-Style 3D Orientation Gizmo:** Top-right 3D axis indicator showing live orientation of $(u, v, n)$ with clickable axis caps to snap the camera view.
* **Resizable & Collapsible Sidebars:** Left and right panels can be dragged wider/narrower (`220px`–`560px`) or collapsed via `◀` / `▶` buttons.
* **Right-Click Context Menu:** Stationary right-click opens context actions on atoms/territories/chains, while right-click drag pans the camera.
