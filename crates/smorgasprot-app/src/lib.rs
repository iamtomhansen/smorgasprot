//! Smorgasprot GUI: interactive 2D/3D protein interface cartography on egui (native and wasm).
//! Layering, low to high: `camera`, `dataset` → `state` → `style` → `panels`, `viewport` → `app`.
//!
//! `loader` and `export` are I/O edges that the binary (or a future web entry point) calls.
//! Lower layers never import higher ones; scenes only paint, and only `viewport` mutates selection.

pub mod app;
pub mod camera;
pub mod dataset;
pub mod export;
pub mod loader;
pub mod panels;
pub mod state;
pub mod style;
pub mod viewport;

pub use app::SmorgasprotApp;
