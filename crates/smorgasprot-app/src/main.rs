//! Smorgasprot interactive 2D/3D protein interface cartography & triage application.

pub mod app;
pub mod camera;
pub mod dataset;
pub mod export;
pub mod panels;
pub mod state;
pub mod style;
pub mod viewport;

use dataset::{AtomMeta, ViewerDataset};
use export::generate_viewer_html;
use serde::{Deserialize, Serialize};
use smorgasprot_core::{
    ComplexStructure,
    flatten::build_interface_map,
    ingest::{load_complex, set_active_groups},
};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

struct ComplexSpec {
    id: String,
    title: String,
    path: String,
    group_a_name: String,
    group_b_name: String,
    group_a_chains: Vec<String>,
    group_b_chains: Vec<String>,
}

#[derive(Deserialize)]
struct RecomputeRequest {
    dataset_id: String,
    group_a_name: String,
    group_b_name: String,
    group_a_chains: Vec<String>,
    group_b_chains: Vec<String>,
}

#[derive(Serialize)]
struct RecomputeResponse {
    map: smorgasprot_core::InterfaceMap,
    atom_meta: Vec<AtomMeta>,
    stats: smorgasprot_core::flatten::FlatteningStats,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut custom_cif: Option<String> = None;
    let mut group_a_chain: Option<String> = None;
    let mut group_b_chain: Option<String> = None;
    let mut out_path = "viewer.html".to_string();
    let mut serve_port: Option<u16> = None;
    let mut export_html_only = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--cif" | "--pdb" | "-i" if i + 1 < args.len() => {
                custom_cif = Some(args[i + 1].clone());
                i += 2;
            }
            "--group-a" | "--binder" | "-a" | "-b" if i + 1 < args.len() => {
                group_a_chain = Some(args[i + 1].clone());
                i += 2;
            }
            "--group-b" | "--target" | "-t" if i + 1 < args.len() => {
                group_b_chain = Some(args[i + 1].clone());
                i += 2;
            }
            "--export-html" => {
                export_html_only = true;
                if i + 1 < args.len() && (args[i + 1].ends_with(".html") || args[i + 1].ends_with(".htm")) {
                    out_path = args[i + 1].clone();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--headless" => {
                export_html_only = true;
                i += 1;
            }
            "--serve" => {
                serve_port = Some(7878);
                i += 1;
            }
            arg if !arg.starts_with('-') && custom_cif.is_none() => {
                if arg.ends_with(".html") || arg.ends_with(".htm") {
                    out_path = arg.to_string();
                } else {
                    custom_cif = Some(arg.to_string());
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let specs = if let Some(path) = custom_cif {
        let a = group_a_chain.unwrap_or_else(|| "A".to_string());
        let b = group_b_chain.unwrap_or_else(|| "C".to_string());
        vec![ComplexSpec {
            id: path.clone(),
            title: format!("{path} (Group A: {a} vs Group B: {b})"),
            path,
            group_a_name: "Group A".to_string(),
            group_b_name: "Group B".to_string(),
            group_a_chains: vec![a],
            group_b_chains: vec![b],
        }]
    } else {
        let mut defaults = Vec::new();
        if Path::new("examples/7JVB.cif").exists() {
            defaults.push(ComplexSpec {
                id: "7JVB".to_string(),
                title: "examples/7JVB.cif — 4-Chain Assembly (Chains A, C, B, D)".to_string(),
                path: "examples/7JVB.cif".to_string(),
                group_a_name: "Group A".to_string(),
                group_b_name: "Group B".to_string(),
                group_a_chains: vec!["A".to_string()],
                group_b_chains: vec!["C".to_string()],
            });
        }
        if Path::new("examples/fold_r8s15_mp3_model_0.cif").exists() {
            defaults.push(ComplexSpec {
                id: "LatentX".to_string(),
                title: "examples/fold_r8s15_mp3_model_0.cif — 2-Chain Complex (Chains A, B)"
                    .to_string(),
                path: "examples/fold_r8s15_mp3_model_0.cif".to_string(),
                group_a_name: "Group A".to_string(),
                group_b_name: "Group B".to_string(),
                group_a_chains: vec!["B".to_string()],
                group_b_chains: vec!["A".to_string()],
            });
        }
        defaults
    };

    if specs.is_empty() {
        eprintln!(
            "Usage: cargo run -p smorgasprot-app -- [path.cif] [--group-a A] [--group-b C] [--out viewer.html] [--export-html] [--serve]"
        );
        std::process::exit(1);
    }

    let mut datasets = Vec::with_capacity(specs.len());
    let mut loaded_complexes: HashMap<String, ComplexStructure> = HashMap::new();

    for spec in specs {
        println!("Loading {} ...", spec.title);
        let mut complex = load_complex(&spec.path, &spec.group_a_chains, &spec.group_b_chains)
            .expect("Failed to load complex");
        set_active_groups(
            &mut complex,
            &spec.group_a_name,
            &spec.group_a_chains,
            &spec.group_b_name,
            &spec.group_b_chains,
        );

        let (map, stats) =
            build_interface_map(&complex).expect("Failed to extract and flatten interface");

        println!(
            "  -> Loaded {} total chains; active interface has {} Group A atoms ({} territories) & {} Group B atoms ({} territories)",
            complex.chains.len(),
            stats.side_a_atoms,
            stats.side_a_territories,
            stats.side_b_atoms,
            stats.side_b_territories
        );
        println!(
            "  -> 1st-shell edges: {}, cross-interface contacts: {}",
            stats.intra_edges, stats.cross_contacts
        );
        println!(
            "  -> Relaxation reduced edge RMSE: {:.2} Å -> {:.2} Å | 2D overlaps: {} -> {}",
            stats.edge_rmse_before_a,
            stats.edge_rmse_after_a,
            stats.overlaps_before,
            stats.overlaps_after
        );

        let ds = ViewerDataset::from_complex_and_map(
            spec.id.clone(),
            spec.title,
            &complex,
            map,
            stats,
        );
        loaded_complexes.insert(spec.id, complex);
        datasets.push(ds);
    }

    if export_html_only || serve_port.is_some() {
        let html = generate_viewer_html(&datasets).expect("Failed to generate HTML viewer");
        fs::write(&out_path, &html).expect("Failed to write HTML viewer file");
        let abs_out = fs::canonicalize(&out_path).unwrap_or_else(|_| out_path.clone().into());
        println!(
            "\nGenerated interactive HTML5 Canvas viewer: {}",
            abs_out.display()
        );
        println!("Open in your browser: file://{}", abs_out.display());

        if let Some(port) = serve_port {
            run_live_rust_server(port, html, loaded_complexes);
        }
    } else {
        println!("\nLaunching Smorgasprot desktop application (egui)...");
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1400.0, 900.0])
                .with_min_inner_size([960.0, 600.0])
                .with_title("Smorgasprot — Macromolecular Interface Cartography"),
            ..Default::default()
        };
        let res = eframe::run_native(
            "Smorgasprot",
            native_options,
            Box::new(|_cc| Ok(Box::new(app::SmorgasprotApp::new(datasets)))),
        );
        if let Err(e) = res {
            eprintln!("Error running native Smorgasprot app: {e}");
        }
    }
}

fn run_live_rust_server(
    port: u16,
    html: String,
    mut complexes: HashMap<String, ComplexStructure>,
) {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).expect("Failed to bind local server port");
    println!("Live Rust interface server listening on http://{addr}");

    for stream in listener.incoming().flatten() {
        let mut stream = stream;
        let mut buf = [0u8; 16384];
        let Ok(n) = stream.read(&mut buf) else {
            continue;
        };
        if n == 0 {
            continue;
        }
        let req_str = String::from_utf8_lossy(&buf[..n]);

        if req_str.starts_with("POST /api/recompute") {
            if let Some(body_idx) = req_str.find("\r\n\r\n") {
                let body = &req_str[(body_idx + 4)..];
                if let Ok(req) = serde_json::from_str::<RecomputeRequest>(body)
                    && let Some(complex) = complexes.get_mut(&req.dataset_id)
                {
                    set_active_groups(
                        complex,
                        &req.group_a_name,
                        &req.group_a_chains,
                        &req.group_b_name,
                        &req.group_b_chains,
                    );
                    if let Ok((map, stats)) = build_interface_map(complex) {
                        let ds = ViewerDataset::from_complex_and_map(
                            &req.dataset_id,
                            "",
                            complex,
                            map,
                            stats,
                        );
                        let resp_json = serde_json::to_string(&RecomputeResponse {
                            map: ds.map,
                            atom_meta: ds.atom_meta,
                            stats: ds.stats,
                        })
                        .unwrap_or_default();
                        let http_resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            resp_json.len(),
                            resp_json
                        );
                        let _ = stream.write_all(http_resp.as_bytes());
                        continue;
                    }
                }
            }
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
        } else {
            let http_resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                html.len(),
                html
            );
            let _ = stream.write_all(http_resp.as_bytes());
        }
    }
}
