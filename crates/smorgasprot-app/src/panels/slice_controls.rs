//! Complex tab controls: group names, color palette, 3D representations with customization popovers, and explode separation.

use super::widgets::{
    UNIT_RANGE, choice_combo, max_text_width, presentation_popover, row_height, section, slider, sphere_popovers,
    tween_presets, tween_slider,
};
use crate::dataset::ViewerDataset;
use crate::state::{
    BoundaryLayerStyle, InterfaceDisplayMode, PresentationCaps, UiState, WorkspaceTab,
};
use egui::{Color32, RichText, ScrollArea, Ui};
use smorgasprot_core::InterfaceSide;

/// Renders the Slice tab's side panel.
pub fn show(ui: &mut Ui, ds: &mut ViewerDataset, state: &mut UiState) {
    ui.heading("Interface Slicing");
    ui.add_space(4.0);

    let danger_color = state.palette.chrome.error;
    section(ui, "Active interface", |ui| {
        active_interface_section(ui, ds, danger_color);
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
    let mut open_garnish = false;
    let s = &mut state.slice;
    section(ui, "2. Complex display", |ui| {
        ui.horizontal(|ui| {
            ui.weak("Inherits styles from Garnish.");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Edit in Garnish ->").clicked() {
                    open_garnish = true;
                }
            });
        });

        ui.checkbox(&mut s.show_surface, RichText::new("Surface mesh").strong());
        if s.show_surface {
            slider(ui, "Alpha:", &mut s.surface_alpha, UNIT_RANGE);
        }

        ui.checkbox(&mut s.show_cartoon, RichText::new("Cartoon ribbon").strong());
        if s.show_cartoon {
            slider(ui, "Alpha:", &mut s.cartoon_alpha, UNIT_RANGE);
        }

        ui.checkbox(&mut s.show_spheres, RichText::new("Atom spheres (whole complex)").strong());
        if s.show_spheres {
            slider(ui, "Alpha:", &mut s.spheres_alpha, UNIT_RANGE);
        }
    });

    section(ui, "3. Interface analysis", |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("3D interface boundary:").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if s.boundary.mode != InterfaceDisplayMode::Off {
                    presentation_popover(
                        ui,
                        "cx_boundary_pres",
                        "Boundary presentation",
                        &mut s.boundary.presentation,
                        &BoundaryLayerStyle::default_presentation(),
                        PresentationCaps {
                            shading: true,
                            culling: true,
                            border: true,
                            wireframe: true,
                            fill_alpha: true,
                        },
                        false,
                        |_, _| {},
                    );
                }
            });
        });
        choice_combo(ui, "cx_interface_mode", &mut s.boundary.mode);
        if s.boundary.mode != InterfaceDisplayMode::Off {
            slider(ui, "Alpha:", &mut s.boundary.alpha, UNIT_RANGE);
        }
        if s.boundary.mode == InterfaceDisplayMode::NegativeSpace
            && let Some(iface) = ds.active_interface()
        {
            ui.colored_label(void_color, format!("Sub-solvent void volume: {:.1} Å³", iface.analysis_void_volume_a3));
        }

        ui.separator();

        ui.horizontal(|ui| {
            ui.checkbox(&mut s.interface_spheres_visible, RichText::new("Interface spheres").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                sphere_popovers(ui, "cx_iface_spheres", "Interface spheres", &mut s.interface_spheres);
            });
        });
        if s.interface_spheres_visible {
            slider(ui, "Alpha:", &mut s.interface_spheres_alpha, UNIT_RANGE);
        }
    });

    let explode_params = &state.view_params.explode;
    let mean_complex_disp_a =
        ds.mean_chain_explode_distance_a() * s.complex_explode.value * explode_params.complex_explode_scale;
    section(ui, "4. Explode separation", |ui| {
        ui.add_enabled_ui(ds.chains.len() > 1, |ui| {
            ui.label(RichText::new("Complex subunits:").strong());
            let label = format!("{:.1} Å", mean_complex_disp_a);
            tween_slider(ui, &mut s.complex_explode, label);
            tween_presets(ui, &mut s.complex_explode, &[("Closed", 0.0), ("Half", 0.5), ("Exploded", 1.0)]);
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);

        ui.add_enabled_ui(ds.active_interface().is_some(), |ui| {
            ui.label(RichText::new("Active interface (±n):").strong());
            let label = format!("{:.1} Å", s.explode_distance_a(explode_params));
            tween_slider(ui, &mut s.explode, label);
            tween_presets(ui, &mut s.explode, &[("Closed", 0.0), ("Half", 0.5), ("Exploded", 1.0)]);
        });
    });

    if open_garnish {
        state.switch_tab(WorkspaceTab::Garnish);
    }
}

/// Stateful monitoring for short term UI interactions in the 'Complex' workspace.
#[derive(Clone, Default)]
struct InterfaceListUiState {
    renaming: Option<(usize, String)>,
    focus_rename: bool,
    confirm_delete: Option<ConfirmDelete>,
}

/// A pending deletion of a single interface or all interfaces, awaiting user confirmation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfirmDelete {
    Single(usize),
    All,
}

fn interface_display_name(ds: &ViewerDataset, idx: usize) -> String {
    match ds.interfaces.get(idx) {
        Some(iface) if !iface.partition.name.is_empty() => iface.partition.name.clone(),
        _ => format!("Interface {}", idx + 1),
    }
}

fn active_interface_section(ui: &mut Ui, ds: &mut ViewerDataset, danger_color: Color32) {
    let state_id = egui::Id::new("complex_active_interface_ui");
    let mut ui_state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<InterfaceListUiState>(state_id))
        .unwrap_or_default();

    if let Some((idx, _)) = &ui_state.renaming {
        if *idx >= ds.interfaces.len() {
            ui_state.renaming = None;
            ui_state.focus_rename = false;
        }
    }

    // Sets the height of the scrollable list to show a maximum of this many rows before scrolling.
    let visible_rows = 6.0;
    let btn_w = max_text_width(ui, &["New", "Rename", "Delete", "Delete all"], false)
        + ui.spacing().button_padding.x * 2.0
        + 4.0;
    let spacing = ui.spacing().item_spacing;
    let row_h = row_height(ui);
    let list_max_h = visible_rows * row_h + (visible_rows - 1.0) * spacing.y;
    let list_w = (ui.available_width() - btn_w - spacing.x).max(80.0);

    let mut active_idx = ds.active_interface.filter(|&i| i < ds.interfaces.len());

    ui.horizontal(|ui| {
        // The scrollable list of interfaces
        let inner_margin = 4;
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(inner_margin))
            .show(ui, |ui| {
                ui.set_width((list_w - (inner_margin as f32) * 2.0 - 2.0).max(40.0));
                ScrollArea::vertical()
                    .id_salt("complex_active_interface_scroll")
                    .max_height(list_max_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                            if ui
                                .selectable_label(ds.active_interface.is_none(), "(No active interface)")
                                .clicked()
                            {
                                ds.active_interface = None;
                                active_idx = None;
                                ui_state.renaming = None;
                            }

                            let mut clear_rename = false;
                            let mut commit_rename: Option<(usize, String)> = None;
                            let mut start_rename: Option<usize> = None;

                            for idx in 0..ds.interfaces.len() {
                                let is_renaming = ui_state.renaming.as_ref().is_some_and(|(r, _)| *r == idx);
                                if is_renaming {
                                    if let Some((_, buf)) = &mut ui_state.renaming {
                                        let resp = ui.add(
                                            egui::TextEdit::singleline(buf).desired_width(f32::INFINITY),
                                        );
                                        let requested_focus = std::mem::take(&mut ui_state.focus_rename);
                                        if requested_focus {
                                            resp.request_focus();
                                        }
                                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                            clear_rename = true;
                                        } else if !requested_focus && resp.lost_focus() {
                                            let trimmed = buf.trim().to_owned();
                                            if !trimmed.is_empty() {
                                                commit_rename = Some((idx, trimmed));
                                            }
                                            clear_rename = true;
                                        }
                                    }
                                    continue;
                                }

                                let label = interface_display_name(ds, idx);
                                let resp = ui.selectable_label(ds.active_interface == Some(idx), label);
                                if resp.clicked() {
                                    ds.active_interface = Some(idx);
                                    active_idx = Some(idx);
                                    ui_state.renaming = None;
                                }
                                if resp.double_clicked() {
                                    ds.active_interface = Some(idx);
                                    active_idx = Some(idx);
                                    start_rename = Some(idx);
                                }
                            }

                            if let Some((idx, new_name)) = commit_rename {
                                ds.interfaces[idx].partition.name = new_name;
                            }
                            if clear_rename {
                                ui_state.renaming = None;
                            }
                            if let Some(idx) = start_rename {
                                ui_state.renaming = Some((idx, interface_display_name(ds, idx)));
                                ui_state.focus_rename = true;
                            }
                        });
                    });
            });

        // Action buttons to the right of the interface selector.
        ui.vertical(|ui| {
            let btn_size = egui::vec2(btn_w, row_h);
            if ui.add(egui::Button::new("New").min_size(btn_size)).clicked() {
                // Intentional no-op until interface creation workflow is implemented.
            }
            if ui
                .add_enabled(active_idx.is_some(), egui::Button::new("Rename").min_size(btn_size))
                .clicked()
            {
                if let Some(idx) = active_idx {
                    ui_state.renaming = Some((idx, interface_display_name(ds, idx)));
                    ui_state.focus_rename = true;
                }
            }
            if ui
                .add_enabled(active_idx.is_some(), egui::Button::new("Delete").min_size(btn_size))
                .clicked()
            {
                if let Some(idx) = active_idx {
                    ui_state.confirm_delete = Some(ConfirmDelete::Single(idx));
                }
            }
            if ui
                .add_enabled(!ds.interfaces.is_empty(), egui::Button::new("Delete all").min_size(btn_size))
                .clicked()
            {
                ui_state.confirm_delete = Some(ConfirmDelete::All);
            }
        });
    });

    if ui_state.focus_rename {
        ui.ctx().request_repaint();
    }

    confirm_delete_dialog(ui.ctx(), ds, &mut ui_state, danger_color);
    ui.ctx().data_mut(|d| d.insert_temp(state_id, ui_state));
}

/// A pop-up confirming destructive deletion of an interface (or all interfaces) with a cancel button.
fn confirm_delete_dialog(
    ctx: &egui::Context,
    ds: &mut ViewerDataset,
    ui_state: &mut InterfaceListUiState,
    danger_color: Color32,
) {
    let Some(pending) = ui_state.confirm_delete else {
        return;
    };
    let message = match pending {
        ConfirmDelete::Single(idx) if idx < ds.interfaces.len() => {
            format!("Delete interface \"{}\"? This cannot be undone.", interface_display_name(ds, idx))
        }
        ConfirmDelete::All if !ds.interfaces.is_empty() => {
            format!("Delete all {} interfaces? This cannot be undone.", ds.interfaces.len())
        }
        _ => {
            ui_state.confirm_delete = None;
            return;
        }
    };

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui_state.confirm_delete = None;
        return;
    }

    egui::Window::new("Confirm deletion")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(message);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui_state.confirm_delete = None;
                }
                let delete_label = match pending {
                    ConfirmDelete::Single(_) => "Delete",
                    ConfirmDelete::All => "Delete all",
                };
                if ui
                    .add(
                        egui::Button::new(RichText::new(delete_label).strong().color(Color32::WHITE))
                            .fill(danger_color),
                    )
                    .clicked()
                {
                    match pending {
                        ConfirmDelete::Single(idx) => ds.remove_interface(idx),
                        ConfirmDelete::All => ds.remove_all_interfaces(),
                    }
                    ui_state.confirm_delete = None;
                    ui_state.renaming = None;
                }
            });
        });
}
