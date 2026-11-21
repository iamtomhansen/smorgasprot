//! Turns a structure file plus chain grouping into a render-ready `ViewerDataset`.
//! Runs the full core pipeline (parse → group → interface map → surfaces); expensive, call once per load.

use crate::dataset::ViewerDataset;
use smorgasprot_core::{
    AnalysisParams, ComplexStructure, flatten::build_interface_map, ingest::load_complex, ingest::set_active_groups,
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
    pub group_a_chains: Vec<String>,
    pub group_b_chains: Vec<String>,
}

impl DatasetSpec {
    /// A user-supplied file with one chain per side.
    pub fn custom(path: &str, chain_a: &str, chain_b: &str) -> Self {
        Self {
            id: path.to_owned(),
            title: format!("{path} (Group A: {chain_a} vs Group B: {chain_b})"),
            path: path.to_owned(),
            group_a_chains: vec![chain_a.to_owned()],
            group_b_chains: vec![chain_b.to_owned()],
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
                group_a_chains: vec![a.to_owned()],
                group_b_chains: vec![b.to_owned()],
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
pub fn load(spec: &DatasetSpec, params: &AnalysisParams) -> Result<Loaded, String> {
    let mut complex = load_complex(&spec.path, &spec.group_a_chains, &spec.group_b_chains)
        .map_err(|e| format!("Failed to load {}: {e}", spec.path))?;
    set_active_groups(&mut complex, GROUP_A_NAME, &spec.group_a_chains, GROUP_B_NAME, &spec.group_b_chains);

    let (map, stats) =
        build_interface_map(&complex, params).map_err(|e| format!("Failed to extract and flatten interface: {e}"))?;
    let dataset = ViewerDataset::from_complex_and_map(&spec.id, &spec.title, &complex, map, stats, params);
    Ok(Loaded { dataset, complex })
}
