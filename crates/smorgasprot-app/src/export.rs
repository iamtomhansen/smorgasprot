//! Export edges: the standalone HTML report and the optional live recompute server.

pub mod html;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;

pub use html::generate_viewer_html;
