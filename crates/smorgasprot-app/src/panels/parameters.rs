//! Parameters window: inspect and tune every analysis and display parameter.
//! Renders any `ParamGroup` generically through a `ParamVisitor`, so new fields appear automatically.

use crate::state::{Action, ViewParams};
use crate::style::palette;
use crate::workspace::LoadedStructure;
use egui::{CollapsingHeader, Context, DragValue, Grid, RichText, Ui, Window};
use smorgasprot_core::AnalysisParams;
use smorgasprot_core::params::{ChoiceOption, ParamDefault, ParamGroup, ParamMeta, ParamVisitor};

/// Renders the window while `open` is true.
pub fn show(
    ctx: &Context,
    open: &mut bool,
    analysis: &mut AnalysisParams,
    view: &mut ViewParams,
    active: Option<(usize, &LoadedStructure)>,
    actions: &mut Vec<Action>,
) {
    Window::new("⚙ Parameters").open(open).default_width(420.0).vscroll(true).show(ctx, |ui| {
        ui.label(RichText::new("Analysis (applied on load or recompute)").strong().color(palette::ACCENT));
        if let Some((index, structure)) = active {
            let stale = structure.params != *analysis;
            ui.horizontal_wrapped(|ui| {
                if stale {
                    ui.colored_label(palette::WARNING, "Active structure was computed with different values.");
                }
                let button = ui.add_enabled(stale && structure.draft.is_valid(), egui::Button::new("Recompute active structure"));
                if button.clicked() {
                    actions.push(Action::Recompute(index));
                }
            });
        }
        for group in analysis.groups_mut() {
            group_section(ui, "analysis", group);
        }

        ui.separator();
        ui.label(RichText::new("Display (applied live)").strong().color(palette::ACCENT));
        for group in view.groups_mut() {
            group_section(ui, "view", group);
        }
    });
}

/// One collapsible group with a reset button and a two-column grid of fields.
fn group_section(ui: &mut Ui, id_prefix: &str, group: &mut dyn ParamGroup) {
    let title = if group.is_default() { group.name().to_owned() } else { format!("{} •", group.name()) };
    CollapsingHeader::new(title).id_salt((id_prefix, group.key())).show(ui, |ui| {
        let desc = group.description().trim();
        if !desc.is_empty() {
            ui.label(RichText::new(desc).weak());
            ui.add_space(2.0);
        }
        Grid::new((id_prefix, group.key(), "grid")).num_columns(2).striped(true).show(ui, |ui| {
            group.visit(&mut GridEditor { ui });
        });
        if ui.add_enabled(!group.is_default(), egui::Button::new("Reset group to defaults")).clicked() {
            group.reset();
        }
    });
}

/// Adds one grid row per visited field.
struct GridEditor<'a> {
    ui: &'a mut Ui,
}

impl GridEditor<'_> {
    fn label(&mut self, meta: &ParamMeta) {
        let resp = self.ui.horizontal(|ui| {
            ui.label(meta.name);
            if !meta.unit.is_empty() {
                ui.colored_label(palette::MUTED, meta.unit);
            }
        }).response;
        let desc = meta.description.trim();
        if !desc.is_empty() {
            resp.on_hover_text(format!("{}\n\nKey: {}", desc, meta.key));
        } else {
            resp.on_hover_text(format!("Key: {}", meta.key));
        }
    }
}

impl ParamVisitor for GridEditor<'_> {
    fn visit_f32(&mut self, meta: &ParamMeta, value: &mut f32) {
        self.label(meta);
        let min = meta.min.unwrap_or(f64::NEG_INFINITY);
        let max = meta.max.unwrap_or(f64::INFINITY);
        let speed = if min.is_finite() && max.is_finite() {
            ((max - min) / 300.0) as f32
        } else {
            0.01
        };
        self.ui.horizontal(|ui| {
            ui.add(DragValue::new(value).range(min as f32..=max as f32).speed(speed).max_decimals(4));
            if !meta.is_default_f32(*value) && ui.small_button("↺").on_hover_text("Reset to default").clicked() {
                meta.reset_f32(value);
            }
        });
        self.ui.end_row();
    }

    fn visit_usize(&mut self, meta: &ParamMeta, value: &mut usize) {
        self.label(meta);
        let min = meta.min.unwrap_or(0.0) as usize;
        let max = meta.max.unwrap_or(usize::MAX as f64) as usize;
        self.ui.horizontal(|ui| {
            ui.add(DragValue::new(value).range(min..=max));
            if !meta.is_default_usize(*value) && ui.small_button("↺").on_hover_text("Reset to default").clicked() {
                meta.reset_usize(value);
            }
        });
        self.ui.end_row();
    }

    fn visit_bool(&mut self, meta: &ParamMeta, value: &mut bool) {
        self.label(meta);
        self.ui.horizontal(|ui| {
            ui.checkbox(value, "");
            if !meta.is_default_bool(*value) && ui.small_button("↺").on_hover_text("Reset to default").clicked() {
                meta.reset_bool(value);
            }
        });
        self.ui.end_row();
    }

    fn visit_choice(&mut self, meta: &ParamMeta, options: &'static [ChoiceOption], selected: &mut usize) {
        self.label(meta);
        self.ui.horizontal(|ui| {
            let current_name = options.get(*selected).map(|o| o.name).unwrap_or("Unknown");
            egui::ComboBox::from_id_salt(meta.key)
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    for (idx, opt) in options.iter().enumerate() {
                        let resp = ui.selectable_value(selected, idx, opt.name);
                        if !opt.description.is_empty() {
                            resp.on_hover_text(opt.description);
                        }
                    }
                });
            if let ParamDefault::Choice(d) = meta.default {
                if *selected != d && ui.small_button("↺").on_hover_text("Reset to default").clicked() {
                    *selected = d;
                }
            }
        });
        self.ui.end_row();
    }
}
