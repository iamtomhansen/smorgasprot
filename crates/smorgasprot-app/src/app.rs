//! `SmorgasprotApp`: the `eframe::App` that owns datasets and UI state and lays out the panels.
//! Each frame: advance animations, draw panels, then apply queued `Action`s.

use crate::action::Action;
use crate::dataset::ViewerDataset;
use crate::files::{self, FileKind};
use crate::jobs::{JobOutput, JobQueue};
use crate::loader;
use crate::panels;
use crate::state::UiState;
use crate::status::StatusMessage;
use crate::viewport;
use egui::{CentralPanel, Context, SidePanel, TopBottomPanel};
use smorgasprot_core::AnalysisParams;

/// Default width of the left controls panel (px).
const LEFT_PANEL_WIDTH: f32 = 320.0;
/// Default width of the right inspector panel (px).
const RIGHT_PANEL_WIDTH: f32 = 310.0;
/// Resize limits for both side panels (px), per `docs/visual_grammar.md`.
const SIDE_PANEL_RANGE: std::ops::RangeInclusive<f32> = 220.0..=560.0;
/// Longest frame step fed to animations (s), so a stalled frame doesn't teleport tweens.
const MAX_DT_S: f32 = 0.1;
/// Output path for File → Export on native targets.
#[cfg(not(target_arch = "wasm32"))]
const EXPORT_PATH: &str = "viewer.html";

/// Top-level application: every loaded dataset, the active one, and session UI state.
pub struct SmorgasprotApp {
    datasets: Vec<ViewerDataset>,
    active: usize,
    state: UiState,
    status: Option<StatusMessage>,
    jobs: JobQueue,
}

impl SmorgasprotApp {
    pub fn new(cc: &eframe::CreationContext<'_>, datasets: Vec<ViewerDataset>) -> Self {
        crate::style::configure_fonts(&cc.egui_ctx);
        Self { datasets, active: 0, state: UiState::default(), status: None, jobs: JobQueue::default() }
    }

    /// Spawns a background load of `paths`, discarding the current workspace on success.
    fn start_load(&mut self, ctx: &Context, paths: Vec<String>) {
        let label = match paths.as_slice() {
            [one] => format!("Loading {one}…"),
            many => format!("Loading {} structures…", many.len()),
        };
        self.jobs.spawn(ctx, label, move || {
            let started = std::time::Instant::now();
            let loaded = loader::load_paths(&paths, &AnalysisParams::default())?;
            let datasets = loaded.into_iter().map(|l| l.dataset).collect();
            Ok(JobOutput::Opened { datasets, elapsed: started.elapsed() })
        });
    }

    /// Applies one panel-requested command; `now` is egui time in seconds.
    fn apply(&mut self, action: Action, ctx: &Context, now: f64) {
        match action {
            Action::SelectDataset(idx) if idx < self.datasets.len() => {
                self.active = idx;
                // Atom and residue indices are per-dataset, so the old selection is cleared.
                self.state.selection.clear();
                let draw_preset = self.state.complex.surface.draw_preset;
                let draw_surface = &self.state.complex.surface.draw_mesh;
                if self.datasets[idx].applied_draw_preset() != draw_preset
                    || self.datasets[idx].applied_draw_surface() != draw_surface
                {
                    self.datasets[idx].recompute_draw_surfaces(draw_preset, draw_surface);
                }
            }
            Action::SelectDataset(_) => {}
            #[cfg(not(target_arch = "wasm32"))]
            Action::OpenFileDialog => {
                let paths: Vec<String> = files::dialogs::pick_structures()
                    .into_iter()
                    .filter_map(|f| f.path)
                    .collect();
                if !paths.is_empty() {
                    self.start_load(ctx, paths);
                }
            }
            Action::ExportHtml => self.status = Some(self.export_html(now)),
            Action::DismissStatus => self.status = None,
        }
    }

    /// Applies a finished background job: a successful load replaces the whole workspace.
    fn apply_job(&mut self, result: crate::jobs::JobResult, now: f64) {
        let params = &self.state.view_params.status;
        self.status = Some(match result {
            Ok(JobOutput::Opened { datasets, elapsed }) => {
                let what = match datasets.as_slice() {
                    [one] => one.title.clone(),
                    many => format!("{} structures", many.len()),
                };
                self.datasets = datasets;
                self.active = 0;
                self.state.selection.clear();
                StatusMessage::success(format!("Loaded {what} in {:.2}s", elapsed.as_secs_f64()), now, params)
            }
            Err(e) => StatusMessage::error(e, now, params),
        });
    }

    /// Writes the standalone HTML report and returns a status message describing the outcome.
    #[cfg(not(target_arch = "wasm32"))]
    fn export_html(&self, now: f64) -> StatusMessage {
        let params = &self.state.view_params.status;
        let written = crate::export::generate_viewer_html(&self.datasets)
            .and_then(|html| std::fs::write(EXPORT_PATH, html).map_err(|e| format!("Failed to write {EXPORT_PATH}: {e}")));
        match written {
            Ok(()) => {
                let path = std::fs::canonicalize(EXPORT_PATH).unwrap_or_else(|_| EXPORT_PATH.into());
                StatusMessage::success(format!("Exported self-contained report to {}", path.display()), now, params)
            }
            Err(e) => StatusMessage::error(e, now, params),
        }
    }

    /// Browser builds have no filesystem; a download-based export is future work.
    #[cfg(target_arch = "wasm32")]
    fn export_html(&self, now: f64) -> StatusMessage {
        StatusMessage::error(
            "HTML export is not yet available in the browser build.",
            now,
            &self.state.view_params.status,
        )
    }
}

impl eframe::App for SmorgasprotApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let (now, dt) = ctx.input(|i| (i.time, i.stable_dt.min(MAX_DT_S)));
        if self.state.step_animations(dt) {
            ctx.request_repaint();
        }

        // Apply any debounced presentation surface remesh whose timer has elapsed before rendering.
        let draw_preset = self.state.complex.surface.draw_preset;
        let draw_surface = self.state.complex.surface.draw_mesh.clone();
        for ds in &mut self.datasets {
            ds.poll_draw_surface_remesh(now, draw_preset, &draw_surface);
        }

        // Expire auto-timed status messages, and schedule a wake-up when due.
        if let Some(msg) = &self.status {
            if msg.is_expired(now) {
                self.status = None;
            } else if let Some(rem) = msg.remaining_s(now) {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(rem));
            }
        }

        // Apply finished loads before drawing, so their status message shows this frame.
        for (_label, result) in self.jobs.poll() {
            self.apply_job(result, now);
        }

        // Dropped structure files start a load the same way the Open dialog does.
        let dropped = files::take_dropped(ctx);
        if !dropped.is_empty() {
            let mut paths = Vec::new();
            let mut unsupported = false;
            for file in dropped {
                match (file.kind(), file.path) {
                    (FileKind::Structure, Some(path)) => paths.push(path),
                    _ => unsupported = true,
                }
            }
            if !paths.is_empty() {
                self.start_load(ctx, paths);
            } else if unsupported {
                self.status = Some(StatusMessage::warning(
                    "Only PDB/mmCIF structure files can be opened here (session files aren't supported yet).",
                    now,
                    &self.state.view_params.status,
                ));
            }
        }

        if self.jobs.is_busy() {
            ctx.request_repaint();
        }

        let mut actions = Vec::new();

        // Edge panels must be added before the central panel.
        TopBottomPanel::top("top_bar").show(ctx, |ui| {
            panels::top_bar::show(ui, &self.datasets, self.active, &mut self.state, &mut actions);
        });
        if self.state.panels.status_bar {
            TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                panels::status_bar::show(
                    ui,
                    self.datasets.get(self.active),
                    self.status.as_ref(),
                    self.jobs.running().next(),
                    &self.state.palette.chrome,
                    &mut actions,
                );
            });
        }

        if let Some(ds) = self.datasets.get_mut(self.active) {
            if self.state.panels.left_controls {
                SidePanel::left("controls")
                    .resizable(true)
                    .default_width(LEFT_PANEL_WIDTH)
                    .width_range(SIDE_PANEL_RANGE)
                    .show(ctx, |ui| panels::controls(ui, ds, &mut self.state));
            }
            if self.state.panels.right_inspector {
                SidePanel::right("inspector")
                    .resizable(true)
                    .default_width(RIGHT_PANEL_WIDTH)
                    .width_range(SIDE_PANEL_RANGE)
                    .show(ctx, |ui| panels::inspector(ui, ds, &self.state));
            }
            CentralPanel::default().show(ctx, |ui| viewport::show(ui, ds, &mut self.state));
        } else {
            CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| ui.label("No complex loaded. Pass a structure file on the command line."));
            });
        }

        for action in actions {
            self.apply(action, ctx, now);
        }

        // Schedule wake-up if a panel scheduled a debounced presentation surface remesh during this frame.
        let draw_preset = self.state.complex.surface.draw_preset;
        let draw_surface = &self.state.complex.surface.draw_mesh;
        for ds in &mut self.datasets {
            if let Some(rem) = ds.poll_draw_surface_remesh(now, draw_preset, draw_surface) {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(rem));
            }
        }
    }
}
