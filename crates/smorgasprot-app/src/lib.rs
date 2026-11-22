//! Smorgasprot GUI: interactive 2D/3D protein interface cartography on egui (native and wasm).
//! Layering, low to high: `camera`, `dataset` → `state`, `status`, `action` → `style` → `panels`, `viewport` → `app`.

pub mod action;
pub mod app;
pub mod camera;
pub mod dataset;
pub mod export;
pub mod loader;
pub mod panels;
pub mod state;
pub mod status;
pub mod style;
pub mod viewport;

pub use app::SmorgasprotApp;
