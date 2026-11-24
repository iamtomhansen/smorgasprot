//! Central 3D canvas: camera input, picking, and dispatch to the active tab's scene.
//! Scenes are pure painters; only this module mutates camera and selection state.

mod cartography_scene;
mod complex_scene;
mod draw_list;
mod gizmo;
pub mod gpu_surface;
mod grid;
mod plane_space;

use crate::dataset::ViewerDataset;
use crate::state::{UiState, WorkspaceTab};
use egui::{CursorIcon, PointerButton, Sense, Ui};
use gpu_surface::GpuSurfaceRenderer;

/// Allocates the remaining space in `ui` and renders the active tab's scene.
/// `gpu_surfaces` is `None` on the CPU-only fallback path (see `GpuSurfaceRenderer::new`).
pub fn show(ui: &mut Ui, ds: &ViewerDataset, state: &mut UiState, gpu_surfaces: Option<&mut GpuSurfaceRenderer>) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, state.palette.chrome.viewport_bg);

    let pointer = response.hover_pos();
    let over_gizmo = pointer.is_some_and(|p| gizmo::contains(rect, p));
    let over_roll_ring = pointer.is_some_and(|p| gizmo::is_over_roll_ring(rect, p));
    let gizmo_cap = pointer.and_then(|p| gizmo::hit_test(rect, state.active_camera(), p));

    // Gizmo roll ring or caps take precedence over main orbit navigation
    if response.dragged_by(PointerButton::Primary) && over_roll_ring {
        if let Some(p) = pointer {
            gizmo::handle_roll_drag(rect, state.active_camera_mut(), p, response.drag_delta());
        }
    } else if response.double_clicked() && over_roll_ring {
        state.active_camera_mut().roll = 0.0;
    } else if response.clicked()
        && let Some(axis) = gizmo_cap
    {
        state.active_camera_mut().snap_to(axis);
    } else {
        let flat = state.active_tab == WorkspaceTab::Cartography
            && state.cartography.flatten.value > state.view_params.navigation.flat_pan_threshold;
        let nav = state.view_params.navigation.clone();
        state.active_camera_mut().handle_input(ui, &response, flat, &nav);
    }

    grid::paint(&painter, rect, state.active_camera(), &state.view_params.grid, &state.palette.chrome);

    match state.active_tab {
        WorkspaceTab::Complex => {
            state.selection.hovered_atom = None;
            complex_scene::paint(&painter, rect, ds, state, gpu_surfaces);
        }
        WorkspaceTab::Cartography => {
            let nodes = cartography_scene::project_nodes(ds, rect, &state.cartography);

            // Hover picking is skipped over the gizmo so caps stay clickable.
            let hovered_node = pointer
                .filter(|_| !over_gizmo)
                .and_then(|p| cartography_scene::pick_atom(ds, &state.cartography, &nodes, p))
                .and_then(|idx| ds.node(idx));
            state.selection.hovered_atom = hovered_node.map(|n| n.global_atom);

            // Clicking an atom selects its residue globally across workspaces; clicking empty space clears it.
            if response.clicked() && !over_gizmo {
                state.selection.selected_atom = hovered_node.map(|n| n.global_atom);
                state.selection.selected_residue = hovered_node
                    .and_then(|n| ds.territory(n.territory))
                    .map(|t| t.global_residue);
            }
            cartography_scene::paint(&painter, rect, ds, state, &nodes);
        }
    }

    gizmo::paint(&painter, rect, state.active_camera(), gizmo_cap, over_roll_ring, &state.palette.gizmo, &state.palette.chrome);
    if gizmo_cap.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    } else if over_roll_ring {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
}
