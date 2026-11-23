//! Bottom status bar: transient or persistent messages, or a one-line summary of the active dataset.

use crate::action::Action;
use crate::dataset::ViewerDataset;
use crate::status::{StatusKind, StatusMessage};
use crate::style::ChromePalette;
use egui::{Align, Layout, Ui};

/// Renders the status bar; expiry of `message` is handled by the app, and manual dismissal pushes `Action::DismissStatus`.
pub fn show(
    ui: &mut Ui,
    ds: Option<&ViewerDataset>,
    message: Option<&StatusMessage>,
    chrome: &ChromePalette,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        if let Some(msg) = message {
            let color = match msg.kind {
                StatusKind::Info => chrome.accent,
                StatusKind::Success => chrome.success,
                StatusKind::Warning => chrome.warning,
                StatusKind::Error => chrome.error,
            };
            ui.colored_label(color, &msg.text);
            if ui.small_button("×").on_hover_text("Dismiss message").clicked() {
                actions.push(Action::DismissStatus);
            }
        } else if let Some(ds) = ds {
            if let Some(iface) = ds.active_interface() {
                ui.label(format!(
                    "Active Complex: {} | {} Nodes | {} Cross-Contacts | Tangent Plane Hinge: {:.1} Å",
                    ds.id,
                    iface.map.nodes.len(),
                    ds.cross_contact_count(),
                    iface.map.plane.hinge_u,
                ));
            } else {
                let total_atoms: usize = ds.chains.iter().map(|c| c.atom_count).sum();
                ui.label(format!(
                    "Active Complex: {} (Bare Complex) | {} Chains | {} Heavy Atoms",
                    ds.id,
                    ds.chains.len(),
                    total_atoms,
                ));
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.weak("Orbit: Left Drag • Pan: Shift+Left / Right Drag • Zoom: Scroll / Pinch • Click gizmo to snap");
        });
    });
}
