//! `SmorgasprotApp`: the `eframe::App` that owns datasets and UI state and lays out the panels.
//! Each frame: advance animations, draw panels, then apply queued `Action`s.

use crate::action::Action;
use crate::config::{AppConfig, ConfigPollEvent, ConfigWatcher, StartupConfig};
use crate::dataset::ViewerDataset;
use crate::files::{self, FileKind};
use crate::jobs::{JobOutput, JobQueue};
use crate::loader;
use crate::panels;
use crate::render::GpuSurfaceRenderer;
use crate::state::{RenderBackend, RenderingSettings, SurfaceCulling, UiState};
use crate::status::StatusMessage;
use crate::viewport;
use egui::{CentralPanel, Context, SidePanel, TopBottomPanel};
use std::path::Path;

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
    config: AppConfig,
    startup_rendering: RenderingSettings,
    config_watcher: ConfigWatcher,
    status: Option<StatusMessage>,
    jobs: JobQueue,
    /// `None` on the glow escape hatch or a wgpu init failure; `complex_scene` falls back to
    /// its CPU `DrawList` path for surfaces in that case (the "GPU check").
    gpu_surfaces: Option<GpuSurfaceRenderer>,
    last_viewport_rect: Option<egui::Rect>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_png_export: Option<std::path::PathBuf>,
}

impl SmorgasprotApp {
    pub fn new(cc: &eframe::CreationContext<'_>, datasets: Vec<ViewerDataset>, loaded_cfg: StartupConfig) -> Self {
        crate::style::configure_fonts(&cc.egui_ctx);
        let gpu_surfaces = cc.wgpu_render_state.as_ref().map(GpuSurfaceRenderer::new);
        if gpu_surfaces.is_none() {
            eprintln!("Warning: no wgpu render state available; molecular surfaces will use the CPU fallback path.");
        }
        let StartupConfig { config, path, warning } = loaded_cfg;
        let startup_rendering = config.rendering.clone();
        let mut state = UiState::default();
        state.view_params = config.view.clone();
        let status = warning.map(|w| StatusMessage::warning(w, 0.0, &state.view_params.status));
        let config_watcher = ConfigWatcher::new(path);
        Self {
            datasets,
            active: 0,
            state,
            config,
            startup_rendering,
            config_watcher,
            status,
            jobs: JobQueue::default(),
            gpu_surfaces,
            last_viewport_rect: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_png_export: None,
        }
    }

    fn apply_reloaded_config(&mut self, cfg: AppConfig) {
        self.state.view_params = cfg.view.clone();
        self.config = cfg;
    }

    /// Spawns a background load of `paths`, discarding the current workspace on success.
    fn start_load(&mut self, ctx: &Context, paths: Vec<String>) {
        let label = match paths.as_slice() {
            [one] => format!("Loading {one}…"),
            many => format!("Loading {} structures…", many.len()),
        };
        let analysis = self.config.analysis.clone();
        self.jobs.spawn(ctx, label, move || {
            let started = std::time::Instant::now();
            let loaded = loader::load_paths(&paths, &analysis)?;
            let datasets = loaded.into_iter().map(|l| l.dataset).collect();
            Ok(JobOutput::Opened { datasets, elapsed: started.elapsed() })
        });
    }

    /// Applies one panel-requested command; `now` is egui time in seconds.
    fn apply(
        &mut self,
        action: Action,
        #[cfg_attr(target_arch = "wasm32", allow(unused_variables))] ctx: &Context,
        now: f64,
    ) {
        match action {
            Action::SelectDataset(idx) if idx < self.datasets.len() => {
                self.active = idx;
                // Atom and residue indices are per-dataset, so the old selection is cleared.
                // Surface display settings (preset/mesh params/culling) live on `ViewerDataset`
                // itself, not here, so there's nothing else to reconcile when switching tabs.
                self.state.selection.clear();
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
            Action::ExportPng => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let default_name = self
                        .datasets
                        .get(self.active)
                        .map_or_else(|| "viewport.png".to_owned(), |ds| format!("{}.png", ds.id));
                    if let Some(path) = files::dialogs::pick_png_save(&default_name) {
                        self.pending_png_export = Some(path);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                        ctx.request_repaint();
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    self.status = Some(StatusMessage::error(
                        "PNG export is not yet available in the browser build.",
                        now,
                        &self.state.view_params.status,
                    ));
                }
            }
            Action::ExportHtml => self.status = Some(self.export_html(now)),
            Action::DismissStatus => self.status = None,
            Action::SaveConfig => {
                self.config.view = self.state.view_params.clone();
                let params = &self.state.view_params.status;
                self.status = Some(match self.config_watcher.path().map(Path::to_path_buf) {
                    Some(path) => match self.config.save_to(&path) {
                        Ok(()) => {
                            self.config_watcher.record_synced();
                            StatusMessage::success(format!("Saved config to {}", path.display()), now, params)
                        }
                        Err(err) => StatusMessage::error(err, now, params),
                    },
                    None => StatusMessage::warning("No config path available.", now, params),
                });
            }
            Action::ReloadConfig => {
                self.status = Some(match self.config_watcher.path().map(Path::to_path_buf) {
                    Some(path) => match AppConfig::reload_from(&path) {
                        Ok(cfg) => {
                            self.apply_reloaded_config(cfg);
                            self.config_watcher.record_synced();
                            StatusMessage::info(format!("Reloaded {}", path.display()), now, &self.state.view_params.status)
                        }
                        Err(err) => StatusMessage::warning(err, now, &self.state.view_params.status),
                    },
                    None => StatusMessage::warning("No config path available.", now, &self.state.view_params.status),
                });
            }
            Action::ResetConfig => {
                let default_cfg = AppConfig::default();
                self.apply_reloaded_config(default_cfg.clone());
                self.status = Some(match self.config_watcher.path().map(Path::to_path_buf) {
                    Some(path) => match default_cfg.save_to(&path) {
                        Ok(()) => {
                            self.config_watcher.record_synced();
                            StatusMessage::success(
                                format!("Reset config to defaults ({})", path.display()),
                                now,
                                &self.state.view_params.status,
                            )
                        }
                        Err(err) => StatusMessage::error(err, now, &self.state.view_params.status),
                    },
                    None => StatusMessage::info("Reset preferences to defaults.", now, &self.state.view_params.status),
                });
            }
        }
    }

    /// Applies a finished background job: a successful load replaces the whole workspace.
    fn apply_job(&mut self, result: crate::jobs::JobResult, now: f64) {
        let params = self.state.view_params.status.clone();
        self.status = Some(match result {
            Ok(JobOutput::Opened { datasets, elapsed }) => {
                let what = match datasets.as_slice() {
                    [one] => one.title.clone(),
                    many => format!("{} structures", many.len()),
                };
                self.datasets = datasets;
                self.active = 0;
                self.state.selection.clear();
                let has_interface = self.datasets.first().is_some_and(|ds| ds.active_interface().is_some());
                self.state.garnish.retarget_color_defaults(has_interface);

                // A large structure starts at a coarser preset + backface culling, decided and
                // baked onto the dataset itself at construction in `ViewerDataset::new` — this
                // just reads that fact to phrase the status message, it isn't deciding anything.
                let detail_note = match self.datasets.first() {
                    Some(ds) if ds.culling == SurfaceCulling::Backfaces => " (reduced display detail due to size)",
                    _ => "",
                };
                StatusMessage::success(format!("Loaded {what} in {:.2}s{detail_note}", elapsed.as_secs_f64()), now, &params)
            }
            Err(e) => StatusMessage::error(e, now, &params),
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

#[cfg(not(target_arch = "wasm32"))]
fn write_viewport_png(
    path: &std::path::Path,
    image: &egui::ColorImage,
    viewport_rect: Option<egui::Rect>,
    pixels_per_point: f32,
) -> Result<(), String> {
    let cropped;
    let src = if let Some(rect) = viewport_rect {
        cropped = image.region(&rect, Some(pixels_per_point));
        &cropped
    } else {
        image
    };
    let [w, h] = src.size;
    if w == 0 || h == 0 {
        return Err("Viewport has zero area.".into());
    }
    let file = std::fs::File::create(path).map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| format!("PNG header error: {e}"))?;
    let bytes: &[u8] = bytemuck::cast_slice(&src.pixels);
    writer.write_image_data(bytes).map_err(|e| format!("PNG write error: {e}"))?;
    Ok(())
}

impl eframe::App for SmorgasprotApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let (now, dt) = ctx.input(|i| (i.time, i.stable_dt.min(MAX_DT_S)));
        #[cfg(not(target_arch = "wasm32"))]
        if self.pending_png_export.is_some() {
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|ev| match ev {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot
                && let Some(path) = self.pending_png_export.take()
            {
                let params = &self.state.view_params.status;
                self.status = Some(match write_viewport_png(&path, &img, self.last_viewport_rect, ctx.pixels_per_point()) {
                    Ok(()) => StatusMessage::success(format!("Exported viewport PNG to {}", path.display()), now, params),
                    Err(e) => StatusMessage::error(e, now, params),
                });
            }
        }
        if self.state.step_animations(dt) {
            ctx.request_repaint();
        }

        // Apply any debounced presentation surface or interface sheet remesh whose timer has elapsed before rendering.
        for ds in &mut self.datasets {
            ds.poll_draw_surface_remesh(now);
            ds.poll_interface_sheet_remesh(now);
            ds.poll_cavity_remesh(now);
            ds.poll_contour_remesh(now);
        }

        // Poll app config for external edits at the configured cadence.
        match self.config_watcher.poll(now, self.state.view_params.config.poll_interval_s) {
            Some(ConfigPollEvent::Reloaded(cfg)) => {
                self.apply_reloaded_config(cfg);
                self.status = Some(StatusMessage::info(
                    "Reloaded config from disk.",
                    now,
                    &self.state.view_params.status,
                ));
            }
            Some(ConfigPollEvent::Warning(err)) => {
                self.status = Some(StatusMessage::warning(err, now, &self.state.view_params.status));
            }
            None => {}
        }
        if let Some(delay) = self.config_watcher.next_poll_delay_s(now, self.state.view_params.config.poll_interval_s) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(delay));
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

        ctx.set_visuals(if self.state.dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() });
        if self.state.dark_mode {
            self.state.palette.chrome.viewport_bg = self.state.view_params.dark_viewport_bg;
            self.state.palette.chrome.ink = egui::Color32::from_rgb(226, 232, 240);
        } else {
            self.state.palette.chrome.viewport_bg = self.state.view_params.viewport_bg;
            self.state.palette.chrome.ink = egui::Color32::from_rgb(15, 23, 42);
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
                    .show(ctx, |ui| panels::inspector(ui, ds, &mut self.state));
            }
            let active_gpu = match self.config.rendering.backend {
                RenderBackend::Auto | RenderBackend::Wgpu => self.gpu_surfaces.as_mut(),
                RenderBackend::Glow => None,
            };
            CentralPanel::default().show(ctx, |ui| {
                self.last_viewport_rect = Some(ui.max_rect());
                viewport::show(ui, ds, &mut self.state, &self.config.rendering, active_gpu);
            });
        } else {
            CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| ui.label("No complex loaded. Pass a structure file on the command line."));
            });
        }

        if self.state.panels.preferences {
            let wboit_supported = self.gpu_surfaces.as_ref().is_some_and(GpuSurfaceRenderer::wboit_supported);
            panels::preferences::show(
                ctx,
                &mut self.state,
                &mut self.config,
                &self.startup_rendering,
                self.gpu_surfaces.is_some(),
                wboit_supported,
                self.config_watcher.path(),
                &mut actions,
            );
        }

        for action in actions {
            self.apply(action, ctx, now);
        }

        // Schedule wake-up if a panel scheduled a debounced remesh during this frame.
        for ds in &mut self.datasets {
            if let Some(rem) = ds.poll_draw_surface_remesh(now) {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(rem));
            }
            if let Some(rem) = ds.poll_interface_sheet_remesh(now) {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(rem));
            }
            if let Some(rem) = ds.poll_cavity_remesh(now) {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(rem));
            }
            if let Some(rem) = ds.poll_contour_remesh(now) {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(rem));
            }
        }
    }
}
