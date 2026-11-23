//! Small reusable widgets shared by the side panels.
//! Keeps section styling, sliders, and combo boxes consistent across tabs.

use crate::state::{Choice, Tween};
use crate::style::ChromePalette;
use egui::{Color32, ComboBox, RichText, Slider, Ui, vec2};

/// Range for every opacity slider (fully transparent layers are hidden via checkboxes instead).
const ALPHA_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;
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

/// Labelled opacity slider in `[0, 1]` driving a [`Tween`].
pub fn alpha_slider(ui: &mut Ui, label: impl Into<egui::WidgetText>, alpha: &mut Tween) {
    ui.horizontal(|ui| {
        ui.label(label);
        bare_alpha_slider(ui, alpha);
    });
}

/// Opacity slider in `[0, 1]` without an inline label (for grid layouts).
pub fn bare_alpha_slider(ui: &mut Ui, alpha: &mut Tween) {
    let mut value = alpha.value;
    let resp = ui.add(Slider::new(&mut value, ALPHA_RANGE));
    // Only cancel an in-flight Tween on direct human interaction (never on formatting/programmatic updates).
    if resp.changed() && (resp.dragged() || resp.clicked() || resp.has_focus() || resp.lost_focus()) {
        alpha.snap_to(value);
    }
}

/// `[0, 1]` slider driving a [`Tween`]; dragging jumps immediately, cancelling any preset animation.
pub fn tween_slider(ui: &mut Ui, tween: &mut Tween, text: impl Into<egui::WidgetText>) {
    let mut value = tween.value;
    if ui.add(Slider::new(&mut value, 0.0..=1.0).text(text)).changed() {
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

/// Toggle button that opens a layer customization popover (`egui::Area` so nested `ComboBox`es work).
/// Closes automatically when clicking outside the popover or pressing `Escape`.
pub fn customize_popover(ui: &mut Ui, id_salt: &str, modified: bool, add_contents: impl FnOnce(&mut Ui)) {
    let state_key = egui::Id::new("active_customize_popover");
    let this_id = ui.make_persistent_id(id_salt);
    let label = if modified { "Customize*..." } else { "Customize..." };
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
        // Snapshot whether a nested ComboBox popup was open at the start of the frame so selecting an item
        // from the ComboBox doesn't count as clicking outside the outer popover.
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
