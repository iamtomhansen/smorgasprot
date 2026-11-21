//! Right inspector sidebar: selected residue/atom card, cross-interface contacts, diagnostics, and legends.

use crate::dataset::ViewerDataset;
use crate::state::{BFactorMode, ColorLens};
use egui::{vec2, Color32, Sense, Ui};
use smorgasprot_core::{EdgeKind, InterfaceSide};

pub fn render_right_panel(
    ui: &mut Ui,
    ds: &ViewerDataset,
    lens: ColorLens,
    bfactor_mode: BFactorMode,
    hovered_atom_idx: Option<usize>,
    selected_territory_idx: Option<usize>,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("🔍 Inspector & Diagnostics");
        ui.add_space(4.0);

        // 1. Selected Residue / Hovered Atom Card
        ui.group(|ui| {
            ui.label(egui::RichText::new("Selected Residue / Atom").strong().color(Color32::from_rgb(14, 165, 233)));

            let active_terr = selected_territory_idx
                .and_then(|t_idx| ds.map.territories.get(t_idx))
                .or_else(|| {
                    hovered_atom_idx
                        .and_then(|a_idx| ds.map.nodes.get(a_idx))
                        .and_then(|n| ds.map.territories.get(n.territory.0 as usize))
                });

            if let Some(terr) = active_terr {
                let side_name = if terr.role == InterfaceSide::SideA { &ds.group_a_name } else { &ds.group_b_name };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&terr.label_top).strong().size(15.0));
                    ui.label(format!("({side_name})"));
                });
                ui.label(format!("Chain {} • {} • {} interfacial atoms", terr.chain_id, terr.label_bottom, terr.patch_atoms.len()));
                ui.label(format!("Buried ΔSASA: {:.1} Å²", terr.buried_sasa_a2));

                if terr.has_unsatisfied_polar {
                    ui.colored_label(Color32::from_rgb(245, 158, 11), "⚠️ Buried polar atom with 0 satisfied H-bonds/salt bridges.");
                }

                if let Some(a_idx) = hovered_atom_idx {
                    if let (Some(node), Some(meta)) = (ds.map.nodes.get(a_idx), ds.atom_meta.get(a_idx)) {
                        ui.separator();
                        ui.label(egui::RichText::new(format!("Atom: {} ({:?})", meta.atom_name, meta.element)).strong());
                        ui.label(format!("Buried: {:.1} Å² (Unbound {:.1} → Bound {:.1} Å²)",
                            node.buried_sasa_a2, node.free_sasa_a2, node.complex_sasa_a2));
                        let b_label = if bfactor_mode == BFactorMode::AlphaFold {
                            format!("pLDDT: {:.1}", node.b_factor)
                        } else {
                            format!("B-Factor: {:.1} Å²", node.b_factor)
                        };
                        ui.label(b_label);
                        ui.label(format!("2D Relaxed (u, v): ({:.2}, {:.2})", node.pos_uv[0], node.pos_uv[1]));
                    }
                }

                ui.separator();
                ui.label(egui::RichText::new("Cross-Interface Contacts:").strong());
                let cross_contacts: Vec<_> = ds.map.edges.iter().filter(|e| e.is_cross_interface).collect();
                if cross_contacts.is_empty() {
                    ui.label("No direct cross-contacts (rim residue).");
                } else {
                    for c in cross_contacts.iter().take(6) {
                        let a_meta = &ds.atom_meta[c.a.0 as usize];
                        let b_meta = &ds.atom_meta[c.b.0 as usize];
                        let tag = match c.kind {
                            EdgeKind::Clash => format!("Clash (-{:.2} Å)", c.overlap_a),
                            _ => format!("{:?} ({:.2} Å)", c.kind, c.dist_3d_a),
                        };
                        ui.horizontal(|ui| {
                            ui.label(format!("{} ↔ {}", a_meta.atom_name, b_meta.atom_name));
                            ui.colored_label(Color32::from_rgb(14, 165, 233), tag);
                        });
                    }
                }
            } else {
                ui.label("Hover or click any atom or residue territory to inspect.");
            }
        });

        ui.add_space(6.0);

        // 2. Sandwich Diagnostics
        ui.group(|ui| {
            ui.label(egui::RichText::new("Interface Metrics & Diagnostics").strong().color(Color32::from_rgb(14, 165, 233)));
            let s = &ds.stats;
            ui.label(format!("{} Atoms/Residues: {} / {}", ds.group_a_name, s.side_a_atoms, s.side_a_territories));
            ui.label(format!("{} Atoms/Residues: {} / {}", ds.group_b_name, s.side_b_atoms, s.side_b_territories));
            ui.separator();
            ui.label(format!("Edge RMSE (Raw → Relaxed): {:.2} → {:.2} Å", s.edge_rmse_before_a, s.edge_rmse_after_a));
            ui.label(format!("2D Overlaps (Raw → Relaxed): {} → {}", s.overlaps_before, s.overlaps_after));
            ui.colored_label(Color32::from_rgb(6, 182, 212), format!("Sub-Solvent Void Volume: {:.1} Å³", ds.geometry.void_volume_a3));
        });

        ui.add_space(6.0);

        // 3. Color Legend
        ui.group(|ui| {
            ui.label(egui::RichText::new("Legend").strong().color(Color32::from_rgb(14, 165, 233)));
            match lens {
                ColorLens::Interactions => {
                    render_legend_item(ui, Color32::from_rgb(242, 20, 20), "VdW Clash");
                    render_legend_item(ui, Color32::from_rgb(37, 99, 235), "H-Bond");
                    render_legend_item(ui, Color32::from_rgb(157, 20, 255), "Salt Bridge");
                    render_legend_item(ui, Color32::from_rgb(14, 165, 233), &ds.group_a_name);
                    render_legend_item(ui, Color32::from_rgb(251, 146, 60), &ds.group_b_name);
                }
                ColorLens::Chemistry => {
                    render_legend_item(ui, Color32::from_rgb(148, 163, 184), "Aliphatic");
                    render_legend_item(ui, Color32::from_rgb(245, 158, 11), "Aromatic");
                    render_legend_item(ui, Color32::from_rgb(6, 182, 212), "Polar");
                    render_legend_item(ui, Color32::from_rgb(59, 130, 246), "Positive");
                    render_legend_item(ui, Color32::from_rgb(239, 68, 68), "Negative");
                    render_legend_item(ui, Color32::from_rgb(168, 85, 247), "Gly / Pro");
                }
                ColorLens::ModernCpk => {
                    render_legend_item(ui, Color32::from_rgb(71, 85, 105), "Carbon (C)");
                    render_legend_item(ui, Color32::from_rgb(37, 99, 235), "Nitrogen (N)");
                    render_legend_item(ui, Color32::from_rgb(220, 38, 38), "Oxygen (O)");
                    render_legend_item(ui, Color32::from_rgb(217, 119, 6), "Sulfur (S)");
                }
                ColorLens::Bfactor => {
                    if bfactor_mode == BFactorMode::AlphaFold {
                        render_legend_item(ui, Color32::from_rgb(0, 83, 214), "Very High (>90)");
                        render_legend_item(ui, Color32::from_rgb(101, 203, 243), "Confident (70-90)");
                        render_legend_item(ui, Color32::from_rgb(255, 219, 19), "Low (50-70)");
                        render_legend_item(ui, Color32::from_rgb(255, 125, 69), "Very Low (<50)");
                    } else {
                        render_legend_item(ui, Color32::from_rgb(0, 83, 214), "Rigid (≤30)");
                        render_legend_item(ui, Color32::from_rgb(101, 203, 243), "Ordered (30-50)");
                        render_legend_item(ui, Color32::from_rgb(255, 219, 19), "Flexible (50-70)");
                        render_legend_item(ui, Color32::from_rgb(255, 125, 69), "Labile (>70)");
                    }
                }
            }
        });
    });
}

fn render_legend_item(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.label(label);
    });
}
