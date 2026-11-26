//! Small reusable widgets shared by the side panels.
//! Keeps section styling, sliders, and combo boxes consistent across tabs.

use crate::state::{
    AtomSizeMode, Choice, ColorLens, NodeStrokeColorLens, PresentationCaps, PresentationStyle, SphereRepStyle, Tween,
};
use crate::style::ChromePalette;
use egui::{Color32, ComboBox, RichText, Slider, Ui, vec2};
use smorgasprot_core::params::{ChoiceOption, ParamDefault, ParamGroup, ParamMeta, ParamVisitor};

/// Range for normalized `[0, 1]` sliders (opacity, flattening, book opening, explode).
pub const UNIT_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;
/// Slider range for atom radius / ribbon width scale multipliers.
pub const RADIUS_RANGE: std::ops::RangeInclusive<f32> = 0.0..=3.0;
/// Slider range for stroke and contact edge widths (px).
pub const EDGE_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 0.0..=6.0;

/// A titled, grouped block of controls followed by a small gap.
pub fn section(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(title).strong().color(ChromePalette::default().accent));
        add_contents(ui);
    });
    ui.add_space(6.0);
}

/// Labelled scalar slider driving a [`Tween`].
pub fn slider(
    ui: &mut Ui,
    label: impl Into<egui::WidgetText>,
    tween: &mut Tween,
    range: std::ops::RangeInclusive<f32>,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        bare_slider(ui, tween, range);
    });
}

/// Scalar slider driving a [`Tween`] without an inline label (for grid layouts).
pub fn bare_slider(ui: &mut Ui, tween: &mut Tween, range: std::ops::RangeInclusive<f32>) {
    tween_slider_widget(ui, tween, range, None);
}

/// `[0, 1]` slider driving a [`Tween`]; dragging jumps immediately, cancelling any preset animation.
pub fn tween_slider(ui: &mut Ui, tween: &mut Tween, text: impl Into<egui::WidgetText>) {
    tween_slider_widget(ui, tween, UNIT_RANGE, Some(text.into()));
}

fn tween_slider_widget(
    ui: &mut Ui,
    tween: &mut Tween,
    range: std::ops::RangeInclusive<f32>,
    text: Option<egui::WidgetText>,
) {
    let mut value = tween.value;
    // Format displayed decimals without modifying/rounding the underlying value.
    let mut widget = Slider::new(&mut value, range).custom_formatter(|v, _| format!("{v:.2}"));
    if let Some(t) = text {
        widget = widget.text(t);
    }
    let resp = ui.add(widget);
    // Only cancel an in-flight Tween on direct human interaction (never on formatting/programmatic updates).
    if resp.changed() && (resp.dragged() || resp.clicked() || resp.has_focus() || resp.lost_focus()) {
        tween.snap_to(value);
    }
}

/// Row of preset buttons that animate a [`Tween`]; returns the clicked target, if any.
pub fn tween_presets(ui: &mut Ui, tween: &mut Tween, presets: &[(&str, f32)]) -> Option<f32> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for &(label, target) in presets {
            if ui.button(label).clicked() {
                tween.animate_to(target);
                clicked = Some(target);
            }
        }
    });
    clicked
}

/// Combo box over every option of a [`Choice`] enum.
pub fn choice_combo<T: Choice>(ui: &mut Ui, id_salt: &str, value: &mut T) {
    ComboBox::from_id_salt(id_salt)
        .selected_text(value.label())
        .width(ui.available_width())
        .truncate()
        .show_ui(ui, |ui| {
            for &option in T::ALL {
                ui.selectable_value(value, option, option.label());
            }
        });
}

/// Combo box over a [`Choice`] enum that trims parentheticals on the closed button while showing full labels in the menu.
pub fn choice_combo_short<T: Choice>(ui: &mut Ui, id_salt: &str, value: &mut T) {
    ComboBox::from_id_salt(id_salt)
        .selected_text(truncate_paren(value.label()))
        .width(ui.available_width())
        .truncate()
        .show_ui(ui, |ui| {
            for &option in T::ALL {
                ui.selectable_value(value, option, option.label());
            }
        });
}

/// Visual-channel combo box that inserts a visual divider after `Fixed` and `Off` (index 1).
pub fn channel_combo<T: Choice>(ui: &mut Ui, id_salt: &str, value: &mut T) {
    ComboBox::from_id_salt(id_salt)
        .selected_text(value.label())
        .width(ui.available_width())
        .truncate()
        .show_ui(ui, |ui| {
            for (idx, &option) in T::ALL.iter().enumerate() {
                ui.selectable_value(value, option, option.label());
                if idx == 1 && T::ALL.len() > 2 {
                    ui.separator();
                }
            }
        });
}

/// Minimum width of a layer customization popover so dropdown labels fit cleanly.
pub const POPOVER_MIN_WIDTH: f32 = 290.0;

/// Toggle button opening a floating popover with click-outside and Escape dismissal.
pub fn named_popover(
    ui: &mut Ui,
    id_salt: &str,
    base_label: &str,
    modified: bool,
    add_contents: impl FnOnce(&mut Ui),
) {
    let state_key = egui::Id::new("active_customize_popover");
    let this_id = ui.make_persistent_id(id_salt);
    let label = if modified {
        format!("{base_label}*")
    } else {
        base_label.to_string()
    };
    let resp = ui.button(label);
    let toggled_this_frame = resp.clicked();
    if toggled_this_frame {
        ui.ctx().data_mut(|d| {
            let cur = d.get_temp::<Option<egui::Id>>(state_key).flatten();
            let next = if cur == Some(this_id) { None } else { Some(this_id) };
            d.insert_temp(state_key, next);
        });
    }

    let is_open = ui.ctx().data(|d| d.get_temp::<Option<egui::Id>>(state_key)).flatten() == Some(this_id);
    if is_open {
        // Snapshot nested popup state before drawing so ComboBox selection doesn't close the outer popover.
        let nested_popup_was_open = ui.ctx().memory(|m| m.any_popup_open());
        let pos = resp.rect.left_bottom() + vec2(0.0, 4.0);
        let area_resp = egui::Area::new(this_id)
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .constrain_to(ui.ctx().screen_rect())
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(POPOVER_MIN_WIDTH);
                    add_contents(ui);
                });
            });

        if !toggled_this_frame && !nested_popup_was_open {
            let pressed_outside = ui.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| !area_resp.response.rect.contains(p) && !resp.rect.contains(p))
            });
            let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if pressed_outside || escaped {
                ui.ctx().data_mut(|d| d.insert_temp::<Option<egui::Id>>(state_key, None));
            }
        }
    }
}

/// Legacy single-drawer popover wrapper retained for Cartography controls.
pub fn customize_popover(ui: &mut Ui, id_salt: &str, modified: bool, add_contents: impl FnOnce(&mut Ui)) {
    named_popover(ui, id_salt, "Customize...", modified, add_contents);
}

/// Shared `Presentation` popover for any representation's quality/geometry, shading, culling, border, wireframe, and alphas.
/// Returns `true` when "Reset Layer" was clicked so callers can reset any sibling state cleanly.
#[allow(clippy::too_many_arguments)]
pub fn presentation_popover<Q: PartialEq + Copy + 'static>(
    ui: &mut Ui,
    id_salt: &str,
    title: &str,
    pres: &mut PresentationStyle<Q>,
    baseline: &PresentationStyle<Q>,
    caps: PresentationCaps,
    extra_modified: bool,
    extra_geometry: impl FnOnce(&mut Ui, &mut PresentationStyle<Q>),
) -> bool {
    let mut reset_clicked = false;
    let modified = !pres.is_default_vs(baseline) || extra_modified;
    named_popover(ui, id_salt, "Presentation", modified, |ui| {
        popover_header(ui, false, title, modified, || {
            *pres = baseline.clone();
            reset_clicked = true;
        });
        extra_geometry(ui, pres);
        if caps.shading {
            ui.label("Shading:");
            choice_combo(ui, &format!("{id_salt}_shading"), &mut pres.shading);
        }
        if caps.culling {
            ui.label("Face culling:");
            choice_combo(ui, &format!("{id_salt}_culling"), &mut pres.culling);
        }
        if caps.fill_alpha {
            ui.horizontal(|ui| {
                ui.label("Fill alpha:");
                ui.add(Slider::new(&mut pres.alphas.fill, 0.0..=1.0).step_by(0.05));
            });
        }
        if caps.border {
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut pres.border.enabled, RichText::new("Display border").strong());
                if pres.border.enabled {
                    ui.color_edit_button_srgba(&mut pres.border.color);
                }
            });
            if pres.border.enabled {
                ui.horizontal(|ui| {
                    ui.add(Slider::new(&mut pres.border.width_a, 0.02..=1.0).step_by(0.01).text("Å"));
                });
                ui.horizontal(|ui| {
                    ui.label("Border alpha:");
                    ui.add(Slider::new(&mut pres.alphas.border, 0.0..=1.0).step_by(0.05));
                });
            }
        }
        if caps.wireframe {
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut pres.wireframe.enabled, RichText::new("Wireframe").strong());
                if pres.wireframe.enabled {
                    ui.add(Slider::new(&mut pres.wireframe.width_px, 0.5..=3.0).step_by(0.25).text("px"));
                }
            });
            if pres.wireframe.enabled {
                ui.horizontal(|ui| {
                    ui.label("Wireframe alpha:");
                    ui.add(Slider::new(&mut pres.alphas.wireframe, 0.0..=1.0).step_by(0.05));
                });
            }
        }
    });
    reset_clicked
}

/// Renders dedicated `Channels` and `Presentation` popovers for a [`SphereRepStyle`].
pub fn sphere_popovers(ui: &mut Ui, id_prefix: &str, title: &str, rep: &mut SphereRepStyle) {
    let baseline = SphereRepStyle::default_presentation();
    let pres_id = format!("{id_prefix}_pres");
    let chan_id = format!("{id_prefix}_chan");
    let is_mesh = rep.presentation.quality.geodesic_preset().is_some();
    presentation_popover(
        ui,
        &pres_id,
        &format!("{title} presentation"),
        &mut rep.presentation,
        &baseline,
        PresentationCaps {
            shading: false,
            culling: true,
            border: false,
            wireframe: is_mesh,
            fill_alpha: true,
        },
        false,
        |ui, pres| {
            ui.label(RichText::new("Quality / Geometry").strong());
            choice_combo(ui, &format!("{id_prefix}_quality"), &mut pres.quality);
            if pres.quality.geodesic_preset().is_some() {
                ui.label("Shading:");
                choice_combo(ui, &format!("{id_prefix}_shading"), &mut pres.shading);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut pres.border.enabled, RichText::new("Display border").strong());
                    if pres.border.enabled {
                        ui.color_edit_button_srgba(&mut pres.border.color);
                    }
                });
                if pres.border.enabled {
                    ui.add(Slider::new(&mut pres.border.width_a, 0.02..=1.0).step_by(0.01).text("Å"));
                    ui.horizontal(|ui| {
                        ui.label("Border alpha:");
                        ui.add(Slider::new(&mut pres.alphas.border, 0.0..=1.0).step_by(0.05));
                    });
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label("Border alpha:");
                    ui.add(Slider::new(&mut pres.alphas.border, 0.0..=1.0).step_by(0.05));
                });
            }
        },
    );
    named_popover(ui, &chan_id, "Channels", !rep.is_default_channels(), |ui| {
        popover_header(ui, false, &format!("{title} channels"), !rep.is_default_channels(), || {
            rep.reset_channels();
        });
        color_channel_control(ui, &format!("{id_prefix}_fill"), "Fill color", &mut rep.fill, ColorLens::Fixed);
        scalar_channel_control(
            ui,
            &format!("{id_prefix}_radius"),
            "Radius scale",
            &mut rep.radius,
            AtomSizeMode::Fixed,
            0.3..=2.5,
            "× vdW scale",
        );
        if rep.presentation.quality.geodesic_preset().is_none() {
            color_channel_control(
                ui,
                &format!("{id_prefix}_stroke_col"),
                "Border color",
                &mut rep.stroke_color,
                NodeStrokeColorLens::Fixed,
            );
            scalar_channel_control(
                ui,
                &format!("{id_prefix}_stroke_w"),
                "Border width",
                &mut rep.stroke_width,
                AtomSizeMode::Fixed,
                0.0..=5.0,
                "px",
            );
        }
    });
}

/// Title row inside a layer customization popover with a layer-wide reset button and optional close button.
pub fn popover_header(ui: &mut Ui, show_close: bool, title: &str, modified: bool, on_reset: impl FnOnce()) {
    let state_key = egui::Id::new("active_customize_popover");
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if show_close && ui.small_button("×").on_hover_text("Close").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp::<Option<egui::Id>>(state_key, None));
            }
            if modified && ui.small_button("Reset Layer").clicked() {
                on_reset();
            }
        });
    });
    ui.separator();
}

/// Color channel dropdown (`Fixed`, `Off`, divider, data lenses) + inline reset button + color picker when `Fixed`.
pub fn color_channel_control<T: Choice>(
    ui: &mut Ui,
    id_salt: &str,
    label: &str,
    ch: &mut crate::state::VisualChannel<T, Color32>,
    fixed_variant: T,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).strong());
        if !ch.is_default() && ui.small_button("Reset").on_hover_text("Reset channel to default").clicked() {
            ch.reset();
        }
    });
    channel_combo(ui, id_salt, &mut ch.lens);
    if ch.lens == fixed_variant {
        ui.horizontal(|ui| {
            ui.label("Fixed Color:");
            ui.color_edit_button_srgba(&mut ch.fixed);
        });
    }
    ui.add_space(4.0);
}

/// Scalar channel dropdown (`Fixed`, `Off`, divider, data lenses) + inline reset button + numeric slider when `Fixed`.
pub fn scalar_channel_control<T: Choice>(
    ui: &mut Ui,
    id_salt: &str,
    label: &str,
    ch: &mut crate::state::VisualChannel<T, f32>,
    fixed_variant: T,
    range: std::ops::RangeInclusive<f32>,
    unit: &str,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).strong());
        if !ch.is_default() && ui.small_button("Reset").on_hover_text("Reset channel to default").clicked() {
            ch.reset();
        }
    });
    channel_combo(ui, id_salt, &mut ch.lens);
    if ch.lens == fixed_variant {
        ui.add(Slider::new(&mut ch.fixed, range).step_by(0.05).text(unit));
    }
    ui.add_space(4.0);
}

/// Color swatch followed by a label (legends).
pub fn legend_swatch(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.label(label);
    });
}

/// Trims a label to return only the text before the first opening parenthesis.
pub fn truncate_paren(s: &str) -> &str {
    s.split_once('(').map(|(before, _)| before.trim_end()).unwrap_or(s)
}

/// Computes the maximum rendered pixel width among `texts` in the current body font.
pub fn max_text_width(ui: &Ui, texts: &[&str], strong: bool) -> f32 {
    texts
        .iter()
        .map(|&s| {
            let mut text = RichText::new(s);
            if strong {
                text = text.strong();
            }
            egui::WidgetText::from(text)
                .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max)
}

/// Standard single-line widget row height scaled by the active body font and interact spacing.
pub fn row_height(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body)
        .max(ui.spacing().interact_size.y)
}

/// Renders every field of a [`ParamGroup`] as labelled controls with tooltips and per-field reset buttons.
/// When `baseline` is `Some`, fields compare and reset against it instead of `ParamMeta::default`.
/// Returns `true` if any value changed this frame.
pub fn param_group_controls(
    ui: &mut Ui,
    group: &mut dyn ParamGroup,
    baseline: Option<&mut dyn ParamGroup>,
) -> bool {
    let collector = baseline.map(|b| {
        let mut c = DefaultCollector(Vec::new());
        b.visit(&mut c);
        c.0
    });
    let mut visitor = PopoverParamEditor {
        ui,
        defaults: collector.as_deref(),
        cursor: 0,
        changed: false,
    };
    group.visit(&mut visitor);
    visitor.changed
}

struct DefaultCollector(Vec<ParamDefault>);

impl ParamVisitor for DefaultCollector {
    fn visit_f32(&mut self, _meta: &ParamMeta, value: &mut f32) {
        self.0.push(ParamDefault::F32(*value));
    }

    fn visit_usize(&mut self, _meta: &ParamMeta, value: &mut usize) {
        self.0.push(ParamDefault::Usize(*value));
    }

    fn visit_bool(&mut self, _meta: &ParamMeta, value: &mut bool) {
        self.0.push(ParamDefault::Bool(*value));
    }

    fn visit_choice(&mut self, _meta: &ParamMeta, _options: &'static [ChoiceOption], selected: &mut usize) {
        self.0.push(ParamDefault::Choice(*selected));
    }
}

struct PopoverParamEditor<'a> {
    ui: &'a mut Ui,
    defaults: Option<&'a [ParamDefault]>,
    cursor: usize,
    changed: bool,
}

impl PopoverParamEditor<'_> {
    fn next_default(&mut self, meta: &ParamMeta) -> ParamDefault {
        let d = self
            .defaults
            .and_then(|list| list.get(self.cursor).copied())
            .unwrap_or(meta.default);
        self.cursor += 1;
        d
    }

    fn field_header(&mut self, meta: &ParamMeta, is_default: bool) -> bool {
        let mut reset_clicked = false;
        self.ui.horizontal(|ui| {
            let lbl = ui.label(format!("{}:", meta.name));
            let desc = meta.description.trim();
            if !desc.is_empty() {
                lbl.on_hover_text(desc);
            }
            if !is_default
                && ui
                    .small_button("Reset")
                    .on_hover_text("Reset parameter to default")
                    .clicked()
            {
                reset_clicked = true;
            }
        });
        reset_clicked
    }

    fn finish_field(&mut self, resp: egui::Response) {
        if resp.changed() {
            self.changed = true;
        }
        self.ui.add_space(2.0);
    }
}

impl ParamVisitor for PopoverParamEditor<'_> {
    fn visit_f32(&mut self, meta: &ParamMeta, value: &mut f32) {
        let default = self.next_default(meta);
        let is_default = matches!(default, ParamDefault::F32(d) if *value == d);
        if self.field_header(meta, is_default) {
            if let ParamDefault::F32(d) = default {
                *value = d;
                self.changed = true;
            }
        }
        let resp = match (meta.min, meta.max) {
            (Some(lo), Some(hi)) => {
                let mut w = Slider::new(value, lo as f32..=hi as f32)
                    .custom_formatter(|v, _| format!("{v:.2}"));
                if !meta.unit.is_empty() {
                    w = w.text(meta.unit);
                }
                self.ui.add(w)
            }
            _ => self.ui.add(egui::DragValue::new(value).speed(0.01)),
        };
        self.finish_field(resp);
    }

    fn visit_usize(&mut self, meta: &ParamMeta, value: &mut usize) {
        let default = self.next_default(meta);
        let is_default = matches!(default, ParamDefault::Usize(d) if *value == d);
        if self.field_header(meta, is_default) {
            if let ParamDefault::Usize(d) = default {
                *value = d;
                self.changed = true;
            }
        }
        let resp = match (meta.min, meta.max) {
            (Some(lo), Some(hi)) => {
                let mut w = Slider::new(value, (lo as usize)..=(hi as usize));
                if !meta.unit.is_empty() {
                    w = w.text(meta.unit);
                }
                self.ui.add(w)
            }
            _ => self.ui.add(egui::DragValue::new(value)),
        };
        self.finish_field(resp);
    }

    fn visit_bool(&mut self, meta: &ParamMeta, value: &mut bool) {
        let default = self.next_default(meta);
        let is_default = matches!(default, ParamDefault::Bool(d) if *value == d);
        if self.field_header(meta, is_default) {
            if let ParamDefault::Bool(d) = default {
                *value = d;
                self.changed = true;
            }
        }
        let resp = self.ui.checkbox(value, "");
        self.finish_field(resp);
    }

    fn visit_choice(&mut self, meta: &ParamMeta, options: &'static [ChoiceOption], selected: &mut usize) {
        let default = self.next_default(meta);
        let is_default = matches!(default, ParamDefault::Choice(d) if *selected == d);
        if self.field_header(meta, is_default) {
            if let ParamDefault::Choice(d) = default {
                *selected = d;
                self.changed = true;
            }
        }
        let current_name = options.get(*selected).map(|o| o.name).unwrap_or("Unknown");
        let mut inner_changed = false;
        let resp = ComboBox::from_id_salt(meta.key)
            .selected_text(current_name)
            .show_ui(self.ui, |ui| {
                for (idx, opt) in options.iter().enumerate() {
                    let r = ui.selectable_value(selected, idx, opt.name);
                    if !opt.description.is_empty() {
                        r.clone().on_hover_text(opt.description);
                    }
                    if r.changed() {
                        inner_changed = true;
                    }
                }
            });
        if inner_changed {
            self.changed = true;
        }
        self.finish_field(resp.response);
    }
}
