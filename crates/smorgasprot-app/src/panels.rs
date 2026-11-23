//! Side, top, and bottom panels surrounding the viewport.
//! Panels edit `UiState` directly and request app-level effects via `Action`.

mod cartography_controls;
mod complex_controls;
mod inspector;
pub mod status_bar;
pub mod top_bar;
mod widgets;

use crate::dataset::ViewerDataset;
use crate::state::{UiState, WorkspaceTab};
use egui::{ScrollArea, Ui};

/// Left-hand controls for the active workspace tab.
pub fn controls(ui: &mut Ui, ds: &mut ViewerDataset, state: &mut UiState) {
    ScrollArea::vertical().show(ui, |ui| match state.active_tab {
        WorkspaceTab::Complex => complex_controls::show(ui, ds, state),
        WorkspaceTab::Cartography => cartography_controls::show(ui, ds, state),
    });
}

/// Right-hand inspector, scrollable.
pub fn inspector(ui: &mut Ui, ds: &ViewerDataset, state: &UiState) {
    ScrollArea::vertical().id_salt("inspector_scroll").show(ui, |ui| inspector::show(ui, ds, state));
}
