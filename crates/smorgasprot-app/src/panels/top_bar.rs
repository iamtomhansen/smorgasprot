//! Top application menu bar, file operations, camera alignment shortcuts, and dataset switcher.

use crate::camera::CameraState;
use crate::dataset::ViewerDataset;
use crate::state::WorkspaceTab;
use egui::Ui;

pub fn render_top_bar(
    ui: &mut Ui,
    datasets: &[ViewerDataset],
    active_ds_idx: &mut usize,
    workspace_tab: &mut WorkspaceTab,
    camera: &mut CameraState,
    side_a_alpha: &mut f32,
    side_b_alpha: &mut f32,
    export_requested: &mut bool,
) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("📁 Load Example 1: 7JVB (CryoEM Complex)").clicked() {
                if !datasets.is_empty() {
                    *active_ds_idx = 0;
                }
                ui.close_menu();
            }
            if ui.button("📁 Load Example 2: LatentX (De Novo Binder)").clicked() {
                if datasets.len() > 1 {
                    *active_ds_idx = 1;
                }
                ui.close_menu();
            }
            ui.separator();
            if ui.button("💾 Export Standalone HTML Report...").clicked() {
                *export_requested = true;
                ui.close_menu();
            }
        });

        ui.menu_button("View", |ui| {
            if ui.button("🧭 Reset Camera / Align to Tangent Plane Π").clicked() {
                camera.yaw = 0.0;
                camera.pitch = 0.0;
                camera.pan = [0.0, 0.0];
                camera.zoom = if *workspace_tab == WorkspaceTab::Interface { 18.0 } else { 14.5 };
                ui.close_menu();
            }
            ui.separator();
            if ui.button("🔵 Solo Group A").clicked() {
                *side_a_alpha = 1.0;
                *side_b_alpha = 0.05;
                ui.close_menu();
            }
            if ui.button("🟠 Solo Group B").clicked() {
                *side_a_alpha = 0.05;
                *side_b_alpha = 1.0;
                ui.close_menu();
            }
            if ui.button("👁️ Show Both Groups").clicked() {
                *side_a_alpha = 1.0;
                *side_b_alpha = 0.85;
                ui.close_menu();
            }
        });

        ui.separator();

        // Workspace tab buttons
        ui.selectable_value(
            workspace_tab,
            WorkspaceTab::Complex,
            "🧊 1. Complex (3D Sandwich)",
        );
        ui.selectable_value(
            workspace_tab,
            WorkspaceTab::Interface,
            "🗺️ 2. Interface (2D/3D Cartography)",
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !datasets.is_empty() {
                egui::ComboBox::from_id_salt("dataset_select")
                    .selected_text(&datasets[*active_ds_idx].title)
                    .show_ui(ui, |ui| {
                        for (idx, ds) in datasets.iter().enumerate() {
                            ui.selectable_value(active_ds_idx, idx, &ds.title);
                        }
                    });
                ui.label("Dataset:");
            }
        });
    });
}
