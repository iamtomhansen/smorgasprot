//! Right-hand inspector: residue and atom cards, cross-interface contacts, diagnostics, and legend.
//! Read-only; it reflects `Selection` but never changes it.

use super::widgets::{legend_swatch, section};
use crate::dataset::ViewerDataset;
use crate::state::{BFactorMode, UiState};
use crate::style::{lenses, palette};
use egui::{RichText, Ui};
use smorgasprot_core::{AtomInteraction, AtomNode, EdgeKind, InterfaceSide, ResidueTerritory};

/// Contacts listed before collapsing the rest into "+N more".
const MAX_LISTED_CONTACTS: usize = 8;

/// Renders the inspector panel contents.
pub fn show(ui: &mut Ui, ds: &ViewerDataset, state: &UiState) {
    ui.heading("🔍 Inspector & Diagnostics");
    ui.add_space(4.0);

    let hovered = state.selection.hovered_atom.and_then(|idx| ds.node(idx));
    // A clicked residue wins; otherwise follow the hovered atom's residue.
    let territory = state
        .selection
        .selected_territory
        .or(hovered.map(|n| n.territory))
        .and_then(|idx| ds.territory(idx));

    section(ui, "Selected Residue / Atom", |ui| match territory {
        Some(terr) => {
            residue_card(ui, ds, terr);
            if let Some(node) = hovered {
                ui.separator();
                atom_card(ui, ds, node, state.interface.bfactor_mode);
            }
            ui.separator();
            contacts_list(ui, ds, terr);
        }
        None => {
            ui.label("Hover or click any atom or residue territory to inspect.");
        }
    });

    section(ui, "Interface Metrics & Diagnostics", |ui| diagnostics(ui, ds));

    let s = &state.interface;
    section(ui, "Legend", |ui| {
        let a = ds.group_name(InterfaceSide::SideA);
        let b = ds.group_name(InterfaceSide::SideB);
        for (color, label) in lenses::legend(s.lens, s.bfactor_mode, a, b) {
            legend_swatch(ui, color, &label);
        }
    });
}

fn residue_card(ui: &mut Ui, ds: &ViewerDataset, terr: &ResidueTerritory) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(&terr.label_top).strong().size(15.0));
        ui.label(RichText::new(format!("({})", ds.group_name(terr.role))).color(palette::side_light(terr.role)));
    });
    ui.label(format!("Chain {} • {} • {} interfacial atoms", terr.chain_id, terr.label_bottom, terr.patch_atoms.len()));
    ui.label(format!("Buried ΔSASA: {:.1} Å²", terr.buried_sasa_a2));
    if terr.has_unsatisfied_polar {
        ui.colored_label(palette::WARNING, "⚠️ Buried polar atom with 0 satisfied H-bonds/salt bridges.");
    }
}

fn atom_card(ui: &mut Ui, ds: &ViewerDataset, node: &AtomNode, mode: BFactorMode) {
    let Some(meta) = ds.meta(node.idx) else { return };
    ui.label(RichText::new(format!("Atom: {} ({:?})", meta.atom_name, meta.element)).strong());
    ui.label(format!(
        "Buried: {:.1} Å² (Unbound {:.1} → Bound {:.1} Å²)",
        node.buried_sasa_a2, node.free_sasa_a2, node.complex_sasa_a2
    ));
    ui.label(match mode {
        BFactorMode::AlphaFold => format!("pLDDT: {:.1}", node.b_factor),
        BFactorMode::Crystallographic => format!("B-Factor: {:.1} Å²", node.b_factor),
    });
    ui.label(format!("2D Relaxed (u, v): ({:.2}, {:.2})", node.pos_uv[0], node.pos_uv[1]));
}

/// Lists cross-interface contacts made by atoms of `terr`, from that residue's point of view.
fn contacts_list(ui: &mut Ui, ds: &ViewerDataset, terr: &ResidueTerritory) {
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
            ui.colored_label(lenses::contact_color(contact.kind), format!("{} ({measure})", lenses::contact_label(contact.kind)));
        });
    }
    if contacts.len() > MAX_LISTED_CONTACTS {
        ui.weak(format!("+{} more", contacts.len() - MAX_LISTED_CONTACTS));
    }
}

fn diagnostics(ui: &mut Ui, ds: &ViewerDataset) {
    let s = &ds.stats;
    ui.label(format!("{} Atoms/Residues: {} / {}", ds.group_a_name, s.side_a_atoms, s.side_a_territories));
    ui.label(format!("{} Atoms/Residues: {} / {}", ds.group_b_name, s.side_b_atoms, s.side_b_territories));
    ui.separator();
    ui.label(format!("Edge RMSE (Raw → Relaxed): {:.2} → {:.2} Å", s.edge_rmse_before_a, s.edge_rmse_after_a));
    ui.label(format!("2D Overlaps (Raw → Relaxed): {} → {}", s.overlaps_before, s.overlaps_after));
    ui.colored_label(palette::VOID, format!("Sub-Solvent Void Volume: {:.1} Å³", ds.geometry.void_volume_a3));
}
