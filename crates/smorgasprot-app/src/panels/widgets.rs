//! Small reusable widgets shared by the side panels.
//! Keeps section styling, sliders, and combo boxes consistent across tabs.

use crate::state::{Choice, Tween};
use crate::style::ChromePalette;
use egui::{Color32, ComboBox, RichText, Slider, Ui, vec2};

/// Range for every opacity slider (fully transparent layers are hidden via checkboxes instead).
const ALPHA_RANGE: std::ops::RangeInclusive<f32> = 0.05..=1.0;

/// A titled, grouped block of controls followed by a small gap.
pub fn section(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(title).strong().color(ChromePalette::default().accent));
        add_contents(ui);
    });
    ui.add_space(6.0);
}

/// Labelled opacity slider in `[0.05, 1]` driving a [`Tween`].
pub fn alpha_slider(ui: &mut Ui, label: impl Into<egui::WidgetText>, alpha: &mut Tween) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut value = alpha.value;
        if ui.add(Slider::new(&mut value, ALPHA_RANGE).step_by(0.01)).changed() {
            alpha.snap_to(value);
        }
    });
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
        .show_ui(ui, |ui| {
            for &option in T::ALL {
                ui.selectable_value(value, option, option.label());
            }
        });
}

/// Color swatch followed by a label (legends).
pub fn legend_swatch(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.label(label);
    });
}
