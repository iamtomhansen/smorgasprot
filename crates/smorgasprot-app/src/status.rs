//! Status-bar notifications with severity levels and configurable auto-expiry or manual dismissal.
//! Timestamps come from `egui::InputState::time` so expiration works identically on native and wasm.

use crate::state::StatusParams;

/// Severity and visual styling category for a [`StatusMessage`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Warning,
    Error,
}

/// A status-bar message; `expires_at == None` persists until manually dismissed.
#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub text: String,
    pub kind: StatusKind,
    pub expires_at: Option<f64>,
}

impl StatusMessage {
    /// Creates a message whose lifetime is governed by `params` (`<= 0` persists until dismissed).
    pub fn new(kind: StatusKind, text: impl Into<String>, now: f64, params: &StatusParams) -> Self {
        let expires_at = (params.lifetime_s > 0.0).then(|| now + f64::from(params.lifetime_s));
        Self { text: text.into(), kind, expires_at }
    }

    pub fn info(text: impl Into<String>, now: f64, params: &StatusParams) -> Self {
        Self::new(StatusKind::Info, text, now, params)
    }

    pub fn success(text: impl Into<String>, now: f64, params: &StatusParams) -> Self {
        Self::new(StatusKind::Success, text, now, params)
    }

    pub fn warning(text: impl Into<String>, now: f64, params: &StatusParams) -> Self {
        Self::new(StatusKind::Warning, text, now, params)
    }

    pub fn error(text: impl Into<String>, now: f64, params: &StatusParams) -> Self {
        Self::new(StatusKind::Error, text, now, params)
    }

    /// Creates a message that stays visible until explicitly dismissed.
    pub fn persistent(kind: StatusKind, text: impl Into<String>) -> Self {
        Self { text: text.into(), kind, expires_at: None }
    }

    /// Returns `true` once `now` has reached `expires_at`.
    pub fn is_expired(&self, now: f64) -> bool {
        self.expires_at.is_some_and(|t| now >= t)
    }

    /// Seconds remaining until auto-expiry, or `None` if persistent.
    pub fn remaining_s(&self, now: f64) -> Option<f64> {
        self.expires_at.map(|t| (t - now).max(0.0))
    }
}
