//! Complex tab controls: group names, color palette, 3D representations with customization popovers, and explode separation.

use super::widgets::{
    alpha_slider, choice_combo, color_channel_control, customize_popover, popover_header, scalar_channel_control,
    section, tween_presets, tween_slider,
};
use crate::dataset::ViewerDataset;
use crate::state::{AtomSizeMode, ChainColorLens, ColorLens, InterfaceDisplayMode, UiState};
use egui::{RichText, Slider, Ui};
use smorgasprot_core::InterfaceSide;

/// Renders the Complex tab's side panel.
pub fn show(ui: &mut Ui, ds: &mut ViewerDataset, state: &mut UiState) {
    ui.heading("3D Complex Assembly");
    ui.add_space(4.0);

    section(ui, "Active interface", |ui| {
        ui.horizontal(|ui| {
            ui.label("Selected:");
            let current_name = match ds.active_interface() {
                Some(i) if !i.partition.name.is_empty() => i.partition.name.as_str(),
                Some(_) => "Active interface",
                None => "(No active interface)",
            };
            egui::ComboBox::from_id_salt("complex_active_interface_select")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut ds.active_interface, None, "(No active interface)");
                    if !ds.interfaces.is_empty() {
                        ui.separator();
                    }
                    for (idx, iface) in ds.interfaces.iter().enumerate() {
                        let label = if iface.partition.name.is_empty() {
                            format!("Interface {}", idx + 1)
                        } else {
                            iface.partition.name.clone()
                        };
                        ui.selectable_value(&mut ds.active_interface, Some(idx), label);
                    }
                });
            if !ds.interfaces.is_empty() && ui.small_button("Delete all interfaces").clicked() {
                ds.remove_all_interfaces();
            }
        });
    });

    // Display-only rename and color customization; does not affect chain assignment.
    section(ui, "1. Interface group names & palette", |ui| {
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
            egui::Grid::new("cx_group_names_grid")
                .num_columns(3)
                .spacing([6.0, 4.0])
                .show(ui, |ui| {
                    ui.color_edit_button_srgba(&mut state.palette.sides.side_a);
                    ui.label(RichText::new("Group A:").color(state.palette.sides.side_light(InterfaceSide::SideA)));
                    ui.text_edit_singleline(&mut iface.partition.group_a_name);
                    ui.end_row();

                    ui.color_edit_button_srgba(&mut state.palette.sides.side_b);
                    ui.label(RichText::new("Group B:").color(state.palette.sides.side_light(InterfaceSide::SideB)));
                    ui.text_edit_singleline(&mut iface.partition.group_b_name);
                    ui.end_row();
                });
        } else {
            ui.weak("No active interface selected.");
        }
    });

    let void_color = state.palette.sides.void;
    let s = &mut state.complex;
    section(ui, "2. 3D layers, lenses & opacity", |ui| {
        // Molecular Surface layer row + Customize popover
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.surface.visible, RichText::new("Surface mesh").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                customize_popover(ui, "cx_surface_popover", !s.surface.is_default_channels(), |ui| {
                    popover_header(ui, false, "Surface mesh layer", !s.surface.is_default_channels(), || {
                        s.surface.reset_channels();
                    });
                    color_channel_control(
                        ui,
                        "cx_surface_color_lens",
                        "Surface color",
                        &mut s.surface.color,
                        ChainColorLens::Fixed,
                    );
                    ui.label(RichText::new("Face culling:").strong());
                    choice_combo(ui, "cx_surface_culling", &mut s.surface.culling);
                });
            });
        });
        if s.surface.visible {
            alpha_slider(ui, "Alpha:", &mut s.surface.alpha);
        }

        ui.separator();

        // Cartoon Ribbon layer row + Customize popover
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.cartoon.visible, RichText::new("Cartoon ribbon").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                customize_popover(ui, "cx_cartoon_popover", !s.cartoon.is_default_channels(), |ui| {
                    popover_header(ui, false, "Cartoon ribbon layer", !s.cartoon.is_default_channels(), || {
                        s.cartoon.reset_channels();
                    });
                    color_channel_control(
                        ui,
                        "cx_cartoon_color_lens",
                        "Ribbon color",
                        &mut s.cartoon.color,
                        ChainColorLens::Fixed,
                    );
                    ui.label(RichText::new("Ribbon width scale:").strong());
                    ui.add(Slider::new(&mut s.cartoon.width_scale, 0.3..=2.5).step_by(0.05).text("× scale"));
                });
            });
        });
        if s.cartoon.visible {
            alpha_slider(ui, "Alpha:", &mut s.cartoon.alpha);
        }

        ui.separator();

        // 3D Interface Boundary mesh layer
        ui.label(RichText::new("3D interface boundary:").strong());
        choice_combo(ui, "cx_interface_mode", &mut s.boundary.mode);
        if s.boundary.mode != InterfaceDisplayMode::Off {
            alpha_slider(ui, "Alpha:", &mut s.boundary.alpha);
            ui.checkbox(&mut s.boundary.show_wireframe, "Show mesh wireframe");
        }
        if s.boundary.mode == InterfaceDisplayMode::NegativeSpace {
            if let Some(iface) = ds.active_interface() {
                ui.colored_label(void_color, format!("Sub-solvent void volume: {:.1} Å³", iface.geometry.void_volume_a3));
            }
        }

        ui.separator();

        // Interfacial Atom Spheres layer row + Customize popover
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.spheres.visible, RichText::new("Interfacial spheres").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                customize_popover(ui, "cx_spheres_popover", !s.spheres.is_default_channels(), |ui| {
                    popover_header(ui, false, "Interfacial spheres layer", !s.spheres.is_default_channels(), || {
                        s.spheres.reset_channels();
                    });
                    color_channel_control(
                        ui,
                        "cx_sphere_fill_lens",
                        "Sphere color",
                        &mut s.spheres.fill,
                        ColorLens::Fixed,
                    );
                    scalar_channel_control(
                        ui,
                        "cx_sphere_radius_mode",
                        "Sphere radius",
                        &mut s.spheres.radius,
                        AtomSizeMode::Fixed,
                        0.3..=2.5,
                        "× vdW scale",
                    );
                });
            });
        });
        if s.spheres.visible {
            alpha_slider(ui, "Alpha:", &mut s.spheres.alpha);
        }
    });

    let explode_params = &state.view_params.explode;
    section(ui, "3. Explode separation along ±n", |ui| {
        let label = format!("{:.1} Å", s.explode_distance_a(explode_params));
        tween_slider(ui, &mut s.explode, label);
        tween_presets(ui, &mut s.explode, &[("Closed", 0.0), ("Half", 0.5), ("Exploded", 1.0)]);
    });
}
