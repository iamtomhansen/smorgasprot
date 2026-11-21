//! Bottom status bar with active dataset diagnostics, hinge position, and input controls help.

use crate::dataset::ViewerDataset;
use egui::{Color32, Ui};
use std::time::Instant;

pub fn render_status_bar(
    ui: &mut Ui,
    ds_opt: Option<&ViewerDataset>,
    status_message: &mut Option<(String, Instant)>,
) {
    ui.horizontal(|ui| {
        if let Some((msg, time)) = status_message {
            if time.elapsed().as_secs() < 6 {
                ui.colored_label(Color32::from_rgb(34, 197, 94), msg.as_str());
            } else {
                *status_message = None;
            }
        } else if let Some(ds) = ds_opt {
            ui.label(format!(
                "Active Complex: {} | {} Nodes | {} Cross-Contacts | Tangent Plane Hinge: {:.1} Å",
                ds.id,
                ds.map.nodes.len(),
                ds.map.edges.iter().filter(|e| e.is_cross_interface).count(),
                ds.map.plane.hinge_u,
            ));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label("Orbit: Left Drag • Pan: Shift+Left / Right Drag • Zoom: Scroll Wheel");
        });
    });
}
