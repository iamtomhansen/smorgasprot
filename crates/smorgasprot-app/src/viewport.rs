//! Central 3D canvas: camera input, picking, and dispatch to the active tab's scene.
//! Scenes are pure painters; only this module mutates camera and selection state.

mod complex_scene;
mod draw_list;
mod gizmo;
mod grid;
mod interface_scene;
mod plane_space;

use crate::dataset::ViewerDataset;
use crate::state::{UiState, WorkspaceTab};
use crate::style::palette;
use egui::{CursorIcon, PointerButton, Sense, Ui};

/// Allocates the remaining space in `ui` and renders the active tab's scene.
pub fn show(ui: &mut Ui, ds: &ViewerDataset, state: &mut UiState) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, palette::VIEWPORT_BG);

    let pointer = response.hover_pos();
    let over_gizmo = pointer.is_some_and(|p| gizmo::contains(rect, p));
    let over_roll_ring = pointer.is_some_and(|p| gizmo::is_over_roll_ring(rect, p));
    let gizmo_cap = pointer.and_then(|p| gizmo::hit_test(rect, &state.camera, p));

    // Gizmo roll ring or caps take precedence over main orbit navigation
    if response.dragged_by(PointerButton::Primary) && over_roll_ring {
        if let Some(p) = pointer {
            gizmo::handle_roll_drag(rect, &mut state.camera, p, response.drag_delta());
        }
    } else if response.double_clicked() && over_roll_ring {
        state.camera.roll = 0.0;
    } else if response.clicked()
        && let Some(axis) = gizmo_cap
    {
        state.camera.snap_to(axis);
    } else {
        let flat = state.tab == WorkspaceTab::Interface
            && state.interface.flatten.value > state.view_params.navigation.flat_pan_threshold;
        state.camera.handle_input(ui, &response, flat, &state.view_params.navigation);
    }

    grid::paint(&painter, rect, &state.camera, &state.view_params.render);

    match state.tab {
        WorkspaceTab::Complex => {
            state.selection.hovered_atom = None;
            complex_scene::paint(&painter, rect, ds, &state.camera, &state.complex);
        }
        WorkspaceTab::Interface => {
            let s = &state.interface;
            let nodes = interface_scene::project_nodes(ds, &state.camera, rect, s);

            // Hover picking is skipped over the gizmo so caps stay clickable.
            let hovered = pointer.filter(|_| !over_gizmo).and_then(|p| interface_scene::pick_atom(ds, s, &nodes, p));
            state.selection.hovered_atom = hovered;

            // Clicking an atom selects its residue territory; clicking empty space clears it.
            if response.clicked() && !over_gizmo {
                state.selection.selected_territory = hovered.and_then(|idx| ds.node(idx)).map(|n| n.territory);
            }
            interface_scene::paint(&painter, rect, ds, &state.camera, s, &state.selection, &nodes);
        }
    }

    gizmo::paint(&painter, rect, &state.camera, gizmo_cap, over_roll_ring);
    if gizmo_cap.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    } else if over_roll_ring {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
}
