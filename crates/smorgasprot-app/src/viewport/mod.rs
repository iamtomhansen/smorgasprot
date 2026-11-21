//! Interactive 2D/3D central viewport canvas rendering, camera event handling, and scene dispatch.

pub mod complex_scene;
pub mod gizmo;
pub mod grid;
pub mod interface_scene;

use crate::camera::CameraState;
use crate::dataset::ViewerDataset;
use crate::state::{AtomSizeMode, BFactorMode, ColorLens, InterfaceDisplayMode, WorkspaceTab};
use complex_scene::render_complex_scene;
use gizmo::draw_orientation_gizmo;
use grid::draw_tangent_grid;
use interface_scene::render_interface_scene;
use egui::{Color32, PointerButton, Sense, Ui};

pub fn render_viewport(
    ui: &mut Ui,
    ds: &ViewerDataset,
    tab: WorkspaceTab,
    camera: &mut CameraState,
    // Complex params
    cx_show_surface: bool,
    cx_surface_alpha: f32,
    cx_cull_backfaces: bool,
    cx_show_cartoon: bool,
    cx_cartoon_alpha: f32,
    cx_interface_mode: InterfaceDisplayMode,
    cx_interface_alpha: f32,
    cx_show_spheres: bool,
    cx_spheres_alpha: f32,
    explode_t: f32,
    // Interface params
    flatten_t: f32,
    book_theta: f32,
    use_relaxed_uv: bool,
    lens: ColorLens,
    bfactor_mode: BFactorMode,
    size_mode: AtomSizeMode,
    side_a_alpha: f32,
    side_b_alpha: f32,
    show_territories: bool,
    show_intra_edges: bool,
    show_cross_arcs: bool,
    show_backbone: bool,
    // Selection state
    hovered_atom_idx: &mut Option<usize>,
    selected_territory_idx: &mut Option<usize>,
) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 0.0, Color32::from_rgb(248, 250, 252));

    // Handle interactive camera inputs
    if response.dragged_by(PointerButton::Primary) {
        if ui.input(|i| i.modifiers.shift) || (tab == WorkspaceTab::Interface && flatten_t > 0.92) {
            camera.pan[0] += response.drag_delta().x;
            camera.pan[1] += response.drag_delta().y;
        } else {
            camera.yaw += response.drag_delta().x * 0.008;
            camera.pitch = (camera.pitch + response.drag_delta().y * 0.008).clamp(-1.45, 1.45);
        }
    } else if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        camera.pan[0] += response.drag_delta().x;
        camera.pan[1] += response.drag_delta().y;
    }

    let scroll = ui.input(|i| i.raw_scroll_delta.y);
    if scroll.abs() > 0.01 {
        let factor = (-scroll * 0.002).exp();
        camera.zoom = (camera.zoom * factor).clamp(4.0, 75.0);
    }

    // Draw subtle coordinate grid on tangent plane
    draw_tangent_grid(&painter, rect, camera);

    match tab {
        WorkspaceTab::Complex => {
            render_complex_scene(
                &painter,
                rect,
                ds,
                camera,
                cx_show_surface,
                cx_surface_alpha,
                cx_cull_backfaces,
                cx_show_cartoon,
                cx_cartoon_alpha,
                cx_interface_mode,
                cx_interface_alpha,
                cx_show_spheres,
                cx_spheres_alpha,
                explode_t,
            );
        }
        WorkspaceTab::Interface => {
            render_interface_scene(
                &painter,
                rect,
                ds,
                camera,
                flatten_t,
                book_theta,
                use_relaxed_uv,
                lens,
                bfactor_mode,
                size_mode,
                side_a_alpha,
                side_b_alpha,
                show_territories,
                show_intra_edges,
                show_cross_arcs,
                show_backbone,
                hovered_atom_idx,
                selected_territory_idx,
                &response,
            );
        }
    }

    // Draw orientation gizmo in top right
    draw_orientation_gizmo(&painter, rect, camera);
}
