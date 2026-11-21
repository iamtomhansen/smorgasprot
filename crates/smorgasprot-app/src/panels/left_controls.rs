//! Left sidebar control panel: 3D complex representations, unwrap interpolation, butterfly dihedral, and lenses.

use crate::dataset::ViewerDataset;
use crate::state::{AtomSizeMode, BFactorMode, ColorLens, InterfaceDisplayMode, WorkspaceTab};
use egui::{Color32, Ui};

pub fn render_left_panel(
    ui: &mut Ui,
    ds: &mut ViewerDataset,
    tab: &mut WorkspaceTab,
    // Complex controls
    cx_show_surface: &mut bool,
    cx_surface_alpha: &mut f32,
    cx_cull_backfaces: &mut bool,
    cx_show_cartoon: &mut bool,
    cx_cartoon_alpha: &mut f32,
    cx_interface_mode: &mut InterfaceDisplayMode,
    cx_interface_alpha: &mut f32,
    cx_show_spheres: &mut bool,
    cx_spheres_alpha: &mut f32,
    explode_t: &mut f32,
    target_explode_t: &mut f32,
    // Interface controls
    flatten_t: &mut f32,
    target_flatten_t: &mut f32,
    book_theta: &mut f32,
    target_book_theta: &mut f32,
    use_relaxed_uv: &mut bool,
    lens: &mut ColorLens,
    bfactor_mode: &mut BFactorMode,
    size_mode: &mut AtomSizeMode,
    side_a_alpha: &mut f32,
    side_b_alpha: &mut f32,
    show_territories: &mut bool,
    show_intra_edges: &mut bool,
    show_cross_arcs: &mut bool,
    show_backbone: &mut bool,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        match *tab {
            WorkspaceTab::Complex => {
                ui.heading("🧊 3D Complex Sandwich Setup");
                ui.add_space(4.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("1. Interface Group Names").strong().color(Color32::from_rgb(14, 165, 233)));
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Group A:").color(Color32::from_rgb(56, 189, 248)));
                        ui.text_edit_singleline(&mut ds.group_a_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Group B:").color(Color32::from_rgb(251, 146, 60)));
                        ui.text_edit_singleline(&mut ds.group_b_name);
                    });
                });

                ui.add_space(6.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("2. 3D Representations & Opacity").strong().color(Color32::from_rgb(14, 165, 233)));

                    ui.checkbox(cx_show_surface, "Surface Mesh (Atomic skin)");
                    if *cx_show_surface {
                        ui.horizontal(|ui| {
                            ui.label("Alpha:");
                            ui.add(egui::Slider::new(cx_surface_alpha, 0.05..=1.0).step_by(0.01));
                        });
                        ui.checkbox(cx_cull_backfaces, "Cull Backfaces (Single-wall)");
                    }

                    ui.separator();
                    ui.checkbox(cx_show_cartoon, "Cartoon Ribbon (Backbone trace)");
                    if *cx_show_cartoon {
                        ui.horizontal(|ui| {
                            ui.label("Alpha:");
                            ui.add(egui::Slider::new(cx_cartoon_alpha, 0.05..=1.0).step_by(0.01));
                        });
                    }

                    ui.separator();
                    ui.label(egui::RichText::new("Display Interface:").strong());
                    egui::ComboBox::from_id_salt("cx_iface_mode")
                        .selected_text(cx_interface_mode.label())
                        .show_ui(ui, |ui| {
                            for m in InterfaceDisplayMode::ALL {
                                ui.selectable_value(cx_interface_mode, m, m.label());
                            }
                        });

                    if *cx_interface_mode != InterfaceDisplayMode::Off {
                        ui.horizontal(|ui| {
                            ui.label("Interface Opacity:");
                            ui.add(egui::Slider::new(cx_interface_alpha, 0.05..=1.0).step_by(0.01));
                        });
                    }

                    if *cx_interface_mode == InterfaceDisplayMode::NegativeSpace {
                        ui.colored_label(
                            Color32::from_rgb(6, 182, 212),
                            format!("Sub-Solvent Void Volume: {:.1} Å³", ds.geometry.void_volume_a3),
                        );
                    }

                    ui.separator();
                    ui.checkbox(cx_show_spheres, "Spheres (Interfacial Heavy Atoms)");
                    if *cx_show_spheres {
                        ui.horizontal(|ui| {
                            ui.label("Alpha:");
                            ui.add(egui::Slider::new(cx_spheres_alpha, 0.05..=1.0).step_by(0.01));
                        });
                    }
                });

                ui.add_space(6.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("3. Explode Separation Along ±n").strong().color(Color32::from_rgb(14, 165, 233)));
                    let cur_explode = *explode_t;
                    ui.add(egui::Slider::new(explode_t, 0.0..=1.0).text(format!("{:.1} Å", cur_explode * 24.0)));
                    *target_explode_t = *explode_t;

                    ui.horizontal(|ui| {
                        if ui.button("Closed (0 Å)").clicked() { *target_explode_t = 0.0; }
                        if ui.button("Half (12 Å)").clicked() { *target_explode_t = 0.5; }
                        if ui.button("Exploded (24 Å)").clicked() { *target_explode_t = 1.0; }
                    });
                });

                ui.add_space(10.0);
                if ui.button(egui::RichText::new("Inspect Unwrapped Interface Map →").strong()).clicked() {
                    *tab = WorkspaceTab::Interface;
                }
            }

            WorkspaceTab::Interface => {
                ui.heading("🗺️ 2D/3D Interface Cartography");
                ui.add_space(4.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("1. 3D ↔ 2D Flattening (t)").strong().color(Color32::from_rgb(14, 165, 233)));
                    let mode_str = if *flatten_t < 0.05 { "3D" } else if *flatten_t > 0.95 { "2D Map" } else { "Bas-Relief" };
                    ui.add(egui::Slider::new(flatten_t, 0.0..=1.0).text(mode_str));
                    *target_flatten_t = *flatten_t;

                    ui.horizontal(|ui| {
                        if ui.button("3D (0.0)").clicked() { *target_flatten_t = 0.0; }
                        if ui.button("Bas-Relief (0.5)").clicked() { *target_flatten_t = 0.5; }
                        if ui.button("2D Map (1.0)").clicked() { *target_flatten_t = 1.0; }
                    });
                    ui.checkbox(use_relaxed_uv, "Use Spring-Relaxed 2D (vs. Tangent Plane)");
                });

                ui.add_space(6.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("2. Overlay ↔ Open-Book Butterfly (θ)").strong().color(Color32::from_rgb(14, 165, 233)));
                    let deg = (*book_theta * 180.0).round() as i32;
                    let book_str = if *book_theta < 0.05 { "Overlay".to_string() } else { format!("{deg}°") };
                    ui.add(egui::Slider::new(book_theta, 0.0..=1.0).text(book_str));
                    *target_book_theta = *book_theta;

                    ui.horizontal(|ui| {
                        if ui.button("Overlay (0°)").clicked() { *target_book_theta = 0.0; }
                        if ui.button("90°").clicked() { *target_book_theta = 0.5; }
                        if ui.button("Open-Book (180°)").clicked() {
                            *target_book_theta = 1.0;
                            if *side_b_alpha < 0.85 { *side_b_alpha = 1.0; }
                        }
                    });
                });

                ui.add_space(6.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("3. Visual Lenses & Sizing").strong().color(Color32::from_rgb(14, 165, 233)));

                    ui.label("Color Lens:");
                    egui::ComboBox::from_id_salt("color_lens")
                        .selected_text(lens.label())
                        .show_ui(ui, |ui| {
                            for l in ColorLens::ALL {
                                ui.selectable_value(lens, l, l.label());
                            }
                        });

                    if *lens == ColorLens::Bfactor {
                        ui.label("B-Factor Mode:");
                        egui::ComboBox::from_id_salt("bfactor_mode")
                            .selected_text(bfactor_mode.label())
                            .show_ui(ui, |ui| {
                                for b in BFactorMode::ALL {
                                    ui.selectable_value(bfactor_mode, b, b.label());
                                }
                            });
                    }

                    ui.label("Atom Size:");
                    egui::ComboBox::from_id_salt("size_mode")
                        .selected_text(size_mode.label())
                        .show_ui(ui, |ui| {
                            for s in AtomSizeMode::ALL {
                                ui.selectable_value(size_mode, s, s.label());
                            }
                        });
                });

                ui.add_space(6.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("4. Group Opacity & Visibility").strong().color(Color32::from_rgb(14, 165, 233)));
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{}:", ds.group_a_name)).color(Color32::from_rgb(56, 189, 248)));
                        ui.add(egui::Slider::new(side_a_alpha, 0.05..=1.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{}:", ds.group_b_name)).color(Color32::from_rgb(251, 146, 60)));
                        ui.add(egui::Slider::new(side_b_alpha, 0.05..=1.0));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Solo A").clicked() { *side_a_alpha = 1.0; *side_b_alpha = 0.05; }
                        if ui.button("Solo B").clicked() { *side_a_alpha = 0.05; *side_b_alpha = 1.0; }
                        if ui.button("Both").clicked() { *side_a_alpha = 1.0; *side_b_alpha = 0.85; }
                    });

                    ui.separator();
                    ui.checkbox(show_territories, "Residue Territory Contours");
                    ui.checkbox(show_intra_edges, "1st-Shell Intra-Chain Springs");
                    ui.checkbox(show_cross_arcs, "Cross-Interface Contact Arcs");
                    ui.checkbox(show_backbone, "3D Cα Backbone Ribbons");
                });
            }
        }
    });
}
