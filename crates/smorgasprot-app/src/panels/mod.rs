//! Immediate-mode UI layout panels: top menu bar, left control panel, right inspector, and status strip.

pub mod inspector;
pub mod left_controls;
pub mod status_bar;
pub mod top_bar;

pub use inspector::render_right_panel;
pub use left_controls::render_left_panel;
pub use status_bar::render_status_bar;
pub use top_bar::render_top_bar;
