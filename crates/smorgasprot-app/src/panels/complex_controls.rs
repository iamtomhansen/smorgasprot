//! Complex tab controls: group names, 3D representations, and explode separation.

use super::widgets::{alpha_slider, choice_combo, section, tween_presets, tween_slider};
use crate::dataset::ViewerDataset;
use crate::state::{InterfaceDisplayMode, UiState, WorkspaceTab};
use crate::style::palette;
use egui::{RichText, Ui};

/// Renders the Complex tab's side panel.
pub fn show(ui: &mut Ui, ds: &mut ViewerDataset, state: &mut UiState) {
    ui.heading("🧊 3D Complex Sandwich Setup");
    ui.add_space(4.0);

    // Display-only rename; does not affect chain assignment.
    section(ui, "1. Interface Group Names", |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Group A:").color(palette::SIDE_A_LIGHT));
            ui.text_edit_singleline(&mut ds.group_a_name);
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Group B:").color(palette::SIDE_B_LIGHT));
            ui.text_edit_singleline(&mut ds.group_b_name);
        });
    });

    let s = &mut state.complex;
    section(ui, "2. 3D Representations & Opacity", |ui| {
        ui.checkbox(&mut s.show_surface, "Surface Mesh (Atomic skin)");
        if s.show_surface {
            alpha_slider(ui, "Alpha:", &mut s.surface_alpha);
            ui.checkbox(&mut s.cull_backfaces, "Cull Backfaces (Single-wall)");
        }

        ui.separator();
        ui.checkbox(&mut s.show_cartoon, "Cartoon Ribbon (Backbone trace)");
        if s.show_cartoon {
            alpha_slider(ui, "Alpha:", &mut s.cartoon_alpha);
        }

        ui.separator();
        ui.label(RichText::new("Display Interface:").strong());
        choice_combo(ui, "cx_interface_mode", &mut s.interface_mode);
        if s.interface_mode != InterfaceDisplayMode::Off {
            alpha_slider(ui, "Interface Opacity:", &mut s.interface_alpha);
        }
        if s.interface_mode == InterfaceDisplayMode::NegativeSpace {
            ui.colored_label(palette::VOID, format!("Sub-Solvent Void Volume: {:.1} Å³", ds.geometry.void_volume_a3));
        }

        ui.separator();
        ui.checkbox(&mut s.show_spheres, "Spheres (Interfacial Heavy Atoms)");
        if s.show_spheres {
            alpha_slider(ui, "Alpha:", &mut s.spheres_alpha);
        }
    });

    section(ui, "3. Explode Separation Along ±n", |ui| {
        let label = format!("{:.1} Å", s.explode_distance_a());
        tween_slider(ui, &mut s.explode, label);
        tween_presets(ui, &mut s.explode, &[("Closed (0 Å)", 0.0), ("Half (12 Å)", 0.5), ("Exploded (24 Å)", 1.0)]);
    });

    ui.add_space(4.0);
    if ui.button(RichText::new("Inspect Unwrapped Interface Map →").strong()).clicked() {
        state.tab = WorkspaceTab::Interface;
    }
}
