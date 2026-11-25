//! Garnish workspace panel: PyMOL/ChimeraX-style representation calibration with dedicated Channels and Presentation popovers.

use super::widgets::{
    UNIT_RANGE, choice_combo, choice_combo_short, color_channel_control, named_popover, param_group_controls,
    popover_header, presentation_popover, scalar_channel_control, section, slider, sphere_popovers,
};
use crate::dataset::ViewerDataset;
use crate::state::{
    AtomSizeMode, BallStickParams, BallStickQuality, BallStickRepStyle, BondColorMode, CartoonParams, CartoonQuality,
    CartoonRepStyle, ChainColorLens, ColorLens, PresentationCaps, RibbonRepStyle, RibbonStyle, SurfaceRepStyle,
    UiState,
};
use egui::{ComboBox, RichText, Slider, Ui};
use smorgasprot_core::surface::SurfaceParams;

/// Pause after the last surface parameter edit before triggering a full 3D remesh (s).
const SURFACE_REMESH_DEBOUNCE_S: f64 = 0.2;

/// Renders the Garnish tab's side panel.
pub fn show(ui: &mut Ui, ds: &mut ViewerDataset, state: &mut UiState) {
    ui.heading("🌿 Molecular Garnish");
    ui.weak("Styles calibrated here automatically apply in Slice mode.");
    ui.checkbox(&mut state.garnish.sync_with_slice, "Sync with Slice")
        .on_hover_text("Apply Slice camera, explode offsets, and layer visibility/alphas in Garnish");
    ui.checkbox(&mut state.garnish.show_interface, "Show interface")
        .on_hover_text("Display Slice's interface boundary and interface spheres in Garnish");
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

    let sync = state.garnish.sync_with_slice;
    let s = &mut state.slice;
    let g = &mut state.garnish;
    section(ui, "2. Complex representations", |ui| {
        // Molecular Surface representation
        let surf_vis = if sync { &mut s.show_surface } else { &mut g.surface_visible };
        let surf_alpha = if sync { &mut s.surface_alpha } else { &mut g.surface_alpha };
        ui.horizontal(|ui| {
            ui.checkbox(surf_vis, RichText::new("Surface mesh").strong());
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
        if *surf_vis {
            slider(ui, "Alpha:", surf_alpha, UNIT_RANGE);
        }

        ui.separator();

        // Cα Backbone Ribbon representation
        let rib_vis = if sync { &mut s.show_ribbon } else { &mut g.ribbon_visible };
        let rib_alpha = if sync { &mut s.ribbon_alpha } else { &mut g.ribbon_alpha };
        ui.horizontal(|ui| {
            ui.checkbox(rib_vis, RichText::new("Ribbon").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_3d = g.ribbon.presentation.quality != RibbonStyle::Capsule2D;
                presentation_popover(
                    ui,
                    "gr_ribbon_pres",
                    "Ribbon presentation",
                    &mut g.ribbon.presentation,
                    &RibbonRepStyle::default_presentation(),
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
                        choice_combo(ui, "gr_ribbon_quality", &mut pres.quality);
                    },
                );
                let chan_mod = !g.ribbon.is_default_channels();
                named_popover(ui, "gr_ribbon_chan", "Channels", chan_mod, |ui| {
                    popover_header(ui, false, "Ribbon channels", chan_mod, || g.ribbon.reset_channels());
                    color_channel_control(
                        ui,
                        "gr_ribbon_color_lens",
                        "Ribbon color",
                        &mut g.ribbon.color,
                        ChainColorLens::Fixed,
                    );
                    scalar_channel_control(
                        ui,
                        "gr_ribbon_width",
                        "Ribbon width",
                        &mut g.ribbon.width,
                        AtomSizeMode::Fixed,
                        0.1..=2.0,
                        "Å",
                    );
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Width scale:").strong());
                        ui.add(Slider::new(&mut g.ribbon.width_scale, 0.3..=2.5).step_by(0.05).text("×"));
                    });
                });
            });
        });
        if *rib_vis {
            slider(ui, "Alpha:", rib_alpha, UNIT_RANGE);
        }

        ui.separator();

        // DSSP Helix–Sheet–Loop Cartoon representation
        let cart_vis = if sync { &mut s.show_cartoon } else { &mut g.cartoon_visible };
        let cart_alpha = if sync { &mut s.cartoon_alpha } else { &mut g.cartoon_alpha };
        ui.horizontal(|ui| {
            ui.checkbox(cart_vis, RichText::new("Cartoon (DSSP)").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let params_mod = g.cartoon.params != CartoonParams::default();
                let reset_params = presentation_popover(
                    ui,
                    "gr_cartoon_pres",
                    "Cartoon presentation",
                    &mut g.cartoon.presentation,
                    &CartoonRepStyle::default_presentation(),
                    PresentationCaps {
                        shading: true,
                        culling: true,
                        border: true,
                        wireframe: true,
                        fill_alpha: true,
                    },
                    params_mod,
                    |ui, pres| {
                        ui.label(RichText::new("Complexity / Geometry").strong());
                        choice_combo(ui, "gr_cartoon_quality", &mut pres.quality);
                        ui.separator();
                        ui.label(RichText::new("Helix, Sheet & Loop").strong());
                        let p = &mut g.cartoon.params;
                        ui.add(Slider::new(&mut p.helix_radius_a, 0.8..=3.0).step_by(0.05).text("Helix radius (Å)"));
                        if pres.quality == CartoonQuality::Detailed {
                            ui.add(Slider::new(&mut p.helix_smoothing, 0.0..=1.0).step_by(0.05).text("Helix smoothing"));
                            ui.checkbox(&mut p.helix_arrow, "Helix arrowhead");
                        }
                        ui.add(Slider::new(&mut p.sheet_width_a, 1.0..=3.5).step_by(0.05).text("Sheet width (Å)"));
                        ui.add(Slider::new(&mut p.sheet_thickness_a, 0.2..=1.0).step_by(0.05).text("Sheet thickness (Å)"));
                        if pres.quality == CartoonQuality::Detailed {
                            ui.checkbox(&mut p.sheet_zigzag, "Sheet zig-zag pleat");
                            ui.add(Slider::new(&mut p.sheet_arrow_scale, 1.0..=2.2).step_by(0.05).text("Sheet arrow scale"));
                        }
                        ui.add(Slider::new(&mut p.loop_radius_a, 0.1..=0.8).step_by(0.02).text("Loop radius (Å)"));
                        ui.add(Slider::new(&mut p.spline_subdiv, 1..=6).text("Spline subdiv"));
                        ui.separator();
                    },
                );
                if reset_params {
                    g.cartoon.params = CartoonParams::default();
                }
                let chan_mod = !g.cartoon.color.is_default();
                named_popover(ui, "gr_cartoon_chan", "Channels", chan_mod, |ui| {
                    popover_header(ui, false, "Cartoon channels", chan_mod, || g.cartoon.color.reset());
                    color_channel_control(
                        ui,
                        "gr_cartoon_color_lens",
                        "Cartoon color",
                        &mut g.cartoon.color,
                        ChainColorLens::Fixed,
                    );
                });
            });
        });
        if *cart_vis {
            slider(ui, "Alpha:", cart_alpha, UNIT_RANGE);
        }

        ui.separator();

        // Covalent Ball-and-Stick representation
        let bs_vis = if sync { &mut s.show_ball_stick } else { &mut g.ball_stick_visible };
        let bs_alpha = if sync { &mut s.ball_stick_alpha } else { &mut g.ball_stick_alpha };
        ui.horizontal(|ui| {
            ui.checkbox(bs_vis, RichText::new("Ball-and-stick").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_3d_mesh = g.ball_stick.presentation.quality != BallStickQuality::Flat2D;
                let has_wireframe = g.ball_stick.presentation.quality.geodesic_preset().is_some();
                let params_mod = g.ball_stick.params != BallStickParams::default();
                let reset_params = presentation_popover(
                    ui,
                    "gr_bs_pres",
                    "Ball-and-stick presentation",
                    &mut g.ball_stick.presentation,
                    &BallStickRepStyle::default_presentation(),
                    PresentationCaps {
                        shading: is_3d_mesh,
                        culling: is_3d_mesh,
                        border: true,
                        wireframe: has_wireframe,
                        fill_alpha: true,
                    },
                    params_mod,
                    |ui, pres| {
                        ui.label(RichText::new("Target & Complexity").strong());
                        choice_combo(ui, "gr_bs_scope", &mut g.ball_stick.params.scope);
                        choice_combo(ui, "gr_bs_quality", &mut pres.quality);
                        ui.separator();
                        ui.label(RichText::new("Bonds & Multiplicity").strong());
                        let p = &mut g.ball_stick.params;
                        ui.add(Slider::new(&mut p.bond_radius_a, 0.04..=0.45).step_by(0.01).text("Bond radius (Å)"));
                        ui.label("Bond coloring:");
                        choice_combo(ui, "gr_bs_bond_col_mode", &mut p.bond_color_mode);
                        if p.bond_color_mode == BondColorMode::Uniform {
                            ui.horizontal(|ui| {
                                ui.label("Bond color:");
                                ui.color_edit_button_srgba(&mut p.bond_color);
                            });
                        }
                        ui.checkbox(&mut p.show_double_bonds, "Distinct double/triple bonds");
                        ui.checkbox(&mut p.show_aromatic_rings, "Distinct aromatic rings");
                        ui.separator();
                    },
                );
                if reset_params {
                    g.ball_stick.params = BallStickParams::default();
                }
                let chan_mod = !g.ball_stick.is_default_channels();
                named_popover(ui, "gr_bs_chan", "Channels", chan_mod, |ui| {
                    popover_header(ui, false, "Ball-and-stick channels", chan_mod, || g.ball_stick.reset_channels());
                    color_channel_control(
                        ui,
                        "gr_bs_atom_color",
                        "Atom color",
                        &mut g.ball_stick.atom_color,
                        ColorLens::Fixed,
                    );
                    scalar_channel_control(
                        ui,
                        "gr_bs_atom_scale",
                        "Atom size",
                        &mut g.ball_stick.atom_scale,
                        AtomSizeMode::Fixed,
                        0.10..=0.80,
                        "× vdW",
                    );
                });
            });
        });
        if *bs_vis {
            slider(ui, "Alpha:", bs_alpha, UNIT_RANGE);
        }

        ui.separator();

        // Atom Spheres representation (all complex atoms: proteins, ligands, ions)
        let sph_vis = if sync { &mut s.show_spheres } else { &mut g.spheres_visible };
        let sph_alpha = if sync { &mut s.spheres_alpha } else { &mut g.spheres_alpha };
        ui.horizontal(|ui| {
            ui.checkbox(sph_vis, RichText::new("Atom spheres").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                sphere_popovers(ui, "gr_spheres", "Atom spheres", &mut g.spheres);
            });
        });
        if *sph_vis {
            slider(ui, "Alpha:", sph_alpha, UNIT_RANGE);
        }
    });

    section(ui, "3. Data interpretation", |ui| {
        ui.label("Mobility column interpretation:");
        choice_combo_short(ui, "gr_bfactor_mode", &mut g.bfactor_mode);
    });
}
