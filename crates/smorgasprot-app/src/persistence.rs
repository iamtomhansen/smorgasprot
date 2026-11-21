//! Persistence in two tiers: auto-saved preferences and explicit session files.
//! Both are serde structs with `#[serde(default)]`, so files from older versions still load.
//!
//! - [`config::AppConfig`]: per-machine preferences (working parameters, last view, recent files),
//!   stored by eframe (a RON file natively, `localStorage` on web).
//! - [`session::Session`]: a self-contained JSON document (structure text, groups, parameters, view)
//!   that reproduces an analysis exactly on any machine.

pub mod config;
pub mod session;
