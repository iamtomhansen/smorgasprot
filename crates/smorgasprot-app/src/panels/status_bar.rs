//! Bottom status bar: transient messages, or a one-line summary of the active dataset.

use crate::dataset::ViewerDataset;
use crate::state::StatusMessage;
use crate::style::palette;
use egui::{Align, Layout, Ui};

/// Renders the status bar; expiry of `message` is handled by the app.
pub fn show(ui: &mut Ui, ds: Option<&ViewerDataset>, message: Option<&StatusMessage>) {
    ui.horizontal(|ui| {
        if let Some(msg) = message {
            let color = if msg.is_error { palette::ERROR } else { palette::SUCCESS };
            ui.colored_label(color, &msg.text);
        } else if let Some(ds) = ds {
            ui.label(format!(
                "Active Complex: {} | {} Nodes | {} Cross-Contacts | Tangent Plane Hinge: {:.1} Å",
                ds.id,
                ds.map.nodes.len(),
                ds.cross_contact_count(),
                ds.map.plane.hinge_u,
            ));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.weak("Orbit: Left Drag • Pan: Shift+Left / Right Drag • Zoom: Scroll / Pinch • Click gizmo to snap");
        });
    });
}
