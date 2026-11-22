//! Minimal local HTTP server for `--serve`: hosts the HTML report and recomputes maps on demand.
//! Lets the frozen JS viewer reassign chain groups via `POST /api/recompute` (native only).

use crate::dataset::{AtomMeta, InterfaceViewData};
use serde::{Deserialize, Serialize};
use smorgasprot_core::{
    AnalysisParams, ChainName, ComplexStructure, InterfaceMap, flatten::FlatteningStats,
    interface::InterfacePartition,
};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// Default port for `--serve`.
pub const DEFAULT_PORT: u16 = 7878;
/// Maximum request size read from a single connection (bytes).
const REQUEST_BUF_LEN: usize = 16384;

/// JSON body of `POST /api/recompute` (sent by the JS viewer).
#[derive(Deserialize)]
struct RecomputeRequest {
    dataset_id: String,
    group_a_name: String,
    group_b_name: String,
    group_a_chains: Vec<ChainName>,
    group_b_chains: Vec<ChainName>,
}

/// JSON reply to `POST /api/recompute`; field names are part of the JS contract.
#[derive(Serialize)]
struct RecomputeResponse {
    map: InterfaceMap,
    atom_meta: Vec<AtomMeta>,
    stats: FlatteningStats,
}

/// Serves `html` on `127.0.0.1:port` forever; `complexes` are keyed by dataset id.
pub fn serve(port: u16, html: String, mut complexes: HashMap<String, ComplexStructure>) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).map_err(|e| format!("Failed to bind {addr}: {e}"))?;
    println!("Live Rust interface server listening on http://{addr}");

    for mut stream in listener.incoming().flatten() {
        let mut buf = [0u8; REQUEST_BUF_LEN];
        let n = match stream.read(&mut buf) {
            Ok(n) if n > 0 => n,
            _ => continue,
        };
        let request = String::from_utf8_lossy(&buf[..n]);

        if request.starts_with("POST /api/recompute") {
            match recompute(&request, &mut complexes) {
                Some(json) => respond(&mut stream, "200 OK", "application/json", &json),
                None => respond(&mut stream, "400 Bad Request", "text/plain", ""),
            }
        } else {
            respond(&mut stream, "200 OK", "text/html; charset=utf-8", &html);
        }
    }
    Ok(())
}

/// Parses a recompute request, reassigns groups, and returns the JSON reply.
fn recompute(request: &str, complexes: &mut HashMap<String, ComplexStructure>) -> Option<String> {
    let body = &request[request.find("\r\n\r\n")? + 4..];
    let req: RecomputeRequest = serde_json::from_str(body).ok()?;
    let complex = complexes.get_mut(&req.dataset_id)?;

    let partition = InterfacePartition::from_chains(
        complex,
        &req.dataset_id,
        &req.group_a_name,
        req.group_a_chains,
        &req.group_b_name,
        req.group_b_chains,
    );
    let params = AnalysisParams::default();
    let iface = InterfaceViewData::compute(complex, partition, &[], &params).ok()?;
    serde_json::to_string(&RecomputeResponse {
        map: iface.map,
        atom_meta: iface.atom_meta,
        stats: iface.stats,
    })
    .ok()
}

/// Writes a complete HTTP/1.1 response; write errors are ignored (client went away).
fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n", body.len());
    let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(body.as_bytes()));
}
