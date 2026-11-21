//! Per-machine preferences restored at startup and saved automatically by eframe.

use crate::state::UiState;
use serde::{Deserialize, Serialize};
use smorgasprot_core::AnalysisParams;

/// eframe storage key for [`AppConfig`].
const STORAGE_KEY: &str = "smorgasprot_config";
/// Number of entries kept in File → Open Recent.
const MAX_RECENT: usize = 10;

/// Preferences that outlive a session.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct AppConfig {
    /// Working analysis parameters, applied to new loads and recomputes.
    pub analysis: AnalysisParams,
    /// Last view state (tab, layers, lenses, display parameters, camera).
    pub ui: UiState,
    /// Recently opened structure and session paths, newest first.
    pub recent_files: Vec<String>,
}

impl AppConfig {
    /// Reads saved preferences, falling back to defaults if absent or unreadable.
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        storage.and_then(|s| eframe::get_value(s, STORAGE_KEY)).unwrap_or_default()
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORAGE_KEY, self);
    }

    /// Moves `path` to the top of the recent list.
    pub fn push_recent(&mut self, path: &str) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_owned());
        self.recent_files.truncate(MAX_RECENT);
    }
}
