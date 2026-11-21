//! Main `SmorgasprotApp` struct implementing `eframe::App` for native desktop and WASM execution.

use crate::camera::CameraState;
use crate::dataset::ViewerDataset;
use crate::export::generate_viewer_html;
use crate::panels::{render_left_panel, render_right_panel, render_status_bar, render_top_bar};
use crate::state::{
    AtomSizeMode, BFactorMode, ColorLens, InterfaceDisplayMode, WorkspaceTab,
};
use crate::viewport::render_viewport;
use eframe::App;
use egui::Context;
use std::time::Instant;

pub struct SmorgasprotApp {
    pub datasets: Vec<ViewerDataset>,
    pub dataset_idx: usize,
    pub workspace_tab: WorkspaceTab,
    pub camera: CameraState,

    // Complex Tab controls
    pub cx_show_surface: bool,
    pub cx_surface_alpha: f32,
    pub cx_cull_backfaces: bool,
    pub cx_show_cartoon: bool,
    pub cx_cartoon_alpha: f32,
    pub cx_interface_mode: InterfaceDisplayMode,
    pub cx_interface_alpha: f32,
    pub cx_show_spheres: bool,
    pub cx_spheres_alpha: f32,
    pub explode_t: f32,
    pub target_explode_t: f32,

    // Interface Tab controls
    pub flatten_t: f32,
    pub target_flatten_t: f32,
    pub book_theta: f32,
    pub target_book_theta: f32,
    pub use_relaxed_uv: bool,
    pub lens: ColorLens,
    pub bfactor_mode: BFactorMode,
    pub size_mode: AtomSizeMode,
    pub side_a_alpha: f32,
    pub side_b_alpha: f32,
    pub show_territories: bool,
    pub show_intra_edges: bool,
    pub show_cross_arcs: bool,
    pub show_backbone: bool,

    // Selection & status
    pub hovered_atom_idx: Option<usize>,
    pub selected_territory_idx: Option<usize>,
    pub export_requested: bool,
    pub status_message: Option<(String, Instant)>,
}

impl SmorgasprotApp {
    pub fn new(datasets: Vec<ViewerDataset>) -> Self {
        Self {
            datasets,
            dataset_idx: 0,
            workspace_tab: WorkspaceTab::Complex,
            camera: CameraState::default(),

            cx_show_surface: true,
            cx_surface_alpha: 0.82,
            cx_cull_backfaces: false,
            cx_show_cartoon: true,
            cx_cartoon_alpha: 0.70,
            cx_interface_mode: InterfaceDisplayMode::SmoothedSdf,
            cx_interface_alpha: 0.85,
            cx_show_spheres: false,
            cx_spheres_alpha: 0.90,
            explode_t: 0.0,
            target_explode_t: 0.0,

            flatten_t: 1.0,
            target_flatten_t: 1.0,
            book_theta: 0.0,
            target_book_theta: 0.0,
            use_relaxed_uv: true,
            lens: ColorLens::Interactions,
            bfactor_mode: BFactorMode::Crystallographic,
            size_mode: AtomSizeMode::SqrtBuriedSasa,
            side_a_alpha: 1.0,
            side_b_alpha: 0.35,
            show_territories: true,
            show_intra_edges: true,
            show_cross_arcs: true,
            show_backbone: true,

            hovered_atom_idx: None,
            selected_territory_idx: None,
            export_requested: false,
            status_message: None,
        }
    }
}

impl App for SmorgasprotApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // 1. Smooth interpolation / animation tweens
        let mut animating = false;

        let dt_flatten = self.target_flatten_t - self.flatten_t;
        if dt_flatten.abs() > 0.002 {
            self.flatten_t += dt_flatten * 0.18;
            animating = true;
        } else {
            self.flatten_t = self.target_flatten_t;
        }

        let dt_book = self.target_book_theta - self.book_theta;
        if dt_book.abs() > 0.002 {
            self.book_theta += dt_book * 0.18;
            animating = true;
        } else {
            self.book_theta = self.target_book_theta;
        }

        let dt_explode = self.target_explode_t - self.explode_t;
        if dt_explode.abs() > 0.002 {
            self.explode_t += dt_explode * 0.18;
            animating = true;
        } else {
            self.explode_t = self.target_explode_t;
        }

        if animating {
            ctx.request_repaint();
        }

        // 2. Handle HTML report export
        if self.export_requested {
            self.export_requested = false;
            if let Ok(html) = generate_viewer_html(&self.datasets) {
                if let Ok(()) = std::fs::write("viewer.html", html) {
                    self.status_message = Some((
                        "Successfully exported self-contained viewer.html report!".to_string(),
                        Instant::now(),
                    ));
                }
            }
        }

        // 3. Top Menu Bar
        egui::TopBottomPanel::top("top_menu").show(ctx, |ui| {
            render_top_bar(
                ui,
                &self.datasets,
                &mut self.dataset_idx,
                &mut self.workspace_tab,
                &mut self.camera,
                &mut self.side_a_alpha,
                &mut self.side_b_alpha,
                &mut self.export_requested,
            );
        });

        // 4. Bottom Status Bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            render_status_bar(ui, self.datasets.get(self.dataset_idx), &mut self.status_message);
        });

        if self.datasets.is_empty() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label("No complex loaded. Use File -> Load Example or pass a CIF on CLI.");
                });
            });
            return;
        }

        let ds = &mut self.datasets[self.dataset_idx];

        // 5. Left Panel (Controls)
        egui::SidePanel::left("left_controls")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .max_width(450.0)
            .show(ctx, |ui| {
                render_left_panel(
                    ui,
                    ds,
                    &mut self.workspace_tab,
                    &mut self.cx_show_surface,
                    &mut self.cx_surface_alpha,
                    &mut self.cx_cull_backfaces,
                    &mut self.cx_show_cartoon,
                    &mut self.cx_cartoon_alpha,
                    &mut self.cx_interface_mode,
                    &mut self.cx_interface_alpha,
                    &mut self.cx_show_spheres,
                    &mut self.cx_spheres_alpha,
                    &mut self.explode_t,
                    &mut self.target_explode_t,
                    &mut self.flatten_t,
                    &mut self.target_flatten_t,
                    &mut self.book_theta,
                    &mut self.target_book_theta,
                    &mut self.use_relaxed_uv,
                    &mut self.lens,
                    &mut self.bfactor_mode,
                    &mut self.size_mode,
                    &mut self.side_a_alpha,
                    &mut self.side_b_alpha,
                    &mut self.show_territories,
                    &mut self.show_intra_edges,
                    &mut self.show_cross_arcs,
                    &mut self.show_backbone,
                );
            });

        // 6. Right Panel (Inspector & Diagnostics)
        let ds_ref = &self.datasets[self.dataset_idx];
        egui::SidePanel::right("right_inspector")
            .resizable(true)
            .default_width(310.0)
            .min_width(250.0)
            .max_width(420.0)
            .show(ctx, |ui| {
                render_right_panel(
                    ui,
                    ds_ref,
                    self.lens,
                    self.bfactor_mode,
                    self.hovered_atom_idx,
                    self.selected_territory_idx,
                );
            });

        // 7. Central Panel (Interactive Viewport)
        egui::CentralPanel::default().show(ctx, |ui| {
            render_viewport(
                ui,
                ds_ref,
                self.workspace_tab,
                &mut self.camera,
                self.cx_show_surface,
                self.cx_surface_alpha,
                self.cx_cull_backfaces,
                self.cx_show_cartoon,
                self.cx_cartoon_alpha,
                self.cx_interface_mode,
                self.cx_interface_alpha,
                self.cx_show_spheres,
                self.cx_spheres_alpha,
                self.explode_t,
                self.flatten_t,
                self.book_theta,
                self.use_relaxed_uv,
                self.lens,
                self.bfactor_mode,
                self.size_mode,
                self.side_a_alpha,
                self.side_b_alpha,
                self.show_territories,
                self.show_intra_edges,
                self.show_cross_arcs,
                self.show_backbone,
                &mut self.hovered_atom_idx,
                &mut self.selected_territory_idx,
            );
        });
    }
}
