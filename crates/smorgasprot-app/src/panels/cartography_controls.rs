//! Cartography ("Sandwich") tab controls: flatten and butterfly morphs, quick presets, layer
//! customization popovers, and group visibility.

use super::widgets::{
    EDGE_WIDTH_RANGE, RADIUS_RANGE, UNIT_RANGE, bare_slider, choice_combo, choice_combo_short, color_channel_control,
    named_popover, param_group_controls, popover_header, presentation_popover, scalar_channel_control, section, slider,
    tween_presets, tween_slider,
};
use crate::dataset::ViewerDataset;
use crate::state::{
    AtomLayerStyle, AtomSizeMode, BoundaryLayerStyle, ChannelPreset, ColorLens, EdgeColorLens, EdgeLayerStyle,
    EdgeWidthLens, LabelBuilder, NodeStrokeColorLens, PresentationCaps, RibbonStyle, SheetColorMode,
    TerritoryColorLens, TerritoryLayerStyle, TerritoryStrokeLens, Tween, UiState,
};
use egui::{Align, ComboBox, Grid, Layout, RichText, Ui};
use smorgasprot_core::InterfaceSide;

/// Debounce delay (s) for live residue territory contour remeshing.
const CONTOUR_REMESH_DEBOUNCE_S: f64 = 0.2;

/// Renders the Sandwich (Cartography) tab's side panel.
pub fn show(ui: &mut Ui, ds: &mut ViewerDataset, state: &mut UiState) {
    ui.heading("Sandwich Cartography");
    ui.add_space(4.0);
    let side_a_color = state.palette.sides.side_light(InterfaceSide::SideA);
    let side_b_color = state.palette.sides.side_light(InterfaceSide::SideB);
    let open_min_b_alpha = state.view_params.morph.open_sandwich_min_b_alpha;
    let s = &mut state.cartography;

    section(ui, "1. 3D ↔ 2D Flattening", |ui| {
        let t = s.flatten.value;
        let label = if t < 0.05 { "3D" } else if t > 0.95 { "2D map" } else { "bas-relief" };
        tween_slider(ui, &mut s.flatten, label);
        tween_presets(ui, &mut s.flatten, &[("3D (0.0)", 0.0), ("Bas-relief (0.5)", 0.5), ("2D map (1.0)", 1.0)]);
        ui.checkbox(&mut s.use_relaxed_uv, "Spring-relaxed 2D (vs. tangent plane)");
        ui.separator();
        let sheet_color_dirty = s.sheet_color != SheetColorMode::default();
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.sheet.visible, RichText::new("Interface sheet").strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if s.sheet.visible
                    && presentation_popover(
                        ui,
                        "carto_sheet_pres",
                        "Interface sheet presentation",
                        &mut s.sheet.presentation,
                        &BoundaryLayerStyle::default_presentation(),
                        PresentationCaps {
                            shading: true,
                            culling: true,
                            border: true,
                            wireframe: true,
                            fill_alpha: true,
                        },
                        sheet_color_dirty,
                        |ui, _| {
                            ui.horizontal(|ui| {
                                ui.label("Coloring:");
                                ComboBox::from_id_salt("carto_sheet_color_mode")
                                    .selected_text(s.sheet_color.label())
                                    .show_ui(ui, |ui| {
                                        for &mode in SheetColorMode::ALL {
                                            ui.selectable_value(&mut s.sheet_color, mode, mode.label());
                                        }
                                    });
                            });
                            if s.sheet_color == SheetColorMode::Fixed {
                                ui.horizontal(|ui| {
                                    ui.label("Color:");
                                    ui.color_edit_button_srgba(&mut s.sheet_fixed_color);
                                });
                            }
                            ui.separator();
                        },
                    )
                {
                    s.sheet_color = SheetColorMode::default();
                }
            });
        });
        if s.sheet.visible {
            slider(ui, "Alpha:", &mut s.sheet.alpha, UNIT_RANGE);
        }
    });

    section(ui, "2. Overlay ↔ Open sandwich (θ)", |ui| {
        let t = s.book.value;
        let label = if t < 0.05 { "Overlay".to_owned() } else { format!("{}°", (t * 180.0).round() as i32) };
        tween_slider(ui, &mut s.book, label);
        let preset = tween_presets(ui, &mut s.book, &[("Overlay (0°)", 0.0), ("Tilt (45°)", 0.25), ("Open (180°)", 1.0)]);
        // Opening the sandwich exposes side B, so make sure it is visible.
        if preset == Some(1.0) && s.side_b_alpha.target < open_min_b_alpha {
            s.side_b_alpha.animate_to(1.0);
        }
    });

    section(ui, "3. Visual lenses & layers", |ui| {
        ui.label(RichText::new("Quick presets:").strong());
        ui.horizontal_wrapped(|ui| {
            for &preset in ChannelPreset::ALL {
                if ui.button(preset.label()).clicked() {
                    s.apply_preset(preset);
                }
            }
        });

        ui.separator();
        let now = ui.input(|i| i.time);

        // ── Atom nodes ──────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.atoms.visible, RichText::new("Atom nodes").strong());
            if s.atoms.visible {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let is_mesh = s.atoms.presentation.quality.geodesic_preset().is_some();
                    presentation_popover(
                        ui,
                        "if_atoms_pres",
                        "Atom node presentation",
                        &mut s.atoms.presentation,
                        &AtomLayerStyle::default_presentation(),
                        PresentationCaps {
                            shading: is_mesh,
                            culling: is_mesh,
                            border: is_mesh,
                            wireframe: is_mesh,
                            fill_alpha: true,
                        },
                        false,
                        |ui, pres| {
                            ui.label(RichText::new("Quality / Geometry").strong());
                            choice_combo(ui, "if_atoms_quality", &mut pres.quality);
                        },
                    );
                    let chan_mod = !s.atoms.channels_default();
                    named_popover(ui, "if_atoms_chan", "Channels", chan_mod, |ui| {
                        popover_header(ui, false, "Atom node channels", chan_mod, || s.atoms.reset_channels());
                        color_channel_control(ui, "if_node_fill_lens", "Fill color", &mut s.atoms.fill, ColorLens::Fixed);
                        scalar_channel_control(
                            ui,
                            "if_node_size_mode",
                            "Node radius",
                            &mut s.atoms.radius,
                            AtomSizeMode::Fixed,
                            RADIUS_RANGE,
                            "x scale",
                        );
                        color_channel_control(
                            ui,
                            "if_node_stroke_color",
                            "Border color",
                            &mut s.atoms.stroke_color,
                            NodeStrokeColorLens::Fixed,
                        );
                        scalar_channel_control(
                            ui,
                            "if_node_stroke_width",
                            "Border width",
                            &mut s.atoms.stroke_width,
                            AtomSizeMode::Fixed,
                            EDGE_WIDTH_RANGE,
                            "px",
                        );
                    });
                });
            }
        });

        // ── Contact edges ───────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.edges.visible, RichText::new("Contact edges").strong());
            if s.edges.visible {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let is_prism = s.edges.presentation.quality == RibbonStyle::Prism8;
                    presentation_popover(
                        ui,
                        "if_edges_pres",
                        "Contact edge presentation",
                        &mut s.edges.presentation,
                        &EdgeLayerStyle::default_presentation(),
                        PresentationCaps {
                            shading: is_prism,
                            culling: is_prism,
                            border: is_prism,
                            wireframe: is_prism,
                            fill_alpha: true,
                        },
                        false,
                        |ui, pres| {
                            ui.label(RichText::new("Quality / Geometry").strong());
                            choice_combo(ui, "if_edges_quality", &mut pres.quality);
                        },
                    );
                    let filt_mod = !s.edges.filters_default();
                    named_popover(ui, "if_edges_filter", "Filter", filt_mod, |ui| {
                        popover_header(ui, false, "Contact edge filters", filt_mod, || s.edges.reset_filters());
                        ui.checkbox(&mut s.edges.show_intra, "1st-shell intra-chain contacts");
                        ui.checkbox(&mut s.edges.show_cross, "Cross-interface contacts");
                        ui.separator();
                        ui.label(RichText::new("Interaction types").strong());
                        ui.checkbox(&mut s.edges.show_covalent, "Covalent bonds");
                        ui.checkbox(&mut s.edges.show_hbond, "Hydrogen bonds");
                        ui.checkbox(&mut s.edges.show_salt_bridge, "Salt bridges");
                        ui.checkbox(&mut s.edges.show_clash, "Steric clashes");
                        ui.checkbox(&mut s.edges.show_vdw, "van der Waals contacts");
                    });
                    let chan_mod = !s.edges.channels_default();
                    named_popover(ui, "if_edges_chan", "Channels", chan_mod, |ui| {
                        popover_header(ui, false, "Contact edge channels", chan_mod, || s.edges.reset_channels());
                        color_channel_control(
                            ui,
                            "if_edge_color_lens",
                            "Edge color",
                            &mut s.edges.color,
                            EdgeColorLens::Fixed,
                        );
                        scalar_channel_control(
                            ui,
                            "if_edge_width_lens",
                            "Edge width",
                            &mut s.edges.width,
                            EdgeWidthLens::Fixed,
                            EDGE_WIDTH_RANGE,
                            "px",
                        );
                    });
                });
            }
        });

        // ── Residue territories ─────────────────────────────────────────────────
        let mut contour_changed = false;
        let mut contour_reset = false;
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.territories.visible, RichText::new("Residue territories").strong());
            if s.territories.visible {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    presentation_popover(
                        ui,
                        "if_terr_pres",
                        "Territory presentation",
                        &mut s.territories.presentation,
                        &TerritoryLayerStyle::default_presentation(),
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
                    let geom_mod = !ds.is_default_draw_contour() || (s.contour_sheet_blend - 0.90).abs() > 1e-3;
                    named_popover(ui, "if_terr_geom", "Geometry", geom_mod, |ui| {
                        popover_header(ui, false, "Territory geometry & drape", geom_mod, || {
                            contour_reset = true;
                            s.contour_sheet_blend = 0.90;
                        });
                        ui.horizontal(|ui| {
                            ui.label("Contour → sheet:")
                                .on_hover_text("0.0 = through atomic centers, 0.99 = 99% onto the interface sheet");
                            ui.add(egui::Slider::new(&mut s.contour_sheet_blend, 0.0..=0.99).fixed_decimals(2));
                        });
                        ui.separator();
                        ui.label(RichText::new("Contour meshing").strong());
                        if param_group_controls(ui, &mut ds.draw_contour, None) {
                            contour_changed = true;
                        }
                    });
                    let chan_mod = s.territories.fill != TerritoryLayerStyle::default().fill
                        || s.territories.stroke_width != TerritoryLayerStyle::default().stroke_width;
                    named_popover(ui, "if_terr_chan", "Channels", chan_mod, |ui| {
                        popover_header(ui, false, "Territory channels", chan_mod, || {
                            let d = TerritoryLayerStyle::default();
                            s.territories.fill = d.fill;
                            s.territories.stroke_width = d.stroke_width;
                        });
                        color_channel_control(
                            ui,
                            "if_terr_color_lens",
                            "Territory fill color",
                            &mut s.territories.fill,
                            TerritoryColorLens::Fixed,
                        );
                        scalar_channel_control(
                            ui,
                            "if_terr_stroke_lens",
                            "Territory border width",
                            &mut s.territories.stroke_width,
                            TerritoryStrokeLens::Fixed,
                            EDGE_WIDTH_RANGE,
                            "px",
                        );
                    });
                });
            }
        });
        if contour_reset {
            ds.reset_draw_contour();
        } else if contour_changed {
            ds.schedule_contour_remesh(now, CONTOUR_REMESH_DEBOUNCE_S);
        }

        // ── Territory labels (top-level) ────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.territories.labels.enabled, RichText::new("Territory labels").strong());
            if s.territories.labels.enabled {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mod_lbl = s.territories.labels != LabelBuilder::default_territory();
                    named_popover(ui, "if_terr_labels_pop", "Customize", mod_lbl, |ui| {
                        label_builder_control(
                            ui,
                            "if_territory_labels",
                            "Territory labels",
                            &mut s.territories.labels,
                            LabelBuilder::default_territory(),
                            false,
                        );
                    });
                });
            }
        });

        // ── Atom labels (top-level) ─────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.atoms.labels.enabled, RichText::new("Atom labels").strong());
            if s.atoms.labels.enabled {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mod_lbl = s.atoms.labels != LabelBuilder::default_atom();
                    named_popover(ui, "if_atom_labels_pop", "Customize", mod_lbl, |ui| {
                        label_builder_control(
                            ui,
                            "if_atom_labels",
                            "Atom labels",
                            &mut s.atoms.labels,
                            LabelBuilder::default_atom(),
                            true,
                        );
                    });
                });
            }
        });

        ui.separator();
        ui.label("Mobility column interpretation:");
        choice_combo_short(ui, "if_bfactor_mode", &mut s.bfactor_mode);
    });

    section(ui, "4. Group visibility", |ui| {
        let name_a = RichText::new(ds.group_name(InterfaceSide::SideA)).color(side_a_color);
        let name_b = RichText::new(ds.group_name(InterfaceSide::SideB)).color(side_b_color);
        Grid::new("carto_group_visibility_grid")
            .num_columns(3)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                group_visibility_row(ui, name_a, &mut s.side_a_alpha);
                group_visibility_row(ui, name_b, &mut s.side_b_alpha);
            });
        // A pure presentation choice — doesn't touch `InterfacePartition`'s actual chain-to-side assignment.
        let on_top_name = ds.group_name(if s.side_a_on_top { InterfaceSide::SideA } else { InterfaceSide::SideB });
        ui.checkbox(&mut s.side_a_on_top, format!("{on_top_name} draws on top"));

        ui.separator();
        ui.checkbox(&mut s.show_backbone, "3D Cα backbone ribbons");
    });
}

fn group_visibility_row(ui: &mut Ui, label: RichText, alpha: &mut Tween) {
    ui.label(label);
    bare_slider(ui, alpha, UNIT_RANGE);
    let mut visible = alpha.target > 0.01;
    if ui.checkbox(&mut visible, "").on_hover_text("Toggle group visibility").changed() {
        alpha.animate_to(if visible { 1.0 } else { 0.0 });
    }
    ui.end_row();
}

/// Composable label builder inside a layer customization popover, with reset-to-default.
fn label_builder_control(
    ui: &mut Ui,
    id_salt: &str,
    title: &str,
    builder: &mut LabelBuilder,
    default_builder: LabelBuilder,
    is_atom: bool,
) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut builder.enabled, RichText::new(title).strong());
        if *builder != default_builder && ui.small_button("Reset").on_hover_text("Reset labels to default").clicked() {
            *builder = default_builder;
        }
    });
    if builder.enabled {
        ui.indent(id_salt, |ui| {
            ui.label("Residue name:");
            choice_combo(ui, &format!("{id_salt}_res_name"), &mut builder.res_name_style);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut builder.show_res_num, "Residue #");
                ui.checkbox(&mut builder.show_chain, "Chain ID");
                if is_atom {
                    ui.checkbox(&mut builder.show_atom_name, "Atom name");
                } else {
                    ui.checkbox(&mut builder.show_sec_struct, "Secondary structure");
                }
            });
            ui.label("Data value channel:");
            choice_combo(ui, &format!("{id_salt}_value_ch"), &mut builder.value_channel);
            ui.label("Position:");
            choice_combo(ui, &format!("{id_salt}_position"), &mut builder.position);
        });
    }
}
