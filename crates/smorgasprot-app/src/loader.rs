//! Turns a structure file plus chain grouping into a render-ready `ViewerDataset`.
//! Runs the full core pipeline (parse → group → interface map → surfaces); expensive, call once per load.

use crate::dataset::ViewerDataset;
use smorgasprot_core::{
    AnalysisParams, ChainName, ComplexStructure, ingest::load_complex,
    interface::InterfacePartition,
};
use std::path::Path;

/// Default display names for the two interface groups.
const GROUP_A_NAME: &str = "Group A";
const GROUP_B_NAME: &str = "Group B";

/// What to load: a structure file and which chains form each side of the interface.
#[derive(Clone, Debug)]
pub struct DatasetSpec {
    pub id: String,
    pub title: String,
    pub path: String,
    pub group_a_chains: Vec<ChainName>,
    pub group_b_chains: Vec<ChainName>,
}

impl DatasetSpec {
    /// A user-supplied file with one chain per side.
    pub fn custom(path: &str, chain_a: &str, chain_b: &str) -> Self {
        Self {
            id: path.to_owned(),
            title: format!("{path} (Group A: {chain_a} vs Group B: {chain_b})"),
            path: path.to_owned(),
            group_a_chains: vec![ChainName::new(chain_a)],
            group_b_chains: vec![ChainName::new(chain_b)],
        }
    }

    /// A user-supplied structure file loaded as a bare complex with no initial interface partition.
    pub fn bare(path: &str) -> Self {
        Self {
            id: path.to_owned(),
            title: format!("{path} (Bare Complex)"),
            path: path.to_owned(),
            group_a_chains: Vec::new(),
            group_b_chains: Vec::new(),
        }
    }

    /// Bundled example structures that exist relative to the working directory.
    pub fn builtin_examples() -> Vec<Self> {
        let examples = [
            ("7JVB", "examples/7JVB.cif — 4-Chain Assembly (Chains A, C, B, D)", "examples/7JVB.cif", "A", "C"),
            (
                "LatentX",
                "examples/fold_r8s15_mp3_model_0.cif — 2-Chain Complex (Chains A, B)",
                "examples/fold_r8s15_mp3_model_0.cif",
                "B",
                "A",
            ),
        ];
        examples
            .into_iter()
            .filter(|(_, _, path, _, _)| Path::new(path).exists())
            .map(|(id, title, path, a, b)| Self {
                id: id.to_owned(),
                title: title.to_owned(),
                path: path.to_owned(),
                group_a_chains: vec![ChainName::new(a)],
                group_b_chains: vec![ChainName::new(b)],
            })
            .collect()
    }
}

/// A loaded dataset plus the parsed complex it came from (kept for live recomputation).
pub struct Loaded {
    pub dataset: ViewerDataset,
    pub complex: ComplexStructure,
}

/// Parses, groups, maps, and meshes one structure.
/// If no interface chains are requested (or the requested chains form no interface, e.g. a monomer),
/// loads the structure as a bare 3D complex with zero interfaces.
pub fn load(spec: &DatasetSpec, params: &AnalysisParams) -> Result<Loaded, String> {
    let complex = load_complex(&spec.path)
        .map_err(|e| format!("Failed to load {}: {e}", spec.path))?;
    let partitions = if spec.group_a_chains.is_empty() || spec.group_b_chains.is_empty() {
        Vec::new()
    } else {
        vec![InterfacePartition::from_chains(
            &complex,
            &spec.id,
            GROUP_A_NAME,
            spec.group_a_chains.clone(),
            GROUP_B_NAME,
            spec.group_b_chains.clone(),
        )]
    };

    let dataset = match ViewerDataset::new(&spec.id, &spec.title, &complex, partitions, params) {
        Ok(ds) => ds,
        Err(_) => ViewerDataset::new(&spec.id, &spec.title, &complex, Vec::new(), params)?,
    };
    Ok(Loaded { dataset, complex })
}
