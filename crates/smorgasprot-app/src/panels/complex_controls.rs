//! Complex tab controls: group names, color palette, 3D representations, and explode separation.

use super::widgets::{alpha_slider, choice_combo, section, tween_presets, tween_slider};
use crate::dataset::ViewerDataset;
use crate::state::{InterfaceDisplayMode, UiState, WorkspaceTab};
use egui::{RichText, Ui};
use smorgasprot_core::InterfaceSide;

/// Renders the Complex tab's side panel.
pub fn show(ui: &mut Ui, ds: &mut ViewerDataset, state: &mut UiState) {
    ui.heading("🧊 3D Complex Sandwich Setup");
    ui.add_space(4.0);

    if ds.interfaces.len() > 1 {
        section(ui, "Active Interface", |ui| {
            ui.horizontal(|ui| {
                ui.label("Selected:");
                let current_name = ds.active_interface().map_or("None", |i| i.partition.name.as_str());
                egui::ComboBox::from_id_salt("complex_active_interface_select")
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        for (idx, iface) in ds.interfaces.iter().enumerate() {
                            let label = if iface.partition.name.is_empty() {
                                format!("Interface {}", idx + 1)
                            } else {
                                iface.partition.name.clone()
                            };
                            ui.selectable_value(&mut ds.active_interface, idx, label);
                        }
                    });
            });
        });
    }

    // Display-only rename and color customization; does not affect chain assignment.
    section(ui, "1. Interface Group Names & Palette", |ui| {
        let mut chosen = None;
        ui.horizontal(|ui| {
            ui.label("Palette:");
            egui::ComboBox::from_id_salt("color_palette_preset")
                .selected_text(&state.palette.name)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for s in &state.palette.available {
                        if ui.selectable_label(s.name == state.palette.name, &s.name).clicked() {
                            chosen = Some(s.name.clone());
                        }
                    }
                });
        });
        if let Some(name) = chosen {
            state.palette.apply_by_name(&name);
        }

        if let Some(iface) = ds.active_interface_mut() {
            ui.horizontal(|ui| {
                ui.color_edit_button_srgba(&mut state.palette.sides.side_a);
                ui.label(RichText::new("Group A:").color(state.palette.sides.side_light(InterfaceSide::SideA)));
                ui.text_edit_singleline(&mut iface.partition.group_a_name);
            });
            ui.horizontal(|ui| {
                ui.color_edit_button_srgba(&mut state.palette.sides.side_b);
                ui.label(RichText::new("Group B:").color(state.palette.sides.side_light(InterfaceSide::SideB)));
                ui.text_edit_singleline(&mut iface.partition.group_b_name);
            });
        }
    });

    let void_color = state.palette.sides.void;
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
            if let Some(iface) = ds.active_interface() {
                ui.colored_label(void_color, format!("Sub-Solvent Void Volume: {:.1} Å³", iface.geometry.void_volume_a3));
            }
        }

        ui.separator();
        ui.checkbox(&mut s.show_spheres, "Spheres (Interfacial Heavy Atoms)");
        if s.show_spheres {
            alpha_slider(ui, "Alpha:", &mut s.spheres_alpha);
        }
    });

    let explode_params = &state.view_params.explode;
    section(ui, "3. Explode Separation Along ±n", |ui| {
        let label = format!("{:.1} Å", s.explode_distance_a(explode_params));
        tween_slider(ui, &mut s.explode, label);
        tween_presets(ui, &mut s.explode, &[("Closed (0 Å)", 0.0), ("Half (12 Å)", 0.5), ("Exploded (24 Å)", 1.0)]);
    });

    ui.add_space(4.0);
    if ui.button(RichText::new("Inspect Unwrapped Interface Map →").strong()).clicked() {
        state.switch_tab(WorkspaceTab::Interface);
    }
}
