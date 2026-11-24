//! Native entry point: parses CLI flags, loads datasets, then exports HTML, serves, or opens the GUI.
//! Run `cargo run -- --help` for usage.

use smorgasprot_app::config::{AppConfig, StartupConfig};
use smorgasprot_app::export::{generate_viewer_html, server};
use smorgasprot_app::loader::{self, DatasetSpec};
use smorgasprot_app::state::RenderBackend;
use smorgasprot_app::{SmorgasprotApp, dataset::ViewerDataset};
use smorgasprot_core::{AnalysisParams, ComplexStructure};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
Usage: cargo run -- [FILE.cif|FILE.pdb] [OPTIONS]

Without a file, the bundled examples in ./examples are loaded.

Options:
  -i, --cif, --pdb FILE        Structure file to load
  -a, -b, --group-a, --binder  Chain for interface group A (default: A)
  -t, --group-b, --target      Chain for interface group B (default: C)
  -c, --config FILE            App config file (.ron)
  -o, --out FILE.html          HTML output path (default: viewer.html)
      --export-html [FILE]     Write the HTML report and exit (no GUI)
      --headless               Same as --export-html
      --serve                  Write the HTML report and serve it with live recompute on :7878
  -h, --help                   Show this help";

/// Parsed command-line options.
struct Args {
    input: Option<String>,
    chain_a: Option<String>,
    chain_b: Option<String>,
    config_path: Option<PathBuf>,
    out_path: String,
    export_only: bool,
    serve: bool,
}

impl Args {
    /// Parses `std::env::args`; `Ok(None)` means help was requested.
    fn parse() -> Result<Option<Self>, String> {
        let mut args = Self {
            input: None,
            chain_a: None,
            chain_b: None,
            config_path: None,
            out_path: "viewer.html".to_owned(),
            export_only: false,
            serve: false,
        };
        let mut it = std::env::args().skip(1).peekable();
        while let Some(arg) = it.next() {
            let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} requires a value"));
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "-i" | "--cif" | "--pdb" => args.input = Some(value(&arg)?),
                "-a" | "-b" | "--group-a" | "--binder" => args.chain_a = Some(value(&arg)?),
                "-t" | "--group-b" | "--target" => args.chain_b = Some(value(&arg)?),
                "-c" | "--config" => args.config_path = Some(PathBuf::from(value(&arg)?)),
                "-o" | "--out" => args.out_path = value(&arg)?,
                "--export-html" => {
                    args.export_only = true;
                    // Optional path argument.
                    if let Some(path) = it.next_if(|next| is_html(next)) {
                        args.out_path = path;
                    }
                }
                "--headless" => args.export_only = true,
                "--serve" => args.serve = true,
                flag if flag.starts_with('-') => return Err(format!("Unknown option: {flag}")),
                path if is_html(path) => args.out_path = path.to_owned(),
                path => args.input = Some(path.to_owned()),
            }
        }
        Ok(Some(args))
    }

    /// Datasets to load: the user's file, or the bundled examples.
    fn specs(&self) -> Vec<DatasetSpec> {
        match &self.input {
            Some(path) => {
                let a = self.chain_a.as_deref().unwrap_or("A");
                let b = self.chain_b.as_deref().unwrap_or("C");
                vec![DatasetSpec::custom(path, a, b)]
            }
            None => DatasetSpec::builtin_examples(),
        }
    }
}

fn is_html(path: &str) -> bool {
    path.ends_with(".html") || path.ends_with(".htm")
}

fn main() -> ExitCode {
    let args = match Args::parse() {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("Error: {e}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    let loaded_cfg = AppConfig::load(args.config_path.as_deref());
    if let Some(ref warn) = loaded_cfg.warning {
        eprintln!("Warning: {warn}");
    }

    let specs = args.specs();
    if specs.is_empty() && (args.export_only || args.serve) {
        eprintln!("No structure given and no bundled examples found.\n\n{USAGE}");
        return ExitCode::FAILURE;
    }

    let (datasets, complexes) = match load_all(&specs, &loaded_cfg.config.analysis) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = if args.export_only || args.serve {
        export(&args, &datasets, complexes)
    } else {
        run_gui(datasets, loaded_cfg)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Loads every spec, printing a short interface summary for each.
fn load_all(
    specs: &[DatasetSpec],
    analysis: &AnalysisParams,
) -> Result<(Vec<ViewerDataset>, HashMap<String, ComplexStructure>), String> {
    let mut datasets = Vec::with_capacity(specs.len());
    let mut complexes = HashMap::new();
    for spec in specs {
        println!("Loading {} ...", spec.title);
        let loaded = loader::load(spec, analysis)?;
        let s = loaded.dataset.active_stats();
        let (a_at, a_ter, b_at, b_ter) = s.map_or((0, 0, 0, 0), |s| (s.side_a_atoms, s.side_a_territories, s.side_b_atoms, s.side_b_territories));
        println!(
            "  -> Loaded {} total chains; active interface has {} Group A atoms ({} territories) & {} Group B atoms ({} territories)",
            loaded.complex.chains.len(),
            a_at,
            a_ter,
            b_at,
            b_ter
        );
        let (intra, cross) = s.map_or((0, 0), |s| (s.intra_edges, s.cross_contacts));
        println!("  -> 1st-shell edges: {}, cross-interface contacts: {}", intra, cross);
        let (rmse_b, rmse_a, ov_b, ov_a) = s.map_or((0.0, 0.0, 0, 0), |s| (s.edge_rmse_before_a, s.edge_rmse_after_a, s.overlaps_before, s.overlaps_after));
        println!(
            "  -> Relaxation reduced edge RMSE: {:.2} Å -> {:.2} Å | 2D overlaps: {} -> {}",
            rmse_b, rmse_a, ov_b, ov_a
        );
        complexes.insert(spec.id.clone(), loaded.complex);
        datasets.push(loaded.dataset);
    }
    Ok((datasets, complexes))
}

/// Writes the HTML report and, with `--serve`, keeps serving it.
fn export(args: &Args, datasets: &[ViewerDataset], complexes: HashMap<String, ComplexStructure>) -> Result<(), String> {
    let html = generate_viewer_html(datasets)?;
    std::fs::write(&args.out_path, &html).map_err(|e| format!("Failed to write {}: {e}", args.out_path))?;
    let abs = std::fs::canonicalize(&args.out_path).unwrap_or_else(|_| args.out_path.clone().into());
    println!("\nGenerated interactive HTML5 Canvas viewer: {}", abs.display());
    println!("Open in your browser: file://{}", abs.display());

    if args.serve {
        server::serve(server::DEFAULT_PORT, html, complexes)?;
    }
    Ok(())
}

/// Opens the egui desktop window.
fn run_gui(datasets: Vec<ViewerDataset>, loaded_cfg: StartupConfig) -> Result<(), String> {
    println!("\nLaunching SmörgåsProt desktop application (egui)...");
    let renderer = match loaded_cfg.config.rendering.backend {
        RenderBackend::Auto | RenderBackend::Wgpu => eframe::Renderer::Wgpu,
        RenderBackend::Glow => eframe::Renderer::Glow,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("SmörgåsProt"),
        // Custom scene meshes are not feathered by egui, so MSAA smooths their edges.
        multisampling: loaded_cfg.config.rendering.multisampling,
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "SmörgåsProt",
        options,
        Box::new(|cc| Ok(Box::new(SmorgasprotApp::new(cc, datasets, loaded_cfg)))),
    )
    .map_err(|e| format!("Failed to run the desktop app: {e}"))
}
