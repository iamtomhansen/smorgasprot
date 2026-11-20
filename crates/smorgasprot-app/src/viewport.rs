//! Self-contained HTML5 `<canvas>` interactive viewer with Blender-style `[1. Complex]` and `[2. Interface]` workspace tabs.

use serde::Serialize;
use smorgasprot_core::{
    ChainIdx, ComplexStructure, Element, InterfaceMap, SurfaceMesh,
    flatten::FlatteningStats,
    surface::build_chain_surface_mesh,
};

#[derive(Serialize, Clone, Debug)]
pub struct ChainVisualData {
    pub chain_id: String,
    pub residue_count: usize,
    pub atom_count: usize,
    pub ca_positions: Vec<[f32; 3]>,
    pub surface_mesh: SurfaceMesh,
}

#[derive(Serialize, Clone, Debug)]
pub struct AtomMeta {
    pub atom_name: String,
    pub res_name: String,
    pub seq_num: i32,
    pub chain_id: String,
    pub element: Element,
    pub vdw_radius_a: f32,
    pub b_factor: f32,
}

#[derive(Serialize, Clone, Debug)]
pub struct ViewerDataset {
    pub id: String,
    pub title: String,
    pub group_a_name: String,
    pub group_b_name: String,
    pub group_a_chains: Vec<String>,
    pub group_b_chains: Vec<String>,
    pub chains: Vec<ChainVisualData>,
    pub map: InterfaceMap,
    pub atom_meta: Vec<AtomMeta>,
    pub stats: FlatteningStats,
}

impl ViewerDataset {
    pub fn from_complex_and_map(
        id: impl Into<String>,
        title: impl Into<String>,
        complex: &ComplexStructure,
        map: InterfaceMap,
        stats: FlatteningStats,
    ) -> Self {
        let mut chains = Vec::with_capacity(complex.chains.len());
        for (c_idx, chain_rec) in complex.chains.iter().enumerate() {
            let mut ca_positions = Vec::new();
            for r_idx in chain_rec.residue_range.0..chain_rec.residue_range.1 {
                let res = &complex.residues[r_idx as usize];
                for a_idx in res.atom_range.0..res.atom_range.1 {
                    let atom = &complex.atoms[a_idx as usize];
                    if &atom.atom_name == b"CA  " {
                        ca_positions.push(atom.pos_3d);
                        break;
                    }
                }
            }

            let surface_mesh = build_chain_surface_mesh(complex, ChainIdx(c_idx as u32));
            chains.push(ChainVisualData {
                chain_id: chain_rec.chain_id.clone(),
                residue_count: (chain_rec.residue_range.1 - chain_rec.residue_range.0) as usize,
                atom_count: (chain_rec.atom_range.1 - chain_rec.atom_range.0) as usize,
                ca_positions,
                surface_mesh,
            });
        }

        let atom_meta = map
            .nodes
            .iter()
            .map(|n| {
                let atom = &complex.atoms[n.global_atom.0 as usize];
                let res = &complex.residues[atom.residue_idx.0 as usize];
                AtomMeta {
                    atom_name: std::str::from_utf8(&atom.atom_name)
                        .unwrap_or("?")
                        .trim()
                        .to_string(),
                    res_name: std::str::from_utf8(&res.res_name)
                        .unwrap_or("UNK")
                        .trim()
                        .to_string(),
                    seq_num: res.seq_num,
                    chain_id: res.chain_id.clone(),
                    element: atom.element,
                    vdw_radius_a: atom.vdw_radius_a,
                    b_factor: atom.b_factor,
                }
            })
            .collect();

        Self {
            id: id.into(),
            title: title.into(),
            group_a_name: complex.group_a_name.clone(),
            group_b_name: complex.group_b_name.clone(),
            group_a_chains: complex.group_a_chains.clone(),
            group_b_chains: complex.group_b_chains.clone(),
            chains,
            map,
            atom_meta,
            stats,
        }
    }
}

/// Generates a single self-contained interactive HTML5 `<canvas>` viewer containing `datasets`.
pub fn generate_viewer_html(datasets: &[ViewerDataset]) -> Result<String, String> {
    let json_payload =
        serde_json::to_string(datasets).map_err(|e| format!("JSON serialization error: {e}"))?;
    Ok(HTML_TEMPLATE.replace("/*__SMORGASPROT_DATASETS_JSON__*/", &json_payload))
}

const HTML_TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Smorgasprot — Interactive 3D/2D Protein Interface Cartography</title>
<style>
  :root {
    --bg: #0f172a;
    --panel: #1e293b;
    --panel-border: #334155;
    --text: #f8fafc;
    --muted: #94a3b8;
    --accent: #38bdf8;
    --side-a: #0ea5e9;
    --side-b: #f97316;
    --clash: #f21414;
    --hbond: #2563eb;
    --salt: #9d14ff;
    --warn: #f59e0b;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Inter, sans-serif;
    background: var(--bg);
    color: var(--text);
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
    user-select: none;
  }
  #topbar {
    height: 44px;
    background: #090e1a;
    border-bottom: 1px solid var(--panel-border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 16px;
    z-index: 30;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 700;
    font-size: 14px;
  }
  .workspace-tabs {
    display: flex;
    gap: 6px;
    background: #1e293b;
    padding: 4px;
    border-radius: 8px;
    border: 1px solid var(--panel-border);
  }
  .tab-btn {
    background: transparent;
    border: none;
    color: #94a3b8;
    padding: 5px 14px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.12s;
  }
  .tab-btn:hover { color: #fff; }
  .tab-btn.active {
    background: #0284c7;
    color: #fff;
    box-shadow: 0 1px 4px rgba(0,0,0,0.3);
  }
  #main-area {
    flex: 1;
    display: flex;
    overflow: hidden;
  }
  #sidebar-left {
    width: 345px;
    background: var(--panel);
    border-right: 1px solid var(--panel-border);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    overflow-y: auto;
    z-index: 10;
  }
  #viewport-container {
    flex: 1;
    position: relative;
    background: #f8fafc;
    overflow: hidden;
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
  }
  canvas:active { cursor: grabbing; }
  #sidebar-right {
    width: 340px;
    background: var(--panel);
    border-left: 1px solid var(--panel-border);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    overflow-y: auto;
    z-index: 10;
  }
  #sidebar-left.collapsed, #sidebar-right.collapsed {
    width: 0 !important;
    padding-left: 0 !important;
    padding-right: 0 !important;
    border: none !important;
    overflow: hidden !important;
  }
  .resizer {
    width: 5px;
    background: transparent;
    cursor: col-resize;
    z-index: 20;
    transition: background 0.15s;
  }
  .resizer:hover, .resizer.dragging { background: var(--accent); }
  .sidebar-toggle {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    z-index: 25;
    width: 20px;
    height: 48px;
    background: rgba(15, 23, 42, 0.88);
    border: 1px solid var(--panel-border);
    color: var(--text);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font-size: 11px;
    padding: 0;
  }
  #toggle-left-btn { left: 0; border-radius: 0 6px 6px 0; border-left: none; }
  #toggle-right-btn { right: 0; border-radius: 6px 0 0 6px; border-right: none; }
  .card {
    background: rgba(15, 23, 42, 0.55);
    border: 1px solid var(--panel-border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .card-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--accent);
  }
  label {
    font-size: 12px;
    color: #cbd5e1;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  input[type="text"], select, button {
    background: #0f172a;
    color: var(--text);
    border: 1px solid var(--panel-border);
    border-radius: 6px;
    padding: 5px 9px;
    font-size: 12px;
    transition: all 0.12s;
  }
  select, button { cursor: pointer; }
  input[type="text"]:focus, select:hover, button:hover {
    border-color: var(--accent);
    outline: none;
  }
  button.active {
    background: #0284c7;
    border-color: #38bdf8;
    font-weight: 600;
  }
  .btn-row { display: flex; gap: 6px; }
  .btn-row button { flex: 1; padding: 6px 4px; text-align: center; }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .chain-matrix {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .chain-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: rgba(15, 23, 42, 0.7);
    border: 1px solid #334155;
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 12px;
  }
  .chain-pills { display: flex; gap: 4px; }
  .pill-btn {
    padding: 3px 8px;
    font-size: 11px;
    border-radius: 4px;
  }
  .pill-btn.side-a.active { background: #0284c7; border-color: #38bdf8; color: #fff; }
  .pill-btn.side-b.active { background: #ea580c; border-color: #fb923c; color: #fff; }
  .pill-btn.none.active { background: #475569; border-color: #94a3b8; color: #fff; }
  .stat-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    font-size: 12px;
  }
  .stat-box {
    background: rgba(30, 41, 59, 0.7);
    border: 1px solid rgba(51, 65, 85, 0.8);
    border-radius: 6px;
    padding: 8px;
  }
  .stat-val { font-size: 14px; font-weight: 700; color: #fff; }
  .stat-lbl { font-size: 10px; color: var(--muted); }
  .legend-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 11px;
    color: #cbd5e1;
  }
  .swatch {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    margin-right: 4px;
    vertical-align: middle;
  }
  #hud-overlay {
    position: absolute;
    top: 12px;
    left: 26px;
    background: rgba(15, 23, 42, 0.86);
    color: #f8fafc;
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 12px;
    pointer-events: none;
    border: 1px solid #334155;
    box-shadow: 0 4px 12px rgba(0,0,0,0.18);
  }
  #context-menu {
    position: fixed;
    display: none;
    background: rgba(15, 23, 42, 0.96);
    border: 1px solid #475569;
    border-radius: 8px;
    padding: 6px;
    min-width: 220px;
    box-shadow: 0 10px 25px rgba(0,0,0,0.45);
    z-index: 100;
    backdrop-filter: blur(6px);
  }
  .ctx-header {
    font-size: 11px;
    font-weight: 700;
    color: var(--accent);
    padding: 5px 8px;
    border-bottom: 1px solid #334155;
    margin-bottom: 4px;
  }
  .ctx-item {
    font-size: 12px;
    color: #e2e8f0;
    padding: 6px 8px;
    border-radius: 5px;
    cursor: pointer;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .ctx-item:hover { background: #0284c7; color: #fff; }
  .ctx-sep { height: 1px; background: #334155; margin: 4px 0; }
  .contact-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 290px;
    overflow-y: auto;
  }
  .contact-item {
    font-size: 11px;
    padding: 6px 8px;
    border-radius: 5px;
    background: rgba(15, 23, 42, 0.75);
    border-left: 3px solid #64748b;
    display: flex;
    justify-content: space-between;
  }
  .contact-item.Clash { border-left-color: var(--clash); background: rgba(242, 20, 20, 0.14); }
  .contact-item.HBond { border-left-color: var(--hbond); background: rgba(37, 99, 235, 0.14); }
  .contact-item.SaltBridge { border-left-color: var(--salt); background: rgba(157, 20, 255, 0.14); }
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: #cbd5e1;
    cursor: pointer;
  }
</style>
</head>
<body>

<div id="topbar">
  <div class="brand">
    <span>Smorgasprot</span>
    <select id="dataset-select" style="min-width:270px;"></select>
  </div>
  <div class="workspace-tabs">
    <button class="tab-btn active" data-workspace-tab="Complex">🧊 1. Complex (3D Sandwich Setup)</button>
    <button class="tab-btn" data-workspace-tab="Interface">🗺️ 2. Interface (2D/3D Cartography)</button>
  </div>
  <div style="font-size:11px; color:var(--muted);">
    Right-Click: Context Menu • Click Gizmo: Snap Axis
  </div>
</div>

<div id="main-area">
  <div id="sidebar-left">
    <!-- TAB 1: COMPLEX WORKSPACE CONTROLS -->
    <div id="controls-complex" style="display:flex; flex-direction:column; gap:12px;">
      <div class="card">
        <div class="card-title">1. Interface Group Names</div>
        <label>
          <span style="color:#38bdf8; font-weight:600;">Group A Name</span>
          <input type="text" id="group-a-name-input" value="Group A" style="width:150px;">
        </label>
        <label>
          <span style="color:#fb923c; font-weight:600;">Group B Name</span>
          <input type="text" id="group-b-name-input" value="Group B" style="width:150px;">
        </label>
      </div>

      <div class="card">
        <div class="card-title">2. Chain Assignment (Sandwich Builder)</div>
        <div style="font-size:11px; color:var(--muted);">
          Assign chains to Group A or Group B. Unassigned chains render desaturated in 3D.
        </div>
        <div class="chain-matrix" id="chain-matrix"></div>
      </div>

      <div class="card">
        <div class="card-title">3. 3D Representations & Alpha</div>
        <label class="checkbox-row">
          <input type="checkbox" id="cx-show-surface" checked>
          <span><strong>Surface Mesh</strong> (Highlights Interface Vertices)</span>
        </label>
        <label>
          <span>Surface Alpha</span>
          <span id="cx-surface-alpha-val">0.82</span>
        </label>
        <input type="range" id="cx-surface-alpha" min="0.05" max="1" step="0.03" value="0.82">

        <div class="ctx-sep"></div>

        <label class="checkbox-row">
          <input type="checkbox" id="cx-show-cartoon" checked>
          <span><strong>Cartoon Ribbon</strong> (+ Inter-Chain Sheet Mesh)</span>
        </label>
        <label>
          <span>Cartoon Alpha</span>
          <span id="cx-cartoon-alpha-val">0.70</span>
        </label>
        <input type="range" id="cx-cartoon-alpha" min="0.05" max="1" step="0.03" value="0.70">

        <div class="ctx-sep"></div>

        <label class="checkbox-row">
          <input type="checkbox" id="cx-show-spheres">
          <span><strong>Spheres</strong> (Interfacial Heavy Atoms)</span>
        </label>
        <label>
          <span>Spheres Alpha</span>
          <span id="cx-spheres-alpha-val">0.90</span>
        </label>
        <input type="range" id="cx-spheres-alpha" min="0.05" max="1" step="0.03" value="0.90">
      </div>

      <div class="card">
        <div class="card-title">4. 3D Complex Explode Slider</div>
        <label>
          <span>Explode Separation (Along ±n)</span>
          <strong id="explode-val">0.00 (0.0 Å)</strong>
        </label>
        <input type="range" id="explode-slider" min="0" max="1" step="0.01" value="0.0">
        <div class="btn-row">
          <button data-tween-explode="0.0" class="active">Closed (0 Å)</button>
          <button data-tween-explode="0.5">Half (12 Å)</button>
          <button data-tween-explode="1.0">Exploded (24 Å)</button>
        </div>
      </div>

      <button id="go-to-interface-btn" class="active" style="padding:10px; font-weight:700;">
        Inspect Unwrapped Interface Map →
      </button>
    </div>

    <!-- TAB 2: INTERFACE WORKSPACE CONTROLS -->
    <div id="controls-interface" style="display:none; flex-direction:column; gap:12px;">
      <div class="card">
        <div class="card-title">1. Active Interface Sandwich</div>
        <div class="stat-grid" id="dataset-stats"></div>
      </div>

      <div class="card">
        <div class="card-title">2. 3D ↔ 2D Flattening (t)</div>
        <label>
          <span>Flatten Slider (t)</span>
          <strong id="flatten-val">1.00 (2D Map)</strong>
        </label>
        <input type="range" id="flatten-slider" min="0" max="1" step="0.01" value="1.0">
        <div class="btn-row">
          <button data-tween-t="0.0">3D (0.0)</button>
          <button data-tween-t="0.5">Bas-Relief (0.5)</button>
          <button data-tween-t="1.0" class="active">2D Map (1.0)</button>
        </div>
        <label class="checkbox-row" style="margin-top:4px;">
          <input type="checkbox" id="use-relaxed-uv" checked>
          <span>Use Spring-Relaxed 2D (vs. Raw Tangent Plane)</span>
        </label>
      </div>

      <div class="card">
        <div class="card-title">3. Overlay ↔ Open-Book Butterfly (θ)</div>
        <label>
          <span>Book Hinge Angle (θ)</span>
          <strong id="book-val">0.00 (Overlay)</strong>
        </label>
        <input type="range" id="book-slider" min="0" max="1" step="0.01" value="0.0">
        <div class="btn-row">
          <button data-tween-book="0.0" class="active">Overlaid (0°)</button>
          <button data-tween-book="1.0">Open-Book Butterfly (180°)</button>
        </div>
      </div>

      <div class="card">
        <div class="card-title">4. Visual Lens & Sizing</div>
        <label>
          <span>Color Lens</span>
          <select id="lens-select">
            <option value="Interactions">Interactions (Clash / H-Bond / Salt)</option>
            <option value="Chemistry">Residue Chemistry</option>
            <option value="ModernCpk">Modern CPK + ΔSASA</option>
            <option value="Bfactor">B-Factor / pLDDT</option>
          </select>
        </label>
        <label>
          <span>Atom Radius</span>
          <select id="size-select">
            <option value="SqrtSasa">√ΔSASA (Core → Rim)</option>
            <option value="Uniform">Uniform Geometric</option>
            <option value="Clash">Scaled by Clash Overlap</option>
          </select>
        </label>
        <div class="legend-row" id="lens-legend"></div>
      </div>

      <div class="card">
        <div class="card-title">5. Side Opacity & Overlays</div>
        <label>
          <span id="side-a-opacity-lbl">Group A Opacity</span>
          <span id="side-a-alpha-val">1.00</span>
        </label>
        <input type="range" id="side-a-alpha" min="0" max="1" step="0.05" value="1.0">

        <label>
          <span id="side-b-opacity-lbl">Group B Opacity</span>
          <span id="side-b-alpha-val">0.35</span>
        </label>
        <input type="range" id="side-b-alpha" min="0" max="1" step="0.05" value="0.35">

        <label class="checkbox-row">
          <input type="checkbox" id="show-territories" checked>
          <span>Residue Territory Contours & Labels</span>
        </label>
        <label class="checkbox-row">
          <input type="checkbox" id="show-intra-edges" checked>
          <span>First-Shell Intra-Side Edges</span>
        </label>
        <label class="checkbox-row">
          <input type="checkbox" id="show-cross-arcs" checked>
          <span>Cross-Interface H-Bond / Clash Arcs</span>
        </label>
        <label class="checkbox-row">
          <input type="checkbox" id="show-backbone" checked>
          <span>3D Full-Chain Cα Backbone Trace</span>
        </label>
      </div>
    </div>

    <button id="reset-camera-btn">Align Camera to Tangent Plane Π</button>
  </div>

  <div id="resizer-left" class="resizer" title="Drag to resize left panel"></div>

  <div id="viewport-container">
    <button id="toggle-left-btn" class="sidebar-toggle" title="Toggle Left Panel">◀</button>
    <button id="toggle-right-btn" class="sidebar-toggle" title="Toggle Right Panel">▶</button>
    <canvas id="canvas"></canvas>
    <div id="hud-overlay">
      Left-Drag: Orbit (3D) / Pan (2D) • Right-Drag: Pan • Right-Click: Context Menu • Click Gizmo: Snap Axis
    </div>
  </div>

  <div id="resizer-right" class="resizer" title="Drag to resize right panel"></div>

  <div id="sidebar-right">
    <div class="card">
      <div class="card-title">Interface & Unwrapping Summary</div>
      <div class="stat-grid" id="relax-diagnostics"></div>
    </div>

    <div class="card" style="flex:1;">
      <div class="card-title">Selected Residue / Atom Inspector</div>
      <div id="inspector-content" style="font-size:12px; color:#cbd5e1; display:flex; flex-direction:column; gap:10px;">
        <div style="color:var(--muted);">Hover or click any atom or residue territory to inspect its 3D/2D metrics and interactions.</div>
      </div>
    </div>
  </div>
</div>

<div id="context-menu"></div>

<script>
const DATASETS = /*__SMORGASPROT_DATASETS_JSON__*/;

const state = {
  workspaceTab: "Complex", // "Complex" | "Interface"
  datasetIdx: 0,
  // Complex Tab state
  cxShowSurface: true,
  cxSurfaceAlpha: 0.82,
  cxShowCartoon: true,
  cxCartoonAlpha: 0.70,
  cxShowSpheres: false,
  cxSpheresAlpha: 0.90,
  explodeT: 0.0,
  targetExplodeT: 0.0,
  // Interface Tab state
  flattenT: 1.0,
  targetFlattenT: 1.0,
  bookTheta: 0.0,
  targetBookTheta: 0.0,
  useRelaxedUv: true,
  lens: "Interactions",
  sizeMode: "SqrtSasa",
  sideAAlpha: 1.0,
  sideBAlpha: 0.35,
  showTerritories: true,
  showIntraEdges: true,
  showCrossArcs: true,
  showBackbone: true,
  // Shared Camera in TangentPlane local coordinates (u, v, n)
  camYaw: 0.45,
  camPitch: 0.30,
  camZoom: 14.5,
  camPanX: 0.0,
  camPanY: 0.0,
  hoveredAtomIdx: null,
  selectedTerritoryIdx: null,
  animating: false,
};

const canvas = document.getElementById("canvas");
const ctx = canvas.getContext("2d");

function currentDataset() {
  return DATASETS[state.datasetIdx];
}

// Vector helpers
function vec3Sub(a, b) { return [a[0]-b[0], a[1]-b[1], a[2]-b[2]]; }
function vec3Add(a, b) { return [a[0]+b[0], a[1]+b[1], a[2]+b[2]]; }
function vec3Scale(a, s) { return [a[0]*s, a[1]*s, a[2]*s]; }
function vec3Dot(a, b) { return a[0]*b[0] + a[1]*b[1] + a[2]*b[2]; }
function vec3Lerp(a, b, t) { return [a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t, a[2]+(b[2]-a[2])*t]; }

function worldToPlaneLocal(pos3d, plane) {
  const rel = vec3Sub(pos3d, plane.origin);
  return [vec3Dot(rel, plane.axis_u), vec3Dot(rel, plane.axis_v), vec3Dot(rel, plane.normal)];
}

function computeInterfaceAtomLocal3D(pos3d, posUv, role, plane, flattenT, bookTheta) {
  const p3Local = worldToPlaneLocal(pos3d, plane);
  const u = posUv[0];
  const v = posUv[1];
  const sheetOffset = 0.08;
  let pFlatLocal;
  if (role === "SideA") {
    pFlatLocal = [u, v, sheetOffset];
  } else {
    const phi = bookTheta * Math.PI;
    const du = u - plane.hinge_u;
    const uRot = plane.hinge_u + du * Math.cos(phi);
    const nRot = -du * Math.sin(phi) - sheetOffset * Math.cos(phi);
    pFlatLocal = [uRot, v, nRot];
  }
  return vec3Lerp(p3Local, pFlatLocal, flattenT);
}

function projectLocalPoint(localPt, width, height) {
  const cy = Math.cos(state.camYaw), sy = Math.sin(state.camYaw);
  const cp = Math.cos(state.camPitch), sp = Math.sin(state.camPitch);
  const x0 = localPt[0], y0 = localPt[1], z0 = localPt[2];
  const x1 = x0 * cy + z0 * sy;
  const z1 = -x0 * sy + z0 * cy;
  const y2 = y0 * cp - z1 * sp;
  const z2 = y0 * sp + z1 * cp;

  const sx = width * 0.5 + state.camPanX + x1 * state.camZoom;
  const syScreen = height * 0.5 + state.camPanY - y2 * state.camZoom;
  return [sx, syScreen, z2];
}

function getChainRole(ds, chainId) {
  if (ds.group_a_chains.includes(chainId)) return "SideA";
  if (ds.group_b_chains.includes(chainId)) return "SideB";
  return "Unassigned";
}

function getExplodeOffsetLocal(role) {
  const maxExplodeA = 24.0;
  const d = 0.5 * state.explodeT * maxExplodeA;
  if (role === "SideA") return [0, 0, d];
  if (role === "SideB") return [0, 0, -d];
  return [0, 0, 0];
}

function worstClashOnNode(node) {
  let w = 0;
  for (const inter of node.interactions) {
    if (inter.kind === "Clash" && inter.overlap_a > w) w = inter.overlap_a;
  }
  return w;
}

function hasInteractionKind(node, kindName, crossOnly) {
  return node.interactions.some(inter => (!crossOnly || inter.is_cross_interface) && inter.kind === kindName);
}

function getAtomFillColor(node, meta) {
  if (state.lens === "Interactions") {
    if (worstClashOnNode(node) > 0.05) return "#f21414";
    if (hasInteractionKind(node, "SaltBridge", true)) return "#9d14ff";
    if (hasInteractionKind(node, "HBond", true)) return "#2563eb";
    return node.role === "SideA" ? "#0ea5e9" : "#fb923c";
  } else if (state.lens === "Chemistry") {
    const r = meta.res_name;
    if (["ALA","VAL","LEU","ILE","MET"].includes(r)) return "#94a3b8";
    if (["PHE","TYR","TRP"].includes(r)) return "#f59e0b";
    if (["SER","THR","ASN","GLN","CYS"].includes(r)) return "#06b6d4";
    if (["LYS","ARG","HIS"].includes(r)) return "#3b82f6";
    if (["ASP","GLU"].includes(r)) return "#ef4444";
    return "#a855f7";
  } else if (state.lens === "ModernCpk") {
    if (meta.element === "N") return "#2563eb";
    if (meta.element === "O") return "#dc2626";
    if (meta.element === "S") return "#d97706";
    return "#475569";
  } else {
    const b = Math.max(20, Math.min(100, meta.b_factor));
    const norm = (b - 20) / 80;
    if (norm > 0.75) return "#0053d6";
    if (norm > 0.5) return "#65cbf3";
    if (norm > 0.25) return "#ffdb13";
    return "#ff7d45";
  }
}

function getAtomRadiusPx(node) {
  const baseScale = state.camZoom * 0.28;
  if (state.sizeMode === "Uniform") {
    return Math.max(3.5, baseScale * 1.15);
  } else if (state.sizeMode === "Clash") {
    const boost = 1.0 + worstClashOnNode(node) * 2.2;
    return Math.max(3.5, baseScale * boost);
  } else {
    const s = Math.sqrt(Math.max(1.5, node.buried_sasa_a2));
    return Math.max(3.5, baseScale * (0.65 + 0.25 * s));
  }
}

function renderComplexTab(W, H, ds) {
  const map = ds.map;
  const plane = map.plane;

  // Build O(1) lookup set of interfacial GlobalAtomIdx from Rust InterfaceMap::nodes
  const interfacialGlobalAtoms = new Set();
  for (const n of map.nodes) {
    interfacialGlobalAtoms.add(n.global_atom);
  }

  // 1. Collect and depth-sort 3D surface mesh triangles across all chains (+ inter-chain interface sheet in Cartoon mode)
  const drawTriangles = [];

  if (state.cxShowSurface && state.cxSurfaceAlpha > 0.02) {
    for (const ch of ds.chains) {
      const role = getChainRole(ds, ch.chain_id);
      const explodeShift = getExplodeOffsetLocal(role);
      const mesh = ch.surface_mesh;

      const projVerts = mesh.vertices.map((v3d, vIdx) => {
        const loc = vec3Add(worldToPlaneLocal(v3d, plane), explodeShift);
        const scr = projectLocalPoint(loc, W, H);
        const isInterfacial = role !== "Unassigned" && interfacialGlobalAtoms.has(mesh.nearest_atom[vIdx]);
        return { sx: scr[0], sy: scr[1], depth: scr[2], isInterfacial, normal: mesh.normals[vIdx] };
      });

      for (const [i0, i1, i2] of mesh.triangles) {
        const v0 = projVerts[i0], v1 = projVerts[i1], v2 = projVerts[i2];
        const depth = (v0.depth + v1.depth + v2.depth) / 3.0;
        const ifaceCount = (v0.isInterfacial ? 1 : 0) + (v1.isInterfacial ? 1 : 0) + (v2.isInterfacial ? 1 : 0);
        // Simple directional shading from normal
        const nx = (v0.normal[0] + v1.normal[0] + v2.normal[0]) / 3;
        const ny = (v0.normal[1] + v1.normal[1] + v2.normal[1]) / 3;
        const nz = (v0.normal[2] + v1.normal[2] + v2.normal[2]) / 3;
        const shade = Math.max(0.35, Math.min(1.0, 0.65 + 0.35 * (nx * 0.3 + ny * 0.5 + nz * 0.8)));

        let fillColor, strokeColor, alpha;
        if (role === "Unassigned") {
          // Desaturated structural context for non-interface chains (e.g. B and D in 7JVB)
          const g = Math.round(165 * shade);
          fillColor = `rgb(${g}, ${g+5}, ${g+12})`;
          strokeColor = "rgba(100,116,139,0.22)";
          alpha = state.cxSurfaceAlpha * 0.45;
        } else if (ifaceCount >= 1) {
          // Highlighted interfacial vertices on the molecular surface!
          fillColor = role === "SideA" ? "#facc15" : "#a855f7";
          strokeColor = "#0f172a";
          alpha = Math.min(1.0, state.cxSurfaceAlpha + 0.15);
        } else if (role === "SideA") {
          const r = Math.round(14 * shade), g = Math.round(165 * shade), b = Math.round(233 * shade);
          fillColor = `rgb(${r}, ${g}, ${b})`;
          strokeColor = "rgba(2,132,199,0.28)";
          alpha = state.cxSurfaceAlpha;
        } else {
          const r = Math.round(249 * shade), g = Math.round(115 * shade), b = Math.round(22 * shade);
          fillColor = `rgb(${r}, ${g}, ${b})`;
          strokeColor = "rgba(234,88,12,0.28)";
          alpha = state.cxSurfaceAlpha;
        }

        drawTriangles.push({
          x0: v0.sx, y0: v0.sy,
          x1: v1.sx, y1: v1.sy,
          x2: v2.sx, y2: v2.sy,
          depth, fillColor, strokeColor, alpha
        });
      }
    }
  }

  // If Cartoon mode is active, draw the Approximated Inter-Chain Interface Sheet Mesh between Group A and Group B!
  if (state.cxShowCartoon && state.cxCartoonAlpha > 0.02 && map.interface_sheet_mesh.triangles.length > 0) {
    const sheet = map.interface_sheet_mesh;
    const projSheet = sheet.vertices.map(v3d => {
      const loc = worldToPlaneLocal(v3d, plane);
      const scr = projectLocalPoint(loc, W, H);
      return { sx: scr[0], sy: scr[1], depth: scr[2] };
    });
    for (const [i0, i1, i2] of sheet.triangles) {
      const v0 = projSheet[i0], v1 = projSheet[i1], v2 = projSheet[i2];
      drawTriangles.push({
        x0: v0.sx, y0: v0.sy,
        x1: v1.sx, y1: v1.sy,
        x2: v2.sx, y2: v2.sy,
        depth: (v0.depth + v1.depth + v2.depth) / 3.0,
        fillColor: "rgba(250, 204, 21, 0.72)",
        strokeColor: "rgba(217, 119, 6, 0.85)",
        alpha: state.cxCartoonAlpha * 0.88
      });
    }
  }

  drawTriangles.sort((a, b) => a.depth - b.depth);
  for (const tri of drawTriangles) {
    ctx.save();
    ctx.globalAlpha = tri.alpha;
    ctx.beginPath();
    ctx.moveTo(tri.x0, tri.y0);
    ctx.lineTo(tri.x1, tri.y1);
    ctx.lineTo(tri.x2, tri.y2);
    ctx.closePath();
    ctx.fillStyle = tri.fillColor;
    ctx.fill();
    ctx.strokeStyle = tri.strokeColor;
    ctx.lineWidth = 0.65;
    ctx.stroke();
    ctx.restore();
  }

  // 2. Draw Cartoon Backbone Ribbons
  if (state.cxShowCartoon && state.cxCartoonAlpha > 0.02) {
    for (const ch of ds.chains) {
      if (ch.ca_positions.length < 2) continue;
      const role = getChainRole(ds, ch.chain_id);
      const explodeShift = getExplodeOffsetLocal(role);
      ctx.save();
      ctx.globalAlpha = role === "Unassigned" ? state.cxCartoonAlpha * 0.35 : state.cxCartoonAlpha;
      ctx.strokeStyle = role === "SideA" ? "#0284c7" : (role === "SideB" ? "#ea580c" : "#94a3b8");
      ctx.lineWidth = Math.max(3.0, state.camZoom * 0.28);
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.beginPath();
      ch.ca_positions.forEach((pos3d, idx) => {
        const loc = vec3Add(worldToPlaneLocal(pos3d, plane), explodeShift);
        const scr = projectLocalPoint(loc, W, H);
        if (idx === 0) ctx.moveTo(scr[0], scr[1]);
        else ctx.lineTo(scr[0], scr[1]);
      });
      ctx.stroke();

      // Label chain ID at chain centroid
      const midCa = ch.ca_positions[Math.floor(ch.ca_positions.length / 2)];
      const midLoc = vec3Add(worldToPlaneLocal(midCa, plane), explodeShift);
      const midScr = projectLocalPoint(midLoc, W, H);
      ctx.fillStyle = "#0f172a";
      ctx.font = "700 12px Inter, sans-serif";
      ctx.textAlign = "center";
      const roleTag = role === "SideA" ? ds.group_a_name : (role === "SideB" ? ds.group_b_name : "Unassigned");
      ctx.fillText(`Chain ${ch.chain_id} (${roleTag})`, midScr[0], midScr[1]);
      ctx.restore();
    }
  }

  // 3. Draw Interfacial Heavy-Atom Spheres when Spheres representation is toggled ON
  if (state.cxShowSpheres && state.cxSpheresAlpha > 0.02) {
    const spheres = map.nodes.map((node, i) => {
      const explodeShift = getExplodeOffsetLocal(node.role);
      const loc = vec3Add(worldToPlaneLocal(node.pos_3d, plane), explodeShift);
      const scr = projectLocalPoint(loc, W, H);
      return { idx: i, node, meta: ds.atom_meta[i], sx: scr[0], sy: scr[1], depth: scr[2] };
    }).sort((a, b) => a.depth - b.depth);

    for (const sp of spheres) {
      const r = Math.max(3.5, sp.meta.vdw_radius_a * state.camZoom * 0.35);
      const fill = sp.node.role === "SideA" ? "#38bdf8" : "#fb923c";
      ctx.save();
      ctx.globalAlpha = state.cxSpheresAlpha;
      ctx.beginPath();
      ctx.arc(sp.sx, sp.sy, r, 0, Math.PI * 2);
      const grad = ctx.createRadialGradient(sp.sx - r*0.3, sp.sy - r*0.3, r*0.1, sp.sx, sp.sy, r);
      grad.addColorStop(0, "#ffffff");
      grad.addColorStop(0.4, fill);
      grad.addColorStop(1, fill);
      ctx.fillStyle = grad;
      ctx.fill();
      ctx.strokeStyle = "#0f172a";
      ctx.lineWidth = 1.0;
      ctx.stroke();
      ctx.restore();
    }
  }
}

function renderInterfaceTab(W, H, ds) {
  const map = ds.map;
  const plane = map.plane;

  // Draw Open-Book Hinge axis line when bookTheta > 0.01
  if (state.bookTheta > 0.01 && state.flattenT > 0.1) {
    const h1 = projectLocalPoint([plane.hinge_u, -28, 0], W, H);
    const h2 = projectLocalPoint([plane.hinge_u, 28, 0], W, H);
    ctx.save();
    ctx.strokeStyle = "#64748b";
    ctx.lineWidth = 1.5;
    ctx.setLineDash([6, 5]);
    ctx.beginPath();
    ctx.moveTo(h1[0], h1[1]);
    ctx.lineTo(h2[0], h2[1]);
    ctx.stroke();
    ctx.fillStyle = "#475569";
    ctx.font = "600 11px Inter, sans-serif";
    ctx.fillText(`◀ ${ds.group_b_name.toUpperCase()} (Unfolded)   |   BOOK HINGE   |   ${ds.group_a_name.toUpperCase()} ▶`, h1[0] - 150, Math.min(h1[1], h2[1]) - 8);
    ctx.restore();
  }

  // 1. Draw 3D C-alpha backbone trace (fades as flattenT -> 1.0)
  if (state.showBackbone && state.flattenT < 0.98) {
    const bbAlpha = (1.0 - state.flattenT * 0.85);
    for (const ch of ds.chains) {
      const role = getChainRole(ds, ch.chain_id);
      if (role === "Unassigned") continue;
      const roleAlpha = role === "SideA" ? state.sideAAlpha : state.sideBAlpha;
      if (roleAlpha <= 0.01) continue;
      ctx.save();
      ctx.strokeStyle = role === "SideA" ? "#0284c7" : "#ea580c";
      ctx.globalAlpha = bbAlpha * roleAlpha * 0.45;
      ctx.lineWidth = Math.max(2.0, state.camZoom * 0.16);
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      ctx.beginPath();
      ch.ca_positions.forEach((pos3d, idx) => {
        const loc3d = worldToPlaneLocal(pos3d, plane);
        const pt = vec3Lerp(loc3d, [loc3d[0], loc3d[1], 0], state.flattenT);
        const scr = projectLocalPoint(pt, W, H);
        if (idx === 0) ctx.moveTo(scr[0], scr[1]);
        else ctx.lineTo(scr[0], scr[1]);
      });
      ctx.stroke();
      ctx.restore();
    }
  }

  const projectedNodes = map.nodes.map((node, i) => {
    const uv = state.useRelaxedUv ? node.pos_uv : node.pos_uv_raw;
    const loc = computeInterfaceAtomLocal3D(node.pos_3d, uv, node.role, plane, state.flattenT, state.bookTheta);
    const scr = projectLocalPoint(loc, W, H);
    return { idx: i, node, meta: ds.atom_meta[i], loc, sx: scr[0], sy: scr[1], depth: scr[2] };
  });

  // Identify mating atoms/territories for hover/selection highlighting using node.interactions
  const highlightedAtoms = new Set();
  const highlightedTerritories = new Set();
  const seedAtoms = [];
  if (state.hoveredAtomIdx !== null) {
    seedAtoms.push(state.hoveredAtomIdx);
    highlightedTerritories.add(map.nodes[state.hoveredAtomIdx].territory);
  }
  if (state.selectedTerritoryIdx !== null) {
    highlightedTerritories.add(state.selectedTerritoryIdx);
    for (const aIdx of map.territories[state.selectedTerritoryIdx].patch_atoms) {
      seedAtoms.push(aIdx);
    }
  }
  for (const sIdx of seedAtoms) {
    highlightedAtoms.add(sIdx);
    for (const inter of map.nodes[sIdx].interactions) {
      if (inter.is_cross_interface) {
        highlightedAtoms.add(inter.partner);
        highlightedTerritories.add(map.nodes[inter.partner].territory);
      }
    }
  }

  // 2. Draw 3D-to-2D Residue Territory Contours (SideB first, then SideA)
  if (state.showTerritories) {
    const contourBlend = 0.45 + 0.55 * state.flattenT;
    for (const rolePass of ["SideB", "SideA"]) {
      const roleAlpha = rolePass === "SideA" ? state.sideAAlpha : state.sideBAlpha;
      if (roleAlpha <= 0.02) continue;

      for (const terr of map.territories) {
        if (terr.role !== rolePass || terr.contour_uv.length < 3) continue;
        const isHigh = highlightedTerritories.has(terr.idx);
        ctx.save();
        ctx.globalAlpha = contourBlend * roleAlpha;
        ctx.beginPath();
        terr.contour_uv.forEach((uv, idx) => {
          const pos3d = (terr.contour_3d && terr.contour_3d[idx]) ? terr.contour_3d[idx] : plane.origin;
          const loc = computeInterfaceAtomLocal3D(pos3d, uv, terr.role, plane, state.flattenT, state.bookTheta);
          const scr = projectLocalPoint(loc, W, H);
          if (idx === 0) ctx.moveTo(scr[0], scr[1]);
          else ctx.lineTo(scr[0], scr[1]);
        });
        ctx.closePath();

        if (isHigh) {
          ctx.fillStyle = rolePass === "SideA" ? "rgba(56, 189, 248, 0.22)" : "rgba(251, 146, 60, 0.22)";
        } else {
          ctx.fillStyle = rolePass === "SideA" ? "rgba(15, 23, 42, 0.055)" : "rgba(100, 116, 139, 0.05)";
        }
        ctx.fill();

        if (terr.has_unsatisfied_polar) {
          ctx.strokeStyle = "#d97706";
          ctx.lineWidth = isHigh ? 3.2 : 2.4;
          ctx.setLineDash([6, 4]);
        } else {
          ctx.strokeStyle = rolePass === "SideA"
            ? (isHigh ? "#0284c7" : "#0f172a")
            : (isHigh ? "#ea580c" : "#64748b");
          ctx.lineWidth = isHigh ? 3.0 : (rolePass === "SideA" ? 2.2 : 1.5);
        }
        ctx.stroke();
        ctx.restore();
      }
    }
  }

  // 3. Draw Unified InterfaceEdges (intra-side & cross-interface)
  for (const edge of map.edges) {
    const pa = projectedNodes[edge.a];
    const pb = projectedNodes[edge.b];

    if (!edge.is_cross_interface) {
      if (!state.showIntraEdges) continue;
      const roleAlpha = pa.node.role === "SideA" ? state.sideAAlpha : state.sideBAlpha;
      if (roleAlpha <= 0.02) continue;
      ctx.save();
      ctx.globalAlpha = roleAlpha * 0.55;
      if (edge.kind === "HBond" || edge.kind === "SaltBridge") {
        ctx.strokeStyle = "#0891b2";
        ctx.lineWidth = 2.0;
        ctx.setLineDash([4, 3]);
      } else if (edge.kind === "SameResidue") {
        ctx.strokeStyle = pa.node.role === "SideA" ? "#334155" : "#94a3b8";
        ctx.lineWidth = 1.6;
      } else {
        ctx.strokeStyle = "#cbd5e1";
        ctx.lineWidth = 1.0;
      }
      ctx.beginPath();
      ctx.moveTo(pa.sx, pa.sy);
      ctx.lineTo(pb.sx, pb.sy);
      ctx.stroke();
      ctx.restore();
    } else {
      if (!state.showCrossArcs || state.sideAAlpha <= 0.05 || state.sideBAlpha <= 0.05) continue;
      const isRelevant = highlightedAtoms.has(edge.a) || highlightedAtoms.has(edge.b);
      if (edge.kind === "VanDerWaalsContact" && !isRelevant) continue;

      ctx.save();
      ctx.globalAlpha = isRelevant ? 0.95 : 0.45;
      if (edge.kind === "Clash") {
        ctx.strokeStyle = "#f21414";
        ctx.lineWidth = isRelevant ? 2.6 : 1.8;
      } else if (edge.kind === "SaltBridge") {
        ctx.strokeStyle = "#9d14ff";
        ctx.lineWidth = isRelevant ? 2.4 : 1.6;
        ctx.setLineDash([5, 3]);
      } else if (edge.kind === "HBond") {
        ctx.strokeStyle = "#2563eb";
        ctx.lineWidth = isRelevant ? 2.2 : 1.5;
        ctx.setLineDash([4, 3]);
      } else {
        ctx.strokeStyle = "#64748b";
        ctx.lineWidth = 1.2;
        ctx.setLineDash([2, 3]);
      }

      ctx.beginPath();
      ctx.moveTo(pa.sx, pa.sy);
      if (state.bookTheta > 0.15 && state.flattenT > 0.3) {
        const midX = (pa.sx + pb.sx) * 0.5;
        const span = Math.abs(pa.sx - pb.sx);
        const midY = (pa.sy + pb.sy) * 0.5 - Math.min(90, span * 0.22);
        ctx.quadraticCurveTo(midX, midY, pb.sx, pb.sy);
      } else {
        ctx.lineTo(pb.sx, pb.sy);
      }
      ctx.stroke();
      ctx.restore();
    }
  }

  // 4. Sort Atom Nodes by depth and draw
  const sortedNodes = [...projectedNodes].sort((a, b) => a.depth - b.depth);
  for (const item of sortedNodes) {
    const { idx, node, meta, sx, sy } = item;
    const roleAlpha = node.role === "SideA" ? state.sideAAlpha : state.sideBAlpha;
    if (roleAlpha <= 0.02) continue;

    const r = getAtomRadiusPx(node);
    const fill = getAtomFillColor(node, meta);
    const isHigh = highlightedAtoms.has(idx);

    ctx.save();
    ctx.globalAlpha = roleAlpha;

    if (node.is_unsatisfied_buried_polar) {
      ctx.beginPath();
      ctx.arc(sx, sy, r + 3.5, 0, Math.PI * 2);
      ctx.strokeStyle = "#d97706";
      ctx.lineWidth = 2.0;
      ctx.stroke();
    }

    ctx.beginPath();
    ctx.arc(sx, sy, r, 0, Math.PI * 2);
    if (state.flattenT < 0.95) {
      const grad = ctx.createRadialGradient(sx - r*0.3, sy - r*0.3, r*0.1, sx, sy, r);
      grad.addColorStop(0, "#ffffff");
      grad.addColorStop(0.35, fill);
      grad.addColorStop(1, fill);
      ctx.fillStyle = grad;
    } else {
      ctx.fillStyle = fill;
    }
    ctx.fill();

    ctx.lineWidth = isHigh ? 2.6 : (node.role === "SideA" ? 1.3 : 1.0);
    ctx.strokeStyle = isHigh ? "#facc15" : (node.role === "SideA" ? "#0f172a" : "#475569");
    ctx.stroke();

    if (state.camZoom >= 24 || isHigh) {
      ctx.fillStyle = "#0f172a";
      ctx.font = "600 9px Inter, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(meta.atom_name, sx, sy - r - 3);
    }
    ctx.restore();
  }

  // 5. Draw Two-Line Residue Labels (interpolating from 3D relief pole to 2D pole)
  if (state.showTerritories && state.flattenT > 0.2) {
    const lblAlpha = Math.min(1.0, (state.flattenT - 0.2) / 0.45);
    for (const terr of map.territories) {
      const roleAlpha = terr.role === "SideA" ? state.sideAAlpha : state.sideBAlpha;
      if (roleAlpha <= 0.15) continue;
      if (terr.role === "SideB" && state.bookTheta < 0.25 && state.sideAAlpha > 0.4 && roleAlpha < 0.6) {
        continue;
      }

      const lbl3d = terr.label_center_3d || plane.origin;
      const loc = computeInterfaceAtomLocal3D(lbl3d, terr.label_center_uv, terr.role, plane, state.flattenT, state.bookTheta);
      const scr = projectLocalPoint(loc, W, H);

      ctx.save();
      ctx.globalAlpha = lblAlpha * roleAlpha;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";

      ctx.font = "700 11px Inter, sans-serif";
      const w1 = ctx.measureText(terr.label_top).width;
      ctx.font = "500 9.5px Inter, sans-serif";
      const w2 = ctx.measureText(terr.label_bottom).width;
      const boxW = Math.max(w1, w2) + 10;
      const boxH = 26;

      ctx.fillStyle = terr.role === "SideA" ? "rgba(255, 255, 255, 0.88)" : "rgba(241, 245, 249, 0.84)";
      ctx.strokeStyle = terr.has_unsatisfied_polar ? "#d97706" : "#cbd5e1";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.roundRect(scr[0] - boxW*0.5, scr[1] - boxH*0.5, boxW, boxH, 4);
      ctx.fill();
      ctx.stroke();

      ctx.fillStyle = "#0f172a";
      ctx.font = "700 11px Inter, sans-serif";
      ctx.fillText(terr.label_top, scr[0], scr[1] - 5);

      ctx.fillStyle = "#475569";
      ctx.font = "500 9.5px Inter, sans-serif";
      ctx.fillText(terr.label_bottom, scr[0], scr[1] + 7);
      ctx.restore();
    }
  }
}

function render() {
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  if (canvas.width !== Math.round(rect.width * dpr) || canvas.height !== Math.round(rect.height * dpr)) {
    canvas.width = Math.round(rect.width * dpr);
    canvas.height = Math.round(rect.height * dpr);
  }
  ctx.save();
  ctx.scale(dpr, dpr);
  const W = rect.width, H = rect.height;

  ctx.fillStyle = "#f8fafc";
  ctx.fillRect(0, 0, W, H);

  const ds = currentDataset();

  // Subtle coordinate grid on tangent plane
  ctx.strokeStyle = "#e2e8f0";
  ctx.lineWidth = 1;
  for (let g = -40; g <= 40; g += 5) {
    const p1 = projectLocalPoint([g, -40, 0], W, H);
    const p2 = projectLocalPoint([g, 40, 0], W, H);
    ctx.beginPath(); ctx.moveTo(p1[0], p1[1]); ctx.lineTo(p2[0], p2[1]); ctx.stroke();
    const q1 = projectLocalPoint([-40, g, 0], W, H);
    const q2 = projectLocalPoint([40, g, 0], W, H);
    ctx.beginPath(); ctx.moveTo(q1[0], q1[1]); ctx.lineTo(q2[0], q2[1]); ctx.stroke();
  }

  if (state.workspaceTab === "Complex") {
    renderComplexTab(W, H, ds);
  } else {
    renderInterfaceTab(W, H, ds);
  }

  drawOrientationGizmo(W, H);
  ctx.restore();
}

let gizmoHitTargets = [];

function rotateAxisForGizmo(vec) {
  const cy = Math.cos(state.camYaw), sy = Math.sin(state.camYaw);
  const cp = Math.cos(state.camPitch), sp = Math.sin(state.camPitch);
  const x0 = vec[0], y0 = vec[1], z0 = vec[2];
  const x1 = x0 * cy + z0 * sy;
  const z1 = -x0 * sy + z0 * cy;
  const y2 = y0 * cp - z1 * sp;
  const z2 = y0 * sp + z1 * cp;
  return [x1, y2, z2];
}

function drawOrientationGizmo(W, H) {
  const cx = W - 56;
  const cy = 56;
  const armLen = 34;

  ctx.save();
  ctx.beginPath();
  ctx.arc(cx, cy, 44, 0, Math.PI * 2);
  ctx.fillStyle = "rgba(15, 23, 42, 0.78)";
  ctx.fill();
  ctx.strokeStyle = "rgba(148, 163, 184, 0.35)";
  ctx.lineWidth = 1;
  ctx.stroke();

  const axes = [
    { label: "u",  dir: [ 1,  0,  0], color: "#ef4444", pos: true,  yaw: -Math.PI/2, pitch: 0 },
    { label: "-u", dir: [-1,  0,  0], color: "#991b1b", pos: false, yaw:  Math.PI/2, pitch: 0 },
    { label: "v",  dir: [ 0,  1,  0], color: "#22c55e", pos: true,  yaw: 0,          pitch:  Math.PI/2 },
    { label: "-v", dir: [ 0, -1,  0], color: "#166534", pos: false, yaw: 0,          pitch: -Math.PI/2 },
    { label: "n",  dir: [ 0,  0,  1], color: "#3b82f6", pos: true,  yaw: 0,          pitch: 0 },
    { label: "-n", dir: [ 0,  0, -1], color: "#1e40af", pos: false, yaw: Math.PI,    pitch: 0 },
  ];

  const projected = axes.map(a => {
    const r = rotateAxisForGizmo(a.dir);
    return { ...a, sx: cx + r[0] * armLen, sy: cy - r[1] * armLen, depth: r[2] };
  }).sort((a, b) => a.depth - b.depth);

  gizmoHitTargets = [];

  for (const a of projected) {
    if (a.pos) {
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.lineTo(a.sx, a.sy);
      ctx.strokeStyle = a.color;
      ctx.lineWidth = 2.2;
      ctx.stroke();
    }
    const rad = a.pos ? 8.5 : 6.0;
    ctx.beginPath();
    ctx.arc(a.sx, a.sy, rad, 0, Math.PI * 2);
    ctx.fillStyle = a.color;
    ctx.globalAlpha = a.depth >= -0.1 ? 1.0 : 0.6;
    ctx.fill();
    ctx.strokeStyle = "#0f172a";
    ctx.lineWidth = 1.2;
    ctx.stroke();

    if (a.pos) {
      ctx.fillStyle = "#ffffff";
      ctx.font = "700 9.5px Inter, sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(a.label, a.sx, a.sy);
    }
    gizmoHitTargets.push({ sx: a.sx, sy: a.sy, rad: rad + 3, yaw: a.yaw, pitch: a.pitch, label: a.label });
  }
  ctx.restore();
}

function buildChainMatrixUI() {
  const ds = currentDataset();
  document.getElementById("group-a-name-input").value = ds.group_a_name;
  document.getElementById("group-b-name-input").value = ds.group_b_name;
  document.getElementById("side-a-opacity-lbl").textContent = `${ds.group_a_name} Opacity`;
  document.getElementById("side-b-opacity-lbl").textContent = `${ds.group_b_name} Opacity`;

  const container = document.getElementById("chain-matrix");
  container.innerHTML = ds.chains.map(ch => {
    const role = getChainRole(ds, ch.chain_id);
    return `
      <div class="chain-row">
        <div>
          <strong>Chain ${ch.chain_id}</strong>
          <span style="color:var(--muted); font-size:11px;">(${ch.residue_count} aa)</span>
        </div>
        <div class="chain-pills">
          <button class="pill-btn side-a ${role === 'SideA' ? 'active' : ''}" data-chain="${ch.chain_id}" data-assign="SideA">${ds.group_a_name}</button>
          <button class="pill-btn side-b ${role === 'SideB' ? 'active' : ''}" data-chain="${ch.chain_id}" data-assign="SideB">${ds.group_b_name}</button>
          <button class="pill-btn none ${role === 'Unassigned' ? 'active' : ''}" data-chain="${ch.chain_id}" data-assign="Unassigned">Off</button>
        </div>
      </div>
    `;
  }).join("");

  container.querySelectorAll(".pill-btn").forEach(btn => {
    btn.addEventListener("click", async () => {
      const cid = btn.getAttribute("data-chain");
      const assign = btn.getAttribute("data-assign");
      ds.group_a_chains = ds.group_a_chains.filter(c => c !== cid);
      ds.group_b_chains = ds.group_b_chains.filter(c => c !== cid);
      if (assign === "SideA") ds.group_a_chains.push(cid);
      if (assign === "SideB") ds.group_b_chains.push(cid);

      // If running with live Rust backend (`cargo run -p smorgasprot-app -- --serve`), request live Rust recomputation
      if (window.location.protocol.startsWith("http")) {
        try {
          const resp = await fetch("/api/recompute", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              dataset_id: ds.id,
              group_a_name: ds.group_a_name,
              group_b_name: ds.group_b_name,
              group_a_chains: ds.group_a_chains,
              group_b_chains: ds.group_b_chains,
            })
          });
          if (resp.ok) {
            const updated = await resp.json();
            ds.map = updated.map;
            ds.atom_meta = updated.atom_meta;
            ds.stats = updated.stats;
          }
        } catch (_e) {}
      }

      buildChainMatrixUI();
      updateStatsPanel();
      updateInspector();
      render();
    });
  });
}

function updateInspector() {
  const ds = currentDataset();
  const map = ds.map;
  const container = document.getElementById("inspector-content");

  let terrIdx = state.selectedTerritoryIdx;
  let atomIdx = state.hoveredAtomIdx;
  if (atomIdx !== null) {
    terrIdx = map.nodes[atomIdx].territory;
  }
  if (terrIdx === null) {
    container.innerHTML = `<div style="color:var(--muted);">Hover or click any atom node or residue territory to inspect its 3D/2D metrics and interactions.</div>`;
    return;
  }

  const terr = map.territories[terrIdx];
  const sideName = terr.role === "SideA" ? ds.group_a_name : ds.group_b_name;
  const terrAtomSet = new Set(terr.patch_atoms);
  const crossEdges = map.edges.filter(e =>
    e.is_cross_interface && (terrAtomSet.has(e.a) || terrAtomSet.has(e.b))
  );

  let atomDetailHtml = "";
  if (atomIdx !== null) {
    const node = map.nodes[atomIdx];
    const meta = ds.atom_meta[atomIdx];
    atomDetailHtml = `
      <div class="stat-box" style="border-color:var(--accent);">
        <div style="font-weight:700; color:#fff;">Atom ${meta.atom_name} (${meta.element}) — ${terr.label_top} (Chain ${meta.chain_id})</div>
        <div style="margin-top:4px; color:#cbd5e1;">
          ΔSASA: <strong>${node.buried_sasa_a2.toFixed(1)} Å²</strong> (Unbound ${node.free_sasa_a2.toFixed(1)} → Bound ${node.complex_sasa_a2.toFixed(1)} Å²)<br>
          2D Relaxed (u, v): <code>(${node.pos_uv[0].toFixed(2)}, ${node.pos_uv[1].toFixed(2)})</code>
        </div>
      </div>
    `;
  }

  const contactRows = crossEdges.map(c => {
    const aMeta = ds.atom_meta[c.a];
    const bMeta = ds.atom_meta[c.b];
    const aTerr = map.territories[map.nodes[c.a].territory];
    const bTerr = map.territories[map.nodes[c.b].territory];
    const badge = c.kind === "Clash"
      ? `Clash (-${c.overlap_a.toFixed(2)} Å)`
      : `${c.kind} (${c.dist_3d_a.toFixed(2)} Å)`;
    return `
      <div class="contact-item ${c.kind}">
        <span><strong>${aTerr.label_top} ${aMeta.atom_name}</strong> ↔ <strong>${bTerr.label_top} ${bMeta.atom_name}</strong></span>
        <span>${badge}</span>
      </div>
    `;
  }).join("");

  container.innerHTML = `
    <div style="display:flex; justify-content:space-between; align-items:center;">
      <div>
        <div style="font-size:15px; font-weight:700; color:#fff;">${terr.label_top} (${sideName})</div>
        <div style="font-size:11px; color:var(--muted);">Chain ${terr.chain_id} • ${terr.label_bottom} • ${terr.patch_atoms.length} interfacial atoms</div>
      </div>
      <div style="text-align:right;">
        <div style="font-size:14px; font-weight:700; color:var(--accent);">${terr.buried_sasa_a2.toFixed(1)} Å²</div>
        <div style="font-size:10px; color:var(--muted);">Buried ΔSASA</div>
      </div>
    </div>
    ${terr.has_unsatisfied_polar ? `<div style="padding:6px 8px; border-radius:6px; background:rgba(217,119,6,0.18); border:1px solid #d97706; color:#fcd34d; font-size:11px;">⚠️ Contains buried polar atom with 0 satisfied H-bonds or salt bridges.</div>` : ""}
    ${atomDetailHtml}
    <div style="font-size:11px; font-weight:700; color:var(--muted); text-transform:uppercase; margin-top:4px;">
      Cross-Interface Contacts (${crossEdges.length})
    </div>
    <div class="contact-list">
      ${contactRows || `<div style="color:var(--muted); font-size:11px;">No direct ≤ 4.5 Å cross-contacts (peripheral ΔSASA rim residue).</div>`}
    </div>
  `;
}

function updateStatsPanel() {
  const ds = currentDataset();
  const s = ds.stats;
  document.getElementById("dataset-stats").innerHTML = `
    <div class="stat-box">
      <div class="stat-val" style="color:#38bdf8;">${s.side_a_atoms} / ${s.side_a_territories}</div>
      <div class="stat-lbl">${ds.group_a_name} Atoms / Residues (${ds.group_a_chains.join(",") || "None"})</div>
    </div>
    <div class="stat-box">
      <div class="stat-val" style="color:#fb923c;">${s.side_b_atoms} / ${s.side_b_territories}</div>
      <div class="stat-lbl">${ds.group_b_name} Atoms / Residues (${ds.group_b_chains.join(",") || "None"})</div>
    </div>
  `;

  const crossEdges = ds.map.edges.filter(e => e.is_cross_interface);
  const clashes = crossEdges.filter(c => c.kind === "Clash").length;
  const hbonds = crossEdges.filter(c => c.kind === "HBond").length;
  const salts = crossEdges.filter(c => c.kind === "SaltBridge").length;

  document.getElementById("relax-diagnostics").innerHTML = `
    <div class="stat-box">
      <div class="stat-val">${s.edge_rmse_before_a.toFixed(2)} → ${s.edge_rmse_after_a.toFixed(2)} Å</div>
      <div class="stat-lbl">1st-Shell Edge RMSE (Raw → Relaxed)</div>
    </div>
    <div class="stat-box">
      <div class="stat-val">${s.overlaps_before} → ${s.overlaps_after}</div>
      <div class="stat-lbl">2D Hard-Core Overlaps (Raw → Relaxed)</div>
    </div>
    <div class="stat-box">
      <div class="stat-val" style="color:#38bdf8;">${hbonds} HB / ${salts} SB</div>
      <div class="stat-lbl">H-Bonds & Salt Bridges</div>
    </div>
    <div class="stat-box">
      <div class="stat-val" style="color:${clashes > 0 ? '#f87171' : '#4ade80'};">${clashes}</div>
      <div class="stat-lbl">Steric VdW Clashes</div>
    </div>
  `;
}

function updateLegend() {
  const el = document.getElementById("lens-legend");
  const ds = currentDataset();
  const items = {
    Interactions: [
      ["#f21414", "VdW Clash"],
      ["#2563eb", "H-Bond"],
      ["#9d14ff", "Salt Bridge"],
      ["#0ea5e9", ds.group_a_name],
      ["#fb923c", ds.group_b_name],
      ["#d97706", "Unsatisfied Polar (Ring)"]
    ],
    Chemistry: [
      ["#94a3b8", "Aliphatic"],
      ["#f59e0b", "Aromatic"],
      ["#06b6d4", "Polar"],
      ["#3b82f6", "Positive"],
      ["#ef4444", "Negative"],
      ["#a855f7", "Gly / Pro"]
    ],
    ModernCpk: [
      ["#475569", "Carbon"],
      ["#2563eb", "Nitrogen"],
      ["#dc2626", "Oxygen"],
      ["#d97706", "Sulfur"]
    ],
    Bfactor: [
      ["#0053d6", "High (>80)"],
      ["#65cbf3", "Med (60-80)"],
      ["#ffdb13", "Low (40-60)"],
      ["#ff7d45", "Very Low (<40)"]
    ]
  }[state.lens] || [];

  el.innerHTML = items.map(([c, lbl]) => `<span><i class="swatch" style="background:${c}"></i>${lbl}</span>`).join("");
}

function stepAnimation() {
  let needsNext = false;

  const dt = state.targetFlattenT - state.flattenT;
  if (Math.abs(dt) > 0.002) {
    state.flattenT += dt * 0.18;
    needsNext = true;
  } else {
    state.flattenT = state.targetFlattenT;
  }

  const db = state.targetBookTheta - state.bookTheta;
  if (Math.abs(db) > 0.002) {
    state.bookTheta += db * 0.18;
    needsNext = true;
  } else {
    state.bookTheta = state.targetBookTheta;
  }

  const de = state.targetExplodeT - state.explodeT;
  if (Math.abs(de) > 0.002) {
    state.explodeT += de * 0.18;
    needsNext = true;
  } else {
    state.explodeT = state.targetExplodeT;
  }

  if (state.workspaceTab === "Interface" && state.targetFlattenT === 1.0 && needsNext) {
    state.camYaw *= 0.82;
    state.camPitch *= 0.82;
  }

  syncSliderLabels();
  render();
  if (needsNext) {
    requestAnimationFrame(stepAnimation);
  } else {
    state.animating = false;
  }
}

function triggerTween() {
  if (!state.animating) {
    state.animating = true;
    requestAnimationFrame(stepAnimation);
  }
}

function syncSliderLabels() {
  document.getElementById("flatten-slider").value = state.flattenT;
  const modeStr = state.flattenT < 0.05 ? "3D" : (state.flattenT > 0.95 ? "2D Map" : "Bas-Relief");
  document.getElementById("flatten-val").textContent = `${state.flattenT.toFixed(2)} (${modeStr})`;

  document.getElementById("book-slider").value = state.bookTheta;
  const bookStr = state.bookTheta < 0.05 ? "Overlay" : (state.bookTheta > 0.95 ? "Open-Book 180°" : `${Math.round(state.bookTheta*180)}°`);
  document.getElementById("book-val").textContent = `${state.bookTheta.toFixed(2)} (${bookStr})`;

  document.getElementById("explode-slider").value = state.explodeT;
  document.getElementById("explode-val").textContent = `${state.explodeT.toFixed(2)} (${(state.explodeT * 24).toFixed(1)} Å)`;
}

function switchWorkspaceTab(tabName) {
  state.workspaceTab = tabName;
  document.querySelectorAll("[data-workspace-tab]").forEach(btn => {
    btn.classList.toggle("active", btn.getAttribute("data-workspace-tab") === tabName);
  });
  document.getElementById("controls-complex").style.display = tabName === "Complex" ? "flex" : "none";
  document.getElementById("controls-interface").style.display = tabName === "Interface" ? "flex" : "none";

  if (tabName === "Interface") {
    state.camYaw = 0.0;
    state.camPitch = 0.0;
    state.camZoom = 18.0;
  } else {
    state.camYaw = 0.45;
    state.camPitch = 0.30;
    state.camZoom = 14.5;
    state.camPanX = 0.0;
    state.camPanY = 0.0;
  }
  render();
}

document.querySelectorAll("[data-workspace-tab]").forEach(btn => {
  btn.addEventListener("click", () => switchWorkspaceTab(btn.getAttribute("data-workspace-tab")));
});

document.getElementById("go-to-interface-btn").addEventListener("click", () => {
  switchWorkspaceTab("Interface");
});

const dsSelect = document.getElementById("dataset-select");
DATASETS.forEach((d, idx) => {
  const opt = document.createElement("option");
  opt.value = idx;
  opt.textContent = d.title;
  dsSelect.appendChild(opt);
});
dsSelect.addEventListener("change", (e) => {
  state.datasetIdx = parseInt(e.target.value, 10);
  state.hoveredAtomIdx = null;
  state.selectedTerritoryIdx = null;
  buildChainMatrixUI();
  updateLegend();
  updateStatsPanel();
  updateInspector();
  render();
});

document.getElementById("group-a-name-input").addEventListener("input", (e) => {
  currentDataset().group_a_name = e.target.value || "Group A";
  buildChainMatrixUI();
  updateLegend();
  updateStatsPanel();
  updateInspector();
  render();
});

document.getElementById("group-b-name-input").addEventListener("input", (e) => {
  currentDataset().group_b_name = e.target.value || "Group B";
  buildChainMatrixUI();
  updateLegend();
  updateStatsPanel();
  updateInspector();
  render();
});

// Complex Tab Representation controls
["cx-show-surface", "cx-show-cartoon", "cx-show-spheres"].forEach(id => {
  document.getElementById(id).addEventListener("change", () => {
    state.cxShowSurface = document.getElementById("cx-show-surface").checked;
    state.cxShowCartoon = document.getElementById("cx-show-cartoon").checked;
    state.cxShowSpheres = document.getElementById("cx-show-spheres").checked;
    render();
  });
});

document.getElementById("cx-surface-alpha").addEventListener("input", (e) => {
  state.cxSurfaceAlpha = parseFloat(e.target.value);
  document.getElementById("cx-surface-alpha-val").textContent = state.cxSurfaceAlpha.toFixed(2);
  render();
});
document.getElementById("cx-cartoon-alpha").addEventListener("input", (e) => {
  state.cxCartoonAlpha = parseFloat(e.target.value);
  document.getElementById("cx-cartoon-alpha-val").textContent = state.cxCartoonAlpha.toFixed(2);
  render();
});
document.getElementById("cx-spheres-alpha").addEventListener("input", (e) => {
  state.cxSpheresAlpha = parseFloat(e.target.value);
  document.getElementById("cx-spheres-alpha-val").textContent = state.cxSpheresAlpha.toFixed(2);
  render();
});

document.getElementById("explode-slider").addEventListener("input", (e) => {
  state.explodeT = parseFloat(e.target.value);
  state.targetExplodeT = state.explodeT;
  syncSliderLabels();
  render();
});

document.querySelectorAll("[data-tween-explode]").forEach(btn => {
  btn.addEventListener("click", () => {
    document.querySelectorAll("[data-tween-explode]").forEach(b => b.classList.remove("active"));
    btn.classList.add("active");
    state.targetExplodeT = parseFloat(btn.getAttribute("data-tween-explode"));
    triggerTween();
  });
});

// Interface Tab controls
document.getElementById("flatten-slider").addEventListener("input", (e) => {
  state.flattenT = parseFloat(e.target.value);
  state.targetFlattenT = state.flattenT;
  syncSliderLabels();
  render();
});

document.querySelectorAll("[data-tween-t]").forEach(btn => {
  btn.addEventListener("click", () => {
    document.querySelectorAll("[data-tween-t]").forEach(b => b.classList.remove("active"));
    btn.classList.add("active");
    state.targetFlattenT = parseFloat(btn.getAttribute("data-tween-t"));
    triggerTween();
  });
});

document.getElementById("book-slider").addEventListener("input", (e) => {
  state.bookTheta = parseFloat(e.target.value);
  state.targetBookTheta = state.bookTheta;
  if (state.bookTheta > 0.5 && state.sideBAlpha < 0.85) {
    state.sideBAlpha = 1.0;
    document.getElementById("side-b-alpha").value = 1.0;
    document.getElementById("side-b-alpha-val").textContent = "1.00";
  }
  syncSliderLabels();
  render();
});

document.querySelectorAll("[data-tween-book]").forEach(btn => {
  btn.addEventListener("click", () => {
    document.querySelectorAll("[data-tween-book]").forEach(b => b.classList.remove("active"));
    btn.classList.add("active");
    state.targetBookTheta = parseFloat(btn.getAttribute("data-tween-book"));
    if (state.targetBookTheta > 0.5) {
      state.sideBAlpha = 1.0;
      document.getElementById("side-b-alpha").value = 1.0;
      document.getElementById("side-b-alpha-val").textContent = "1.00";
      state.camPanX = 110;
    } else {
      state.sideBAlpha = 0.35;
      document.getElementById("side-b-alpha").value = 0.35;
      document.getElementById("side-b-alpha-val").textContent = "0.35";
      state.camPanX = 0;
    }
    triggerTween();
  });
});

document.getElementById("use-relaxed-uv").addEventListener("change", (e) => {
  state.useRelaxedUv = e.target.checked;
  render();
});

document.getElementById("lens-select").addEventListener("change", (e) => {
  state.lens = e.target.value;
  updateLegend();
  render();
});

document.getElementById("size-select").addEventListener("change", (e) => {
  state.sizeMode = e.target.value;
  render();
});

document.getElementById("side-a-alpha").addEventListener("input", (e) => {
  state.sideAAlpha = parseFloat(e.target.value);
  document.getElementById("side-a-alpha-val").textContent = state.sideAAlpha.toFixed(2);
  render();
});

document.getElementById("side-b-alpha").addEventListener("input", (e) => {
  state.sideBAlpha = parseFloat(e.target.value);
  document.getElementById("side-b-alpha-val").textContent = state.sideBAlpha.toFixed(2);
  render();
});

["show-territories", "show-intra-edges", "show-cross-arcs", "show-backbone"].forEach(id => {
  document.getElementById(id).addEventListener("change", () => {
    state.showTerritories = document.getElementById("show-territories").checked;
    state.showIntraEdges = document.getElementById("show-intra-edges").checked;
    state.showCrossArcs = document.getElementById("show-cross-arcs").checked;
    state.showBackbone = document.getElementById("show-backbone").checked;
    render();
  });
});

document.getElementById("reset-camera-btn").addEventListener("click", () => {
  state.camYaw = 0;
  state.camPitch = 0;
  state.camPanX = (state.workspaceTab === "Interface" && state.bookTheta > 0.5) ? 110 : 0;
  state.camPanY = 0;
  state.camZoom = state.workspaceTab === "Interface" ? 18.0 : 14.5;
  render();
});

// Collapsible & drag-resizable sidebars
const sidebarLeft = document.getElementById("sidebar-left");
const sidebarRight = document.getElementById("sidebar-right");
const toggleLeftBtn = document.getElementById("toggle-left-btn");
const toggleRightBtn = document.getElementById("toggle-right-btn");
const resizerLeft = document.getElementById("resizer-left");
const resizerRight = document.getElementById("resizer-right");

toggleLeftBtn.addEventListener("click", () => {
  sidebarLeft.classList.toggle("collapsed");
  toggleLeftBtn.textContent = sidebarLeft.classList.contains("collapsed") ? "▶" : "◀";
  render();
});

toggleRightBtn.addEventListener("click", () => {
  sidebarRight.classList.toggle("collapsed");
  toggleRightBtn.textContent = sidebarRight.classList.contains("collapsed") ? "◀" : "▶";
  render();
});

let activeResizer = null;
resizerLeft.addEventListener("mousedown", (e) => {
  e.preventDefault();
  activeResizer = "left";
  resizerLeft.classList.add("dragging");
});
resizerRight.addEventListener("mousedown", (e) => {
  e.preventDefault();
  activeResizer = "right";
  resizerRight.classList.add("dragging");
});

// Custom right-click context menu
const ctxMenu = document.getElementById("context-menu");
function hideContextMenu() { ctxMenu.style.display = "none"; }

function showContextMenu(clientX, clientY, atomIdx, terrIdx) {
  const ds = currentDataset();
  const map = ds.map;
  const terr = terrIdx !== null ? map.territories[terrIdx] : null;
  const meta = atomIdx !== null ? ds.atom_meta[atomIdx] : null;

  let headerText = `${ state.workspaceTab } Workspace Actions`;
  if (terr && meta) {
    const sideName = terr.role === "SideA" ? ds.group_a_name : ds.group_b_name;
    headerText = `${terr.label_top} (${sideName}) • Atom ${meta.atom_name}`;
  } else if (terr) {
    const sideName = terr.role === "SideA" ? ds.group_a_name : ds.group_b_name;
    headerText = `${terr.label_top} (${sideName})`;
  }

  let itemsHtml = `<div class="ctx-header">${headerText}</div>`;
  if (terr && state.workspaceTab === "Interface") {
    itemsHtml += `
      <div class="ctx-item" data-ctx-action="center-territory">🎯 Center Camera on ${terr.label_top}</div>
      <div class="ctx-item" data-ctx-action="select-territory">🔍 Pin ${terr.label_top} in Inspector</div>
      <div class="ctx-sep"></div>
    `;
  }
  itemsHtml += `
    <div class="ctx-item" data-ctx-action="switch-tab">🔄 Switch to ${state.workspaceTab === 'Complex' ? 'Interface' : 'Complex'} Tab</div>
    <div class="ctx-sep"></div>
    <div class="ctx-item" data-ctx-action="solo-a">🔵 Solo ${ds.group_a_name}</div>
    <div class="ctx-item" data-ctx-action="solo-b">🟠 Solo ${ds.group_b_name}</div>
    <div class="ctx-item" data-ctx-action="show-both">👁️ Show Both Groups</div>
    <div class="ctx-sep"></div>
    <div class="ctx-item" data-ctx-action="reset-camera">🧭 Align Camera to Tangent Plane Π</div>
  `;

  ctxMenu.innerHTML = itemsHtml;
  ctxMenu.style.display = "block";
  const menuW = 235, menuH = 225;
  ctxMenu.style.left = `${Math.min(window.innerWidth - menuW, clientX)}px`;
  ctxMenu.style.top = `${Math.min(window.innerHeight - menuH, clientY)}px`;

  ctxMenu.querySelectorAll("[data-ctx-action]").forEach(item => {
    item.addEventListener("click", (ev) => {
      ev.stopPropagation();
      const act = item.getAttribute("data-ctx-action");
      if (act === "center-territory" && terr) {
        const rect = canvas.getBoundingClientRect();
        const loc = computeInterfaceAtomLocal3D(map.plane.origin, terr.label_center_uv, terr.role, map.plane, state.flattenT, state.bookTheta);
        const scr = projectLocalPoint(loc, rect.width, rect.height);
        state.camPanX += (rect.width * 0.5 - scr[0]);
        state.camPanY += (rect.height * 0.5 - scr[1]);
        state.selectedTerritoryIdx = terr.idx;
      } else if (act === "select-territory" && terr) {
        state.selectedTerritoryIdx = terr.idx;
        sidebarRight.classList.remove("collapsed");
        toggleRightBtn.textContent = "▶";
      } else if (act === "switch-tab") {
        switchWorkspaceTab(state.workspaceTab === "Complex" ? "Interface" : "Complex");
      } else if (act === "solo-a") {
        state.sideAAlpha = 1.0; state.sideBAlpha = 0.05;
        document.getElementById("side-a-alpha").value = 1.0;
        document.getElementById("side-a-alpha-val").textContent = "1.00";
        document.getElementById("side-b-alpha").value = 0.05;
        document.getElementById("side-b-alpha-val").textContent = "0.05";
      } else if (act === "solo-b") {
        state.sideAAlpha = 0.05; state.sideBAlpha = 1.0;
        document.getElementById("side-a-alpha").value = 0.05;
        document.getElementById("side-a-alpha-val").textContent = "0.05";
        document.getElementById("side-b-alpha").value = 1.0;
        document.getElementById("side-b-alpha-val").textContent = "1.00";
      } else if (act === "show-both") {
        state.sideAAlpha = 1.0;
        state.sideBAlpha = state.bookTheta > 0.5 ? 1.0 : 0.35;
        document.getElementById("side-a-alpha").value = state.sideAAlpha;
        document.getElementById("side-a-alpha-val").textContent = state.sideAAlpha.toFixed(2);
        document.getElementById("side-b-alpha").value = state.sideBAlpha;
        document.getElementById("side-b-alpha-val").textContent = state.sideBAlpha.toFixed(2);
      } else if (act === "reset-camera") {
        state.camYaw = 0; state.camPitch = 0;
        state.camPanX = (state.workspaceTab === "Interface" && state.bookTheta > 0.5) ? 110 : 0;
        state.camPanY = 0;
      }
      hideContextMenu();
      updateInspector();
      render();
    });
  });
}

canvas.addEventListener("contextmenu", (e) => e.preventDefault());
window.addEventListener("click", () => hideContextMenu());

let isDragging = false;
let dragButton = 0;
let dragStartX = 0, dragStartY = 0;
let dragMoved = false;

canvas.addEventListener("mousedown", (e) => {
  hideContextMenu();
  isDragging = true;
  dragButton = e.button;
  dragMoved = false;
  dragStartX = e.clientX;
  dragStartY = e.clientY;
});

window.addEventListener("mousemove", (e) => {
  if (activeResizer === "left") {
    sidebarLeft.classList.remove("collapsed");
    sidebarLeft.style.width = `${Math.max(230, Math.min(560, e.clientX))}px`;
    render();
    return;
  }
  if (activeResizer === "right") {
    sidebarRight.classList.remove("collapsed");
    sidebarRight.style.width = `${Math.max(230, Math.min(560, window.innerWidth - e.clientX))}px`;
    render();
    return;
  }

  const rect = canvas.getBoundingClientRect();
  const mx = e.clientX - rect.left;
  const my = e.clientY - rect.top;

  if (isDragging) {
    const dx = e.clientX - dragStartX;
    const dy = e.clientY - dragStartY;
    if (Math.abs(dx) + Math.abs(dy) > 3) dragMoved = true;
    dragStartX = e.clientX;
    dragStartY = e.clientY;

    const isPan = e.shiftKey || dragButton === 2 || (state.workspaceTab === "Interface" && state.flattenT > 0.92);
    if (isPan) {
      state.camPanX += dx;
      state.camPanY += dy;
    } else {
      state.camYaw += dx * 0.008;
      state.camPitch = Math.max(-1.45, Math.min(1.45, state.camPitch + dy * 0.008));
    }
    render();
    return;
  }

  // Hover picking in Interface Tab
  if (state.workspaceTab === "Interface" && mx >= 0 && mx <= rect.width && my >= 0 && my <= rect.height) {
    const ds = currentDataset();
    const map = ds.map;
    let bestIdx = null;
    let bestD2 = 196;
    map.nodes.forEach((node, idx) => {
      const roleAlpha = node.role === "SideA" ? state.sideAAlpha : state.sideBAlpha;
      if (roleAlpha <= 0.05) return;
      const uv = state.useRelaxedUv ? node.pos_uv : node.pos_uv_raw;
      const loc = computeInterfaceAtomLocal3D(node.pos_3d, uv, node.role, map.plane, state.flattenT, state.bookTheta);
      const scr = projectLocalPoint(loc, rect.width, rect.height);
      const d2 = (scr[0] - mx)*(scr[0] - mx) + (scr[1] - my)*(scr[1] - my);
      if (d2 < bestD2) {
        bestD2 = d2;
        bestIdx = idx;
      }
    });
    if (bestIdx !== state.hoveredAtomIdx) {
      state.hoveredAtomIdx = bestIdx;
      updateInspector();
      render();
    }
  }
});

window.addEventListener("mouseup", (e) => {
  if (activeResizer) {
    activeResizer = null;
    resizerLeft.classList.remove("dragging");
    resizerRight.classList.remove("dragging");
    return;
  }

  if (isDragging && !dragMoved) {
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    if (dragButton === 0) {
      for (const hit of gizmoHitTargets) {
        const d2 = (mx - hit.sx)*(mx - hit.sx) + (my - hit.sy)*(my - hit.sy);
        if (d2 <= hit.rad * hit.rad) {
          state.camYaw = hit.yaw;
          state.camPitch = hit.pitch;
          isDragging = false;
          render();
          return;
        }
      }
    }

    if (dragButton === 2) {
      const atomIdx = state.hoveredAtomIdx;
      const terrIdx = atomIdx !== null
        ? currentDataset().map.nodes[atomIdx].territory
        : state.selectedTerritoryIdx;
      showContextMenu(e.clientX, e.clientY, atomIdx, terrIdx);
    } else if (dragButton === 0 && state.workspaceTab === "Interface") {
      if (state.hoveredAtomIdx !== null) {
        state.selectedTerritoryIdx = currentDataset().map.nodes[state.hoveredAtomIdx].territory;
      } else {
        state.selectedTerritoryIdx = null;
      }
      updateInspector();
      render();
    }
  }
  isDragging = false;
});

canvas.addEventListener("wheel", (e) => {
  e.preventDefault();
  hideContextMenu();
  const factor = Math.exp(-e.deltaY * 0.0012);
  state.camZoom = Math.max(4.0, Math.min(65.0, state.camZoom * factor));
  render();
}, { passive: false });

window.addEventListener("resize", render);

buildChainMatrixUI();
updateLegend();
updateStatsPanel();
updateInspector();
render();
</script>
</body>
</html>
"##;
