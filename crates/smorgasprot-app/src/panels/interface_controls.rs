//! Interface tab controls: flatten and butterfly morphs, lenses, and per-side layers.

use super::widgets::{alpha_slider, choice_combo, section, tween_presets, tween_slider};
use crate::dataset::ViewerDataset;
use crate::state::{ColorLens, UiState};
use crate::style::palette;
use egui::{RichText, Ui};
use smorgasprot_core::InterfaceSide;

/// Side B opacity below which the Open-Book preset raises it to fully opaque.
const OPEN_BOOK_MIN_B_ALPHA: f32 = 0.85;

/// Renders the Interface tab's side panel.
pub fn show(ui: &mut Ui, ds: &ViewerDataset, state: &mut UiState) {
    ui.heading("🗺️ 2D/3D Interface Cartography");
    ui.add_space(4.0);
    let s = &mut state.interface;

    section(ui, "1. 3D ↔ 2D Flattening (t)", |ui| {
        let t = s.flatten.value;
        let label = if t < 0.05 { "3D" } else if t > 0.95 { "2D Map" } else { "Bas-Relief" };
        tween_slider(ui, &mut s.flatten, label);
        tween_presets(ui, &mut s.flatten, &[("3D (0.0)", 0.0), ("Bas-Relief (0.5)", 0.5), ("2D Map (1.0)", 1.0)]);
        ui.checkbox(&mut s.use_relaxed_uv, "Use Spring-Relaxed 2D (vs. Tangent Plane)");
    });

    section(ui, "2. Overlay ↔ Open-Book Butterfly (θ)", |ui| {
        let t = s.book.value;
        let label = if t < 0.05 { "Overlay".to_owned() } else { format!("{}°", (t * 180.0).round() as i32) };
        tween_slider(ui, &mut s.book, label);
        let preset = tween_presets(ui, &mut s.book, &[("Overlay (0°)", 0.0), ("90°", 0.5), ("Open-Book (180°)", 1.0)]);
        // Opening the book exposes side B, so make sure it is visible.
        if preset == Some(1.0) && s.side_b_alpha < OPEN_BOOK_MIN_B_ALPHA {
            s.side_b_alpha = 1.0;
        }
    });

    section(ui, "3. Visual Lenses & Sizing", |ui| {
        ui.label("Color Lens:");
        choice_combo(ui, "if_color_lens", &mut s.lens);
        if s.lens == ColorLens::Bfactor {
            ui.label("B-Factor Mode:");
            choice_combo(ui, "if_bfactor_mode", &mut s.bfactor_mode);
        }
        ui.label("Atom Size:");
        choice_combo(ui, "if_size_mode", &mut s.size_mode);
    });

    section(ui, "4. Group Opacity & Visibility", |ui| {
        let name_a = RichText::new(format!("{}:", ds.group_name(InterfaceSide::SideA))).color(palette::SIDE_A_LIGHT);
        let name_b = RichText::new(format!("{}:", ds.group_name(InterfaceSide::SideB))).color(palette::SIDE_B_LIGHT);
        alpha_slider(ui, name_a, &mut s.side_a_alpha);
        alpha_slider(ui, name_b, &mut s.side_b_alpha);
        ui.horizontal(|ui| {
            if ui.button("Solo A").clicked() {
                s.solo(InterfaceSide::SideA);
            }
            if ui.button("Solo B").clicked() {
                s.solo(InterfaceSide::SideB);
            }
            if ui.button("Both").clicked() {
                s.show_both();
            }
        });

        ui.separator();
        ui.checkbox(&mut s.show_territories, "Residue Territory Contours");
        ui.checkbox(&mut s.show_intra_edges, "1st-Shell Intra-Chain Springs");
        ui.checkbox(&mut s.show_cross_arcs, "Cross-Interface Contact Arcs");
        ui.checkbox(&mut s.show_backbone, "3D Cα Backbone Ribbons");
    });
}
