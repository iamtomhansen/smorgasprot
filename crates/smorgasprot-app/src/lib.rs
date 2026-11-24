//! Smorgasprot GUI: interactive 2D/3D protein interface cartography on egui (native and wasm).
//! Layering, low to high: `camera`, `state` → `render_policy` → `dataset` → `render`, `status`, `action` → `style` → `panels`, `viewport` → `app`.
//! (`render_policy` only borrows `state::SurfaceCulling`'s enum definition — it has no dependency
//! on `UiState` itself or on anything above it; `dataset` depends on it for new-dataset defaults.)

pub mod action;
pub mod app;
pub mod camera;
pub mod config;
pub mod dataset;
pub mod export;
pub mod files;
pub mod jobs;
pub mod loader;
pub mod panels;
pub mod render;
pub mod render_policy;
pub mod state;
pub mod status;
pub mod style;
pub mod viewport;

pub use app::SmorgasprotApp;
