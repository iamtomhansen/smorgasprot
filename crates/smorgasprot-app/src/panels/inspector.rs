//! Right-hand inspector: residue and atom cards, cross-interface contacts, diagnostics, and legend.
//! Read-only; it reflects `Selection` and `Palette` but never changes them.

use super::widgets::{legend_swatch, section};
use crate::dataset::ViewerDataset;
use crate::state::{
    BFactorMode, ChainColorLens, ColorLens, InterfaceDisplayMode, LensParams, UiState, WorkspaceTab,
};
use crate::style::{ChromePalette, ContactPalette, InterfaceSidePalette, Palette, lenses};
use egui::{CollapsingHeader, Color32, ComboBox, RichText, Ui};
use smorgasprot_core::{AtomInteraction, AtomNode, EdgeKind, InterfaceSide, ResidueTerritory};

/// Contacts listed before collapsing the rest into "+N more".
const MAX_LISTED_CONTACTS: usize = 8;

/// Renders the right-hand panel contents tailored to the active workspace tab.
pub fn show(ui: &mut Ui, ds: &ViewerDataset, state: &mut UiState) {
    match state.active_tab {
        WorkspaceTab::Cartography => show_cartography_inspector(ui, ds, state),
        WorkspaceTab::Slice => show_slice_inspector(ui, ds, state),
        WorkspaceTab::Garnish => show_garnish_panel(ui, ds, state),
    }
}

fn show_selection_and_diagnostics(ui: &mut Ui, ds: &ViewerDataset, state: &UiState, bfactor_mode: BFactorMode) {
    let pal = &state.palette;
    let hovered = state
        .selection
        .hovered_atom
        .or(state.selection.selected_atom)
        .and_then(|g| ds.node_by_global_atom(g));
    let territory = state
        .selection
        .selected_residue
        .and_then(|r| ds.territory_by_global_residue(r))
        .or_else(|| hovered.and_then(|n| ds.territory(n.territory)));

    section(ui, "Selected Residue / Atom", |ui| match territory {
        Some(terr) => {
            residue_card(ui, ds, terr, &pal.sides, &pal.chrome);
            if let Some(node) = hovered {
                ui.separator();
                atom_card(ui, ds, node, bfactor_mode);
            }
            ui.separator();
            contacts_list(ui, ds, terr, &pal.contacts);
        }
        None => {
            ui.label("Hover or click any atom or residue territory to inspect.");
        }
    });

    section(ui, "Interface Metrics & Diagnostics", |ui| diagnostics(ui, ds, &pal.sides));
}

fn show_cartography_inspector(ui: &mut Ui, ds: &ViewerDataset, state: &UiState) {
    ui.heading("🔍 Inspector & Diagnostics");
    ui.add_space(4.0);
    show_selection_and_diagnostics(ui, ds, state, state.cartography.bfactor_mode);

    let s = &state.cartography;
    section(ui, "Legend", |ui| {
        render_lens_legend(
            ui,
            ds,
            None,
            s.atoms.fill.lens,
            s.atoms.fill.fixed,
            s.bfactor_mode,
            &state.palette.thresholds,
            &state.palette,
        );
    });
}

fn render_interface_legend(
    ui: &mut Ui,
    ds: &ViewerDataset,
    cx: &crate::state::SliceViewSettings,
    bfactor_mode: BFactorMode,
    params: &LensParams,
    pal: &Palette,
) -> bool {
    let mut any = false;
    if cx.interface_spheres_visible && cx.interface_spheres.fill.lens != ColorLens::Off {
        any = true;
        render_lens_legend(
            ui,
            ds,
            Some("Interface spheres"),
            cx.interface_spheres.fill.lens,
            cx.interface_spheres.fill.fixed,
            bfactor_mode,
            params,
            pal,
        );
    }
    if ds.active_interface().is_some() && cx.boundary_layers().iter().any(|(_, b)| b.visible) {
        any = true;
        ui.label(RichText::new("Interface boundary").strong());
        for (mode, layer) in cx.boundary_layers() {
            if !layer.visible {
                continue;
            }
            match mode {
                InterfaceDisplayMode::SmoothedSdf => legend_swatch(ui, pal.sides.sheet, "Smoothed separator sheet"),
                InterfaceDisplayMode::ContactEnvelope => {
                    legend_swatch(ui, pal.sides.envelope_a, &format!("{} envelope", ds.group_name(InterfaceSide::SideA)));
                    legend_swatch(ui, pal.sides.envelope_b, &format!("{} envelope", ds.group_name(InterfaceSide::SideB)));
                }
                InterfaceDisplayMode::NegativeSpace => legend_swatch(ui, pal.sides.void, "Sub-solvent void"),
                InterfaceDisplayMode::CavitiesBuried => {
                    legend_swatch(ui, pal.sides.cavity_buried, "Buried probe cavity")
                }
                InterfaceDisplayMode::CavitiesChannel => {
                    legend_swatch(ui, pal.sides.cavity_channel, "Solvent-connected channel")
                }
            }
        }
    }
    any
}

fn show_slice_inspector(ui: &mut Ui, ds: &ViewerDataset, state: &UiState) {
    ui.heading("🔍 Inspector & Diagnostics");
    ui.add_space(4.0);
    show_selection_and_diagnostics(ui, ds, state, state.garnish.bfactor_mode);

    let cx = &state.slice;
    let g = &state.garnish;
    let pal = &state.palette;
    let params = &pal.thresholds;
    section(ui, "Active Legend", |ui| {
        let mut any = false;
        if cx.show_surface && g.surface.color.lens != ChainColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Surface mesh"),
                g.surface.color.lens.to_color_lens(),
                g.surface.color.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if cx.show_ribbon && g.ribbon.color.lens != ChainColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Ribbon"),
                g.ribbon.color.lens.to_color_lens(),
                g.ribbon.color.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if cx.show_cartoon && g.cartoon.color.lens != ChainColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Cartoon (DSSP)"),
                g.cartoon.color.lens.to_color_lens(),
                g.cartoon.color.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if cx.show_ball_stick && g.ball_stick.atom_color.lens != ColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Ball-and-stick"),
                g.ball_stick.atom_color.lens,
                g.ball_stick.atom_color.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if cx.show_spheres && g.spheres.fill.lens != ColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Complex spheres"),
                g.spheres.fill.lens,
                g.spheres.fill.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        any |= render_interface_legend(ui, ds, cx, g.bfactor_mode, params, pal);
        if !any {
            ui.weak("Enable a representation to see its legend.");
        }
    });
}

fn show_garnish_panel(ui: &mut Ui, ds: &ViewerDataset, state: &mut UiState) {
    ui.heading("🎨 Palette & Legend");
    ui.add_space(4.0);

    let g = &state.garnish;
    let cx = &state.slice;
    let sync = g.sync_with_slice;
    let (surf_vis, rib_vis, cart_vis, bs_vis, sph_vis) = if sync {
        (cx.show_surface, cx.show_ribbon, cx.show_cartoon, cx.show_ball_stick, cx.show_spheres)
    } else {
        (g.surface_visible, g.ribbon_visible, g.cartoon_visible, g.ball_stick_visible, g.spheres_visible)
    };
    let pal = &state.palette;
    let params = &pal.thresholds;
    section(ui, "Active Legend", |ui| {
        let mut any = false;
        if surf_vis && g.surface.color.lens != ChainColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Surface mesh"),
                g.surface.color.lens.to_color_lens(),
                g.surface.color.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if rib_vis && g.ribbon.color.lens != ChainColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Ribbon"),
                g.ribbon.color.lens.to_color_lens(),
                g.ribbon.color.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if cart_vis && g.cartoon.color.lens != ChainColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Cartoon (DSSP)"),
                g.cartoon.color.lens.to_color_lens(),
                g.cartoon.color.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if bs_vis && g.ball_stick.atom_color.lens != ColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Ball-and-stick"),
                g.ball_stick.atom_color.lens,
                g.ball_stick.atom_color.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if sph_vis && g.spheres.fill.lens != ColorLens::Off {
            any = true;
            render_lens_legend(
                ui,
                ds,
                Some("Atom spheres"),
                g.spheres.fill.lens,
                g.spheres.fill.fixed,
                g.bfactor_mode,
                params,
                pal,
            );
        }
        if g.show_interface {
            any |= render_interface_legend(ui, ds, cx, g.bfactor_mode, params, pal);
        }
        if !any {
            ui.weak("Enable a representation to see its legend.");
        }
    });

    section(ui, "Active Palette Editor", |ui| {
        palette_editor(ui, &mut state.palette);
    });
}

#[allow(clippy::too_many_arguments)]
fn render_lens_legend(
    ui: &mut Ui,
    ds: &ViewerDataset,
    heading: Option<&str>,
    lens: ColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
    params: &LensParams,
    pal: &Palette,
) {
    if let Some(title) = heading {
        ui.label(RichText::new(title).strong());
    }
    if lens == ColorLens::ChainIndex && !ds.chains.is_empty() {
        for (i, ch) in ds.chains.iter().enumerate() {
            legend_swatch(ui, pal.categorical(i), &format!("Chain {}", ch.chain_id));
        }
        return;
    }
    let a = ds.group_name(InterfaceSide::SideA);
    let b = ds.group_name(InterfaceSide::SideB);
    for (color, label) in lenses::legend(lens, fixed_color, bfactor_mode, a, b, params, pal) {
        legend_swatch(ui, color, &label);
    }
}

fn color_row(ui: &mut Ui, label: &str, color: &mut Color32) {
    ui.horizontal(|ui| {
        ui.color_edit_button_srgba(color);
        ui.label(label);
    });
}

fn palette_editor(ui: &mut Ui, pal: &mut Palette) {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label("Preset:");
        ComboBox::from_id_salt("garnish_right_palette_preset")
            .selected_text(&pal.name)
            .width((ui.available_width() - 56.0).max(80.0))
            .show_ui(ui, |ui| {
                for p in &pal.available {
                    if ui.selectable_label(p.name == pal.name, &p.name).clicked() {
                        chosen = Some(p.name.clone());
                    }
                }
            });
        if ui.small_button("Reset").on_hover_text("Restore preset colors & thresholds").clicked() {
            chosen = Some(pal.name.clone());
        }
    });
    if let Some(name) = chosen {
        pal.apply_by_name(&name);
    }
    ui.add_space(4.0);

    CollapsingHeader::new("Interface Sides").default_open(true).show(ui, |ui| {
        color_row(ui, "Side A", &mut pal.sides.side_a);
        color_row(ui, "Side B", &mut pal.sides.side_b);
        color_row(ui, "Unassigned / Non-interfacial", &mut pal.sides.unassigned);
        color_row(ui, "Separator Sheet", &mut pal.sides.sheet);
        color_row(ui, "Envelope A", &mut pal.sides.envelope_a);
        color_row(ui, "Envelope B", &mut pal.sides.envelope_b);
        color_row(ui, "Negative Space Void", &mut pal.sides.void);
    });

    CollapsingHeader::new("AA Chemistry").show(ui, |ui| {
        color_row(ui, "Aliphatic", &mut pal.aa_chemistry.aliphatic);
        ui.horizontal(|ui| {
            ui.label("Aliphatic cutoff:");
            ui.add(egui::DragValue::new(&mut pal.thresholds.aliphatic_threshold).range(0.0..=1.0).speed(0.01));
        });
        color_row(ui, "Aromatic", &mut pal.aa_chemistry.aromatic);
        color_row(ui, "Polar", &mut pal.aa_chemistry.polar);
        color_row(ui, "Positive", &mut pal.aa_chemistry.positive);
        color_row(ui, "Negative", &mut pal.aa_chemistry.negative);
        color_row(ui, "Gly / Pro", &mut pal.aa_chemistry.gly_pro);
    });

    CollapsingHeader::new("Interactions").show(ui, |ui| {
        color_row(ui, "VdW Clash", &mut pal.contacts.clash);
        ui.horizontal(|ui| {
            ui.label("Clash overlap:");
            ui.add(egui::DragValue::new(&mut pal.thresholds.clash_threshold_a).range(0.0..=1.0).speed(0.01).suffix(" Å"));
        });
        color_row(ui, "H-Bond", &mut pal.contacts.hbond);
        color_row(ui, "Salt Bridge", &mut pal.contacts.salt_bridge);
        color_row(ui, "Disulfide", &mut pal.contacts.disulfide);
        color_row(ui, "VdW Contact", &mut pal.contacts.contact);
        color_row(ui, "Backbone", &mut pal.contacts.backbone);
    });

    CollapsingHeader::new("Elements (CPK)").show(ui, |ui| {
        color_row(ui, "Carbon (C)", &mut pal.elements.c);
        color_row(ui, "Nitrogen (N)", &mut pal.elements.n);
        color_row(ui, "Oxygen (O)", &mut pal.elements.o);
        color_row(ui, "Sulfur (S)", &mut pal.elements.s);
        color_row(ui, "Phosphorus (P)", &mut pal.elements.p);
        color_row(ui, "Hydrogen (H)", &mut pal.elements.h);
        color_row(ui, "Other / Metals", &mut pal.elements.other);
    });

    CollapsingHeader::new("Mobility (B-Factor / pLDDT)").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for stop in &mut pal.mobility.stops {
                ui.color_edit_button_srgba(stop);
            }
        });
        egui::Grid::new("pal_mobility_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Rigid B-factor max");
            ui.add(egui::DragValue::new(&mut pal.thresholds.bfactor_rigid_max_a2).range(0.0..=200.0).speed(0.5).suffix(" Å²"));
            ui.end_row();
            ui.label("Ordered B-factor max");
            ui.add(egui::DragValue::new(&mut pal.thresholds.bfactor_ordered_max_a2).range(0.0..=200.0).speed(0.5).suffix(" Å²"));
            ui.end_row();
            ui.label("Flexible B-factor max");
            ui.add(egui::DragValue::new(&mut pal.thresholds.bfactor_flexible_max_a2).range(0.0..=200.0).speed(0.5).suffix(" Å²"));
            ui.end_row();
            ui.label("Very high pLDDT min");
            ui.add(egui::DragValue::new(&mut pal.thresholds.plddt_very_high_min).range(0.0..=100.0).speed(0.5));
            ui.end_row();
            ui.label("Confident pLDDT min");
            ui.add(egui::DragValue::new(&mut pal.thresholds.plddt_confident_min).range(0.0..=100.0).speed(0.5));
            ui.end_row();
            ui.label("Low pLDDT min");
            ui.add(egui::DragValue::new(&mut pal.thresholds.plddt_low_min).range(0.0..=100.0).speed(0.5));
            ui.end_row();
        });
    });

    CollapsingHeader::new("Aliphatic Burial Score").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for stop in &mut pal.mobility.stops {
                ui.color_edit_button_srgba(stop);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Aliphatic cutoff:");
            ui.add(egui::DragValue::new(&mut pal.thresholds.aliphatic_threshold).range(0.0..=1.0).speed(0.01));
        });
    });

    CollapsingHeader::new("Buried Interface ΔSASA").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for stop in &mut pal.mobility.stops {
                ui.color_edit_button_srgba(stop);
            }
        });
        egui::Grid::new("pal_sasa_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Atom ΔSASA max");
            ui.add(egui::DragValue::new(&mut pal.thresholds.atom_buried_sasa_max_a2).range(1.0..=100.0).speed(0.5).suffix(" Å²"));
            ui.end_row();
            ui.label("Territory ΔSASA max");
            ui.add(egui::DragValue::new(&mut pal.thresholds.territory_buried_sasa_max_a2).range(10.0..=300.0).speed(1.0).suffix(" Å²"));
            ui.end_row();
        });
    });

    CollapsingHeader::new("Clash Severity").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for stop in &mut pal.charge.stops {
                ui.color_edit_button_srgba(stop);
            }
        });
        egui::Grid::new("pal_clash_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Clash overlap");
            ui.add(egui::DragValue::new(&mut pal.thresholds.clash_threshold_a).range(0.0..=1.0).speed(0.01).suffix(" Å"));
            ui.end_row();
            ui.label("Severe clash max");
            ui.add(egui::DragValue::new(&mut pal.thresholds.clash_severity_max_a).range(0.1..=2.0).speed(0.02).suffix(" Å"));
            ui.end_row();
        });
    });

    CollapsingHeader::new("Categorical Chains").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for col in &mut pal.categorical {
                ui.color_edit_button_srgba(col);
            }
        });
    });
}

fn residue_card(ui: &mut Ui, ds: &ViewerDataset, terr: &ResidueTerritory, sides: &InterfaceSidePalette, chrome: &ChromePalette) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(&terr.label_top).strong().size(15.0));
        ui.label(RichText::new(format!("({})", ds.group_name(terr.side))).color(sides.side_light(terr.side)));
    });
    ui.label(format!("Chain {} • {} • {} interfacial atoms", terr.chain_id, terr.label_bottom, terr.patch_atoms.len()));
    ui.label(format!("Buried ΔSASA: {:.1} Å²", terr.buried_sasa_a2));
    if terr.has_unsatisfied_polar {
        ui.colored_label(chrome.warning, "⚠️ Buried polar atom with 0 satisfied H-bonds/salt bridges.");
    }
}

fn atom_card(ui: &mut Ui, ds: &ViewerDataset, node: &AtomNode, mode: BFactorMode) {
    let Some(meta) = ds.meta(node.idx) else { return };
    ui.label(RichText::new(format!("Atom: #{} {} ({:?})", meta.atom_serial, meta.atom_name, meta.element)).strong());
    ui.label(format!("Residue: {} {} (Chain {})", meta.res_name, meta.res_num, meta.chain_id));
    ui.label(format!(
        "Buried: {:.1} Å² (Unbound {:.1} → Bound {:.1} Å²)",
        node.buried_sasa_a2, node.free_sasa_a2, node.complex_sasa_a2
    ));
    ui.label(match mode {
        BFactorMode::AlphaFold => format!("pLDDT: {:.1} • Occ: {:.2}", node.b_factor, meta.occupancy),
        BFactorMode::Crystallographic => format!("B-Factor: {:.1} Å² • Occ: {:.2}", node.b_factor, meta.occupancy),
    });
    ui.label(format!("2D Relaxed (u, v): ({:.2}, {:.2})", node.pos_uv[0], node.pos_uv[1]));
}

/// Lists cross-interface contacts made by atoms of `terr`, from that residue's point of view.
fn contacts_list(ui: &mut Ui, ds: &ViewerDataset, terr: &ResidueTerritory, contacts_pal: &ContactPalette) {
    ui.label(RichText::new("Cross-Interface Contacts:").strong());
    let contacts: Vec<(&AtomNode, &AtomInteraction)> = terr
        .patch_atoms
        .iter()
        .filter_map(|&idx| ds.node(idx))
        .flat_map(|n| n.interactions.iter().filter(|i| i.is_cross_interface).map(move |i| (n, i)))
        .collect();

    if contacts.is_empty() {
        ui.label("No direct cross-contacts (rim residue).");
        return;
    }
    for &(node, contact) in contacts.iter().take(MAX_LISTED_CONTACTS) {
        let own = ds.meta(node.idx).map_or("?".to_owned(), |m| m.atom_name.clone());
        let partner = ds.meta(contact.partner).map_or("?".to_owned(), |m| m.residue_atom_label());
        let measure = match contact.kind {
            EdgeKind::Clash => format!("-{:.2} Å", contact.overlap_a),
            _ => format!("{:.2} Å", contact.dist_3d_a),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{own} ↔ {partner}"));
            ui.colored_label(
                lenses::contact_color(contact.kind, contacts_pal),
                format!("{} ({measure})", lenses::contact_label(contact.kind)),
            );
        });
    }
    if contacts.len() > MAX_LISTED_CONTACTS {
        ui.weak(format!("+{} more", contacts.len() - MAX_LISTED_CONTACTS));
    }
}

fn diagnostics(ui: &mut Ui, ds: &ViewerDataset, sides: &InterfaceSidePalette) {
    let Some(iface) = ds.active_interface() else {
        let total_res: usize = ds.chains.iter().map(|c| c.residue_count).sum();
        let total_atoms: usize = ds.chains.iter().map(|c| c.atom_count).sum();
        ui.label(format!("Bare Complex: {} chains, {} residues, {} heavy atoms.", ds.chains.len(), total_res, total_atoms));
        return;
    };
    let s = &iface.stats;
    ui.label(format!("{} Atoms/Residues: {} / {}", iface.partition.group_a_name, s.side_a_atoms, s.side_a_territories));
    ui.label(format!("{} Atoms/Residues: {} / {}", iface.partition.group_b_name, s.side_b_atoms, s.side_b_territories));
    ui.separator();
    ui.label(format!("Edge RMSE (Raw → Relaxed): {:.2} → {:.2} Å", s.edge_rmse_before_a, s.edge_rmse_after_a));
    ui.label(format!("2D Overlaps (Raw → Relaxed): {} → {}", s.overlaps_before, s.overlaps_after));
    ui.colored_label(sides.void, format!("Sub-Solvent Void Volume: {:.1} Å³", iface.analysis_void_volume_a3));
    let cav = &iface.draw_geometry.cavities;
    ui.colored_label(
        sides.cavity_buried,
        format!(
            "Probe Cavities (R = {:.2} Å): {} buried ({:.1} Å³), {} channels ({:.1} Å³)",
            ds.draw_cavities.probe_radius_a,
            cav.buried_count,
            cav.buried_volume_a3,
            cav.channel_count,
            cav.channel_volume_a3,
        ),
    );
    ui.weak(format!(
        "Implicit H skin: +{:.2} Å on heavy-atom vdW radii",
        ds.analysis_params().geometry.implicit_h_skin_a
    ));
}
