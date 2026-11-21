//! Top menu bar: File and View menus, workspace tabs, and the dataset picker.

use crate::dataset::ViewerDataset;
use crate::state::{Action, UiState, WorkspaceTab};
use egui::{Align, ComboBox, Layout, Ui};
use smorgasprot_core::InterfaceSide;

/// Renders the menu bar; app-level effects are pushed to `actions`.
pub fn show(ui: &mut Ui, datasets: &[ViewerDataset], active: usize, state: &mut UiState, actions: &mut Vec<Action>) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("💾 Export Standalone HTML Report…").clicked() {
                actions.push(Action::ExportHtml);
                ui.close_menu();
            }
            // Browsers own the tab lifecycle, so Quit is native-only.
            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });

        ui.menu_button("View", |ui| {
            if ui.button("🧭 Reset Camera / Align to Tangent Plane Π").clicked() {
                state.camera.reset();
                ui.close_menu();
            }
            ui.separator();
            let s = &mut state.interface;
            if ui.button("🔵 Solo Group A").clicked() {
                s.solo(InterfaceSide::SideA);
                ui.close_menu();
            }
            if ui.button("🟠 Solo Group B").clicked() {
                s.solo(InterfaceSide::SideB);
                ui.close_menu();
            }
            if ui.button("👁️ Show Both Groups").clicked() {
                s.show_both();
                ui.close_menu();
            }
        });

        ui.separator();
        ui.selectable_value(&mut state.tab, WorkspaceTab::Complex, "🧊 1. Complex (3D Sandwich)");
        ui.selectable_value(&mut state.tab, WorkspaceTab::Interface, "🗺️ 2. Interface (2D/3D Cartography)");

        // Right-aligned dataset picker (layout is right-to-left, so the label comes last).
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let Some(current) = datasets.get(active) else { return };
            ComboBox::from_id_salt("dataset_select").selected_text(&current.title).show_ui(ui, |ui| {
                for (idx, ds) in datasets.iter().enumerate() {
                    if ui.selectable_label(idx == active, &ds.title).clicked() && idx != active {
                        actions.push(Action::SelectDataset(idx));
                    }
                }
            });
            ui.label("Dataset:");
        });
    });
}
