//! Session files: self-contained JSON with structure text, groups, parameters, and view state.
//! Opening a session recomputes every structure from its embedded inputs, so results are reproducible.

use crate::loader::LoadSpec;
use crate::state::UiState;
use crate::workspace::{GroupAssignment, StructureSource, Workspace};
use serde::{Deserialize, Serialize};
use smorgasprot_core::AnalysisParams;

/// Bump when a change cannot be read by older code; additive changes rely on `#[serde(default)]`.
pub const FORMAT_VERSION: u32 = 1;
/// Suggested file name suffix (the content is plain JSON).
pub const FILE_SUFFIX: &str = ".smorgasprot.json";

/// A saved analysis.
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub format_version: u32,
    /// Version of the app that wrote the file (informational).
    pub app_version: String,
    /// Index of the structure shown when the session was saved.
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub ui: UiState,
    pub structures: Vec<SessionStructure>,
}

/// Inputs needed to recompute one structure exactly.
#[derive(Serialize, Deserialize)]
pub struct SessionStructure {
    pub id: String,
    pub title: String,
    pub source: StructureSource,
    pub groups: GroupAssignment,
    #[serde(default)]
    pub params: AnalysisParams,
}

impl Session {
    /// Snapshots the open structures and view state.
    pub fn capture(workspace: &Workspace, ui: &UiState) -> Self {
        let structures = workspace
            .structures
            .iter()
            .map(|s| SessionStructure {
                id: s.id.clone(),
                title: s.title.clone(),
                source: s.source.clone(),
                groups: s.groups.clone(),
                params: s.params.clone(),
            })
            .collect();
        Self {
            format_version: FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            active: workspace.active,
            ui: ui.clone(),
            structures,
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("Failed to serialize session: {e}"))
    }

    /// Parses a session, rejecting files written by a newer, incompatible format.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let session: Self = serde_json::from_str(text).map_err(|e| format!("Not a valid session file: {e}"))?;
        if session.format_version > FORMAT_VERSION {
            return Err(format!(
                "Session format {} is newer than this app supports ({FORMAT_VERSION}); please update Smorgasprot.",
                session.format_version
            ));
        }
        Ok(session)
    }

    /// Splits into per-structure load specs plus the view state to restore afterwards.
    pub fn into_parts(self) -> (Vec<LoadSpec>, UiState, usize) {
        let specs = self
            .structures
            .into_iter()
            .map(|s| LoadSpec { id: s.id, title: Some(s.title), source: s.source, groups: Some(s.groups), params: s.params })
            .collect();
        (specs, self.ui, self.active)
    }
}
