//! App-wide configuration persistence, path resolution, and mtime hot-reload watcher.
//! Stores rendering, view, and analysis settings in RON while keeping session state separate.

use crate::state::{RenderingSettings, ViewParams};
use serde::{Deserialize, Serialize};
use smorgasprot_core::params::AnalysisParams;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const LOCAL_CONFIG_NAME: &str = "smorgasprot.ron";
const APP_DIR_NAME: &str = "smorgasprot";
const USER_CONFIG_NAME: &str = "config.ron";

/// App-wide persisted configuration across rendering, UI defaults, and analysis parameters.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub rendering: RenderingSettings,
    pub view: ViewParams,
    pub analysis: AnalysisParams,
}

/// Startup config load outcome, carrying the resolved target path and any non-fatal warning.
#[derive(Clone, Debug)]
pub struct StartupConfig {
    pub config: AppConfig,
    pub path: Option<PathBuf>,
    pub warning: Option<String>,
}

/// Result of polling the config file for external modifications.
#[derive(Clone, Debug)]
pub enum ConfigPollEvent {
    Reloaded(AppConfig),
    Warning(String),
}

impl AppConfig {
    /// Loads config from `explicit`, `./smorgasprot.ron` if present, or `$XDG_CONFIG_HOME` / `~/.config`.
    pub fn load(explicit: Option<&Path>) -> StartupConfig {
        let path = resolve_config_path(explicit);
        let Some(ref p) = path else {
            return StartupConfig { config: Self::default(), path: None, warning: None };
        };
        match read_config_file(p) {
            Ok(Some(config)) => StartupConfig { config, path, warning: None },
            Ok(None) => StartupConfig { config: Self::default(), path, warning: None },
            Err(err) => StartupConfig {
                config: Self::default(),
                path,
                warning: Some(err),
            },
        }
    }

    /// Reads and parses a config file synchronously, returning `Err` if missing or invalid.
    pub fn reload_from(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config {}: {e}", path.display()))?;
        ron::from_str(&text).map_err(|e| format!("Invalid RON in {}: {e}", path.display()))
    }

    /// Atomically writes this config as pretty RON to `path`, creating parent directories as needed.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory {}: {e}", parent.display()))?;
        }
        let ron_text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| format!("Failed to serialize config: {e}"))?;
        let tmp_name = format!(
            ".{}.tmp.{}",
            path.file_name().and_then(|s| s.to_str()).unwrap_or("config.ron"),
            std::process::id()
        );
        let tmp_path = path.with_file_name(tmp_name);
        if let Err(e) = std::fs::write(&tmp_path, ron_text) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("Failed to write {}: {e}", path.display()));
        }
        std::fs::rename(&tmp_path, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            format!("Failed to replace {}: {e}", path.display())
        })
    }
}

/// Resolves the active config path: `--config`, else existing `./smorgasprot.ron`, else XDG/HOME config.
pub fn resolve_config_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    let local = PathBuf::from(LOCAL_CONFIG_NAME);
    if local.exists() {
        return Some(local);
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(xdg).join(APP_DIR_NAME).join(USER_CONFIG_NAME));
    }
    std::env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .map(|home| PathBuf::from(home).join(".config").join(APP_DIR_NAME).join(USER_CONFIG_NAME))
}

/// Reads a config file, returning `Ok(None)` if missing, or `Err` if invalid.
fn read_config_file(path: &Path) -> Result<Option<AppConfig>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => ron::from_str(&text)
            .map(Some)
            .map_err(|e| format!("Invalid RON in {} (using defaults): {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read {} (using defaults): {e}", path.display())),
    }
}

/// Checks the file modification time of `path`, returning `None` if missing or inaccessible.
fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// Lightweight mtime-based poller for hot-reloading external config edits.
#[derive(Debug)]
pub struct ConfigWatcher {
    path: Option<PathBuf>,
    last_check: f64,
    last_mtime: Option<SystemTime>,
    errored_mtime: Option<SystemTime>,
}

impl ConfigWatcher {
    /// Creates a watcher initialized with the current modification time of `path`.
    pub fn new(path: Option<PathBuf>) -> Self {
        let last_mtime = path.as_deref().and_then(file_mtime);
        Self { path, last_check: f64::NEG_INFINITY, last_mtime, errored_mtime: None }
    }

    /// Path being watched/saved, if resolved.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Updates recorded mtime after an in-app save or manual reload so we do not re-trigger on ourselves.
    pub fn record_synced(&mut self) {
        self.last_mtime = self.path.as_deref().and_then(file_mtime);
        self.errored_mtime = None;
    }

    /// Polls config mtime when `interval_s > 0` and enough time has elapsed since the last check.
    pub fn poll(&mut self, now: f64, interval_s: f32) -> Option<ConfigPollEvent> {
        if interval_s <= 0.0 {
            return None;
        }
        let path = self.path.as_deref()?;
        if now - self.last_check < interval_s as f64 {
            return None;
        }
        self.last_check = now;
        let mtime = file_mtime(path)?;
        if Some(mtime) == self.last_mtime || Some(mtime) == self.errored_mtime {
            return None;
        }
        match AppConfig::reload_from(path) {
            Ok(cfg) => {
                self.last_mtime = Some(mtime);
                self.errored_mtime = None;
                Some(ConfigPollEvent::Reloaded(cfg))
            }
            Err(err) => {
                self.errored_mtime = Some(mtime);
                Some(ConfigPollEvent::Warning(err))
            }
        }
    }

    /// Remaining seconds until the next scheduled poll, or `None` if watching is disabled.
    pub fn next_poll_delay_s(&self, now: f64, interval_s: f32) -> Option<f64> {
        if interval_s <= 0.0 || self.path.is_none() {
            return None;
        }
        let elapsed = (now - self.last_check).max(0.0);
        Some((interval_s as f64 - elapsed).max(0.05))
    }
}
