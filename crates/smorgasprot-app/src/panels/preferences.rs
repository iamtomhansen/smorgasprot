//! Floating app-wide Preferences window: Rendering backend, View defaults, Analysis defaults, and config actions.

use super::widgets;
use crate::action::Action;
use crate::config::AppConfig;
use crate::state::{RenderBackend, RenderingSettings, UiState};
use egui::{CollapsingHeader, ComboBox, Context, RichText, ScrollArea, Ui, Window};
use smorgasprot_core::params::ParamGroup;
use std::path::Path;

const MSAA_CHOICES: &[u16] = &[1, 2, 4, 8];

/// Renders the Preferences window when `state.panels.preferences` is open.
pub fn show(
    ctx: &Context,
    state: &mut UiState,
    config: &mut AppConfig,
    startup_rendering: &RenderingSettings,
    wgpu_available: bool,
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
                rendering_section(ui, &mut config.rendering, startup_rendering, wgpu_available);
                ui.add_space(4.0);
                ui.label(RichText::new("View & Hot-Reload Defaults").strong());
                for group in state.view_params.groups_mut() {
                    let default_open = group.key() == "config";
                    param_collapsing(ui, "view", group, default_open);
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Analysis Defaults (applies on next load)").strong());
                for group in config.analysis.groups_mut() {
                    param_collapsing(ui, "analysis", group, false);
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

fn param_collapsing(ui: &mut Ui, scope: &str, group: &mut dyn ParamGroup, default_open: bool) {
    CollapsingHeader::new(group.name())
        .id_salt((scope, group.key()))
        .default_open(default_open)
        .show(ui, |ui| {
            widgets::param_group_controls(ui, group, None);
        });
}
