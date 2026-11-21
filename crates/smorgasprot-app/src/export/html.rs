//! Standalone HTML report: the frozen JS canvas viewer with all datasets embedded as JSON.
//! `template.html` is kept verbatim from the original viewer; only the payload marker is replaced.

use crate::dataset::ViewerDataset;

/// The frozen viewer page, embedded at compile time.
const TEMPLATE: &str = include_str!("template.html");
/// Placeholder in `TEMPLATE` replaced by the serialized `ViewerDataset` array.
const PAYLOAD_MARKER: &str = "/*__SMORGASPROT_DATASETS_JSON__*/";

/// Builds a single self-contained HTML page that renders `datasets` in any browser.
pub fn generate_viewer_html(datasets: &[ViewerDataset]) -> Result<String, String> {
    let json = serde_json::to_string(datasets).map_err(|e| format!("JSON serialization error: {e}"))?;
    Ok(TEMPLATE.replace(PAYLOAD_MARKER, &json))
}
