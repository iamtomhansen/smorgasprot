//! Garnish workspace panel: PyMOL/ChimeraX-style representation calibration with dedicated Channels and Presentation popovers.

use super::widgets::{
    UNIT_RANGE, choice_combo, choice_combo_short, color_channel_control, named_popover, param_group_controls,
    popover_header, presentation_popover, section, slider, sphere_popovers,
};
use crate::dataset::ViewerDataset;
use crate::state::{
    CartoonRepStyle, CartoonStyle, ChainColorLens, PresentationCaps, SurfaceRepStyle, UiState,
};
use egui::{ComboBox, RichText, Slider, Ui};
use smorgasprot_core::surface::SurfaceParams;

/// Pause after the last surface parameter edit before triggering a full 3D remesh (s).
const SURFACE_REMESH_DEBOUNCE_S: f64 = 0.2;

/// Renders the Garnish tab's side panel.
pub fn show(ui: &mut Ui, ds: &mut ViewerDataset, state: &mut UiState) {
    ui.heading("🌿 Molecular Garnish");
    ui.weak("Styles calibrated here automatically apply in Complex mode.");
    ui.add_space(4.0);

    section(ui, "1. Color palette", |ui| {
        let mut chosen = None;
        ui.horizontal(|ui| {
            ui.label("Palette:");
            ComboBox::from_id_salt("garnish_palette_preset")
                .selected_text(&state.palette.name)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for p in &state.palette.available {
                        if ui.selectable_label(p.name == state.palette.name, &p.name).clicked() {
                            chosen = Some(p.name.clone());
                        }
                    }
                });
        });
        if let Some(name) = chosen {
            state.palette.apply_by_name(&name);
        }
    });

    let g = &mut state.garnish;
    section(ui, "2. Complex representations", |ui| {
        // Molecular Surface representation
        ui.horizontal(|ui| {
            ui.checkbox(&mut g.surface_visible, RichText::new("Surface mesh").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let geom_modified = !ds.is_default_culling() || !ds.is_default_draw_mesh();
                let now = ui.input(|i| i.time);
                let reset_geom = presentation_popover(
                    ui,
                    "gr_surface_pres",
                    "Surface presentation",
                    &mut g.surface.presentation,
                    &SurfaceRepStyle::default_presentation(),
                    PresentationCaps {
                        shading: true,
                        culling: false,
                        border: true,
                        wireframe: true,
                        fill_alpha: true,
                    },
                    geom_modified,
                    |ui, _| {
                        ui.label(RichText::new("Geometry & meshing").strong());
                        ui.label("Face culling:");
                        choice_combo(ui, "gr_surface_culling", &mut ds.culling);
                        ui.add_enabled_ui(ds.complex.is_some(), |ui| {
                            ui.label("Resolution preset:");
                            let prev_preset = ds.draw_preset;
                            choice_combo(ui, "gr_surface_preset", &mut ds.draw_preset);
                            let preset_changed = ds.draw_preset != prev_preset;
                            let mut display_defaults = SurfaceParams::display();
                            let params_changed = param_group_controls(
                                ui,
                                &mut ds.draw_mesh,
                                Some(&mut display_defaults),
                            );
                            if preset_changed || params_changed {
                                ds.schedule_draw_surface_remesh(now, SURFACE_REMESH_DEBOUNCE_S);
                            }
                        });
                        ui.separator();
                    },
                );
                if reset_geom {
                    ds.reset_culling();
                    if !ds.is_default_draw_mesh() {
                        ds.reset_draw_mesh();
                        ds.schedule_draw_surface_remesh(now, SURFACE_REMESH_DEBOUNCE_S);
                    }
                }
                let chan_mod = !g.surface.color.is_default();
                named_popover(ui, "gr_surface_chan", "Channels", chan_mod, |ui| {
                    popover_header(ui, false, "Surface channels", chan_mod, || g.surface.color.reset());
                    color_channel_control(
                        ui,
                        "gr_surface_color_lens",
                        "Surface color",
                        &mut g.surface.color,
                        ChainColorLens::Fixed,
                    );
                });
            });
        });
        if g.surface_visible {
            slider(ui, "Alpha:", &mut g.surface_alpha, UNIT_RANGE);
        }

        ui.separator();

        // Cartoon Ribbon representation
        ui.horizontal(|ui| {
            ui.checkbox(&mut g.cartoon_visible, RichText::new("Cartoon ribbon").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_3d = g.cartoon.presentation.quality != CartoonStyle::Capsule2D;
                presentation_popover(
                    ui,
                    "gr_cartoon_pres",
                    "Cartoon presentation",
                    &mut g.cartoon.presentation,
                    &CartoonRepStyle::default_presentation(),
                    PresentationCaps {
                        shading: is_3d,
                        culling: is_3d,
                        border: true,
                        wireframe: is_3d,
                        fill_alpha: true,
                    },
                    false,
                    |ui, pres| {
                        ui.label(RichText::new("Quality / Geometry").strong());
                        choice_combo(ui, "gr_cartoon_quality", &mut pres.quality);
                    },
                );
                let chan_mod = !g.cartoon.is_default_channels();
                named_popover(ui, "gr_cartoon_chan", "Channels", chan_mod, |ui| {
                    popover_header(ui, false, "Cartoon channels", chan_mod, || g.cartoon.reset_channels());
                    color_channel_control(
                        ui,
                        "gr_cartoon_color_lens",
                        "Ribbon color",
                        &mut g.cartoon.color,
                        ChainColorLens::Fixed,
                    );
                    ui.label(RichText::new("Ribbon width scale:").strong());
                    ui.add(Slider::new(&mut g.cartoon.width_scale, 0.3..=2.5).step_by(0.05).text("× scale"));
                });
            });
        });
        if g.cartoon_visible {
            slider(ui, "Alpha:", &mut g.cartoon_alpha, UNIT_RANGE);
        }

        ui.separator();

        // Atom Spheres representation (all complex atoms: proteins, ligands, ions)
        ui.horizontal(|ui| {
            ui.checkbox(&mut g.spheres_visible, RichText::new("Atom spheres").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                sphere_popovers(ui, "gr_spheres", "Atom spheres", &mut g.spheres);
            });
        });
        if g.spheres_visible {
            slider(ui, "Alpha:", &mut g.spheres_alpha, UNIT_RANGE);
        }
    });

    section(ui, "3. Data interpretation", |ui| {
        ui.label("Mobility column interpretation:");
        choice_combo_short(ui, "gr_bfactor_mode", &mut g.bfactor_mode);
    });
}
