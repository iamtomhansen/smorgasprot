//! Floating app-wide Preferences window: Rendering backend, View defaults, Analysis defaults, and config actions.

use super::widgets;
use crate::action::Action;
use crate::config::AppConfig;
use crate::state::{RenderBackend, RenderingSettings, TransparencyMode, UiState};
use egui::{CollapsingHeader, ComboBox, Context, RichText, ScrollArea, Ui, Window};
use smorgasprot_core::params::ParamGroup;
use std::path::Path;

const MSAA_CHOICES: &[u16] = &[1, 2, 4, 8];
const PEEL_LAYER_CHOICES: &[u8] = &[2, 4, 6, 8];

/// Renders the Preferences window when `state.panels.preferences` is open.
pub fn show(
    ctx: &Context,
    state: &mut UiState,
    config: &mut AppConfig,
    startup_rendering: &RenderingSettings,
    wgpu_available: bool,
    wboit_supported: bool,
    config_path: Option<&Path>,
    actions: &mut Vec<Action>,
) {
    let mut open = state.panels.preferences;
    Window::new("Preferences")
        .open(&mut open)
        .default_width(420.0)
        .max_height(640.0)
        .show(ctx, |ui| {
            config_toolbar(ui, config_path, actions);
            ScrollArea::vertical().id_salt("preferences_scroll").show(ui, |ui| {
                rendering_section(ui, &mut config.rendering, startup_rendering, wgpu_available, wboit_supported);
                ui.add_space(4.0);
                ui.label(RichText::new("View & Hot-Reload Defaults").strong());
                ui.horizontal(|ui| {
                    let default_bg = crate::state::view_params::default_viewport_bg();
                    ui.label("Light viewport bg:");
                    ui.color_edit_button_srgba(&mut state.view_params.viewport_bg);
                    if state.view_params.viewport_bg != default_bg && ui.small_button("Reset").clicked() {
                        state.view_params.viewport_bg = default_bg;
                    }
                });
                ui.horizontal(|ui| {
                    let default_dark = crate::state::view_params::default_dark_viewport_bg();
                    ui.label("Dark viewport bg:");
                    ui.color_edit_button_srgba(&mut state.view_params.dark_viewport_bg);
                    if state.view_params.dark_viewport_bg != default_dark && ui.small_button("Reset").clicked() {
                        state.view_params.dark_viewport_bg = default_dark;
                    }
                });
                let mut grid_color = state.view_params.grid_color;
                for group in state.view_params.groups_mut() {
                    let key = group.key();
                    let default_open = key == "config";
                    let extra_color = (key == "grid").then_some(&mut grid_color);
                    param_collapsing(ui, "view", group, default_open, extra_color);
                }
                state.view_params.grid_color = grid_color;
                ui.add_space(6.0);
                ui.label(RichText::new("Analysis Defaults (applies on next load)").strong());
                for group in config.analysis.groups_mut() {
                    param_collapsing(ui, "analysis", group, false, None);
                }
            });
        });
    state.panels.preferences = open;
}

fn config_toolbar(ui: &mut Ui, config_path: Option<&Path>, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let enabled = config_path.is_some();
        if ui.add_enabled(enabled, egui::Button::new("Save config")).clicked() {
            actions.push(Action::SaveConfig);
        }
        if ui.add_enabled(enabled, egui::Button::new("Reload")).clicked() {
            actions.push(Action::ReloadConfig);
        }
    });
    match config_path {
        Some(path) => ui.small(format!("File: {}", path.display())),
        None => ui.small("No config path resolved (set HOME or XDG_CONFIG_HOME, or pass --config)."),
    };
    ui.separator();
}

fn rendering_section(
    ui: &mut Ui,
    rendering: &mut RenderingSettings,
    startup: &RenderingSettings,
    wgpu_available: bool,
    wboit_supported: bool,
) {
    widgets::section(ui, "Rendering", |ui| {
        ui.horizontal(|ui| {
            ui.label("Backend:");
            // Keep config editable even if startup used Glow; report when runtime is locked.
            widgets::choice_combo(ui, "pref_render_backend", &mut rendering.backend);
        });
        if !wgpu_available {
            ui.small("Runtime locked to Glow (CPU fallback) because wgpu was not initialized at startup.");
        } else if rendering.backend == RenderBackend::Glow {
            ui.small("Live CPU fallback active (wgpu surface shader bypassed).");
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Transparency:");
            widgets::choice_combo(ui, "pref_transparency", &mut rendering.transparency);
        });
        if rendering.transparency == TransparencyMode::Wboit && wgpu_available && !wboit_supported {
            ui.small("Adapter lacks Rgba16Float blending; using depth peeling automatically.");
        } else if rendering.transparency == TransparencyMode::DepthPeeling {
            ui.horizontal(|ui| {
                ui.label("Depth peel layers:");
                ComboBox::from_id_salt("pref_peel_layers")
                    .selected_text(format!("{}", rendering.depth_peel_layers))
                    .show_ui(ui, |ui| {
                        for &layers in PEEL_LAYER_CHOICES {
                            ui.selectable_value(&mut rendering.depth_peel_layers, layers, format!("{layers}"));
                        }
                    });
            });
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("MSAA samples (restart):");
            ComboBox::from_id_salt("pref_msaa")
                .selected_text(format!("{}x", rendering.multisampling))
                .show_ui(ui, |ui| {
                    for &samples in MSAA_CHOICES {
                        ui.selectable_value(&mut rendering.multisampling, samples, format!("{samples}x"));
                    }
                });
        });
        if rendering.multisampling != startup.multisampling {
            ui.small("Restart required for MSAA sample count changes.");
        }
    });
}

fn param_collapsing(
    ui: &mut Ui,
    scope: &str,
    group: &mut dyn ParamGroup,
    default_open: bool,
    mut extra_color: Option<&mut egui::Color32>,
) {
    CollapsingHeader::new(group.name())
        .id_salt((scope, group.key()))
        .default_open(default_open)
        .show(ui, |ui| {
            if let Some(color) = extra_color.as_deref_mut() {
                let default = crate::state::view_params::default_grid_color();
                ui.horizontal(|ui| {
                    ui.label("Grid color:");
                    ui.color_edit_button_srgba(color);
                    if *color != default && ui.small_button("Reset").clicked() {
                        *color = default;
                    }
                });
            }
            widgets::param_group_controls(ui, group, None);
        });
}
