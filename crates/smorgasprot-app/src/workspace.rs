//! The open structures: each keeps its source text, chain groups, analysis parameters, and derived data.
//! Everything needed to recompute or save a structure lives here, so sessions are self-contained.

use crate::dataset::ViewerDataset;
use serde::{Deserialize, Serialize};
use smorgasprot_core::{AnalysisParams, ComplexStructure, InterfaceSide};

/// Default display names for the two interface groups.
pub const DEFAULT_GROUP_A_NAME: &str = "Group A";
pub const DEFAULT_GROUP_B_NAME: &str = "Group B";

/// A structure file's name and decoded text; the text is embedded in saved sessions.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StructureSource {
    /// File name used for format detection and display, e.g. `7JVB.cif`.
    pub file_name: String,
    /// Path it was read from, if any (informational; reloading uses `text`).
    #[serde(default)]
    pub path: Option<String>,
    /// Decoded PDB or mmCIF text.
    pub text: String,
}

/// Names and chain membership of the two interface sides.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GroupAssignment {
    pub a_name: String,
    pub b_name: String,
    pub a_chains: Vec<String>,
    pub b_chains: Vec<String>,
}

impl GroupAssignment {
    /// Groups with default names.
    pub fn new(a_chains: Vec<String>, b_chains: Vec<String>) -> Self {
        Self { a_name: DEFAULT_GROUP_A_NAME.into(), b_name: DEFAULT_GROUP_B_NAME.into(), a_chains, b_chains }
    }

    /// First chain versus all others; `None` if there are fewer than two chains.
    pub fn auto(chain_ids: &[String]) -> Option<Self> {
        let (first, rest) = chain_ids.split_first()?;
        (!rest.is_empty()).then(|| Self::new(vec![first.clone()], rest.to_vec()))
    }

    pub fn side_of(&self, chain_id: &str) -> InterfaceSide {
        if self.a_chains.iter().any(|c| c == chain_id) {
            InterfaceSide::SideA
        } else if self.b_chains.iter().any(|c| c == chain_id) {
            InterfaceSide::SideB
        } else {
            InterfaceSide::None
        }
    }

    /// Moves `chain_id` to `side`, removing it from the other group.
    pub fn set_side(&mut self, chain_id: &str, side: InterfaceSide) {
        self.a_chains.retain(|c| c != chain_id);
        self.b_chains.retain(|c| c != chain_id);
        match side {
            InterfaceSide::SideA => self.a_chains.push(chain_id.to_owned()),
            InterfaceSide::SideB => self.b_chains.push(chain_id.to_owned()),
            InterfaceSide::None => {}
        }
    }

    /// Both sides need at least one chain to define an interface.
    pub fn is_valid(&self) -> bool {
        !self.a_chains.is_empty() && !self.b_chains.is_empty()
    }

    /// True if chain membership differs (names are display-only and don't need a recompute).
    pub fn chains_differ(&self, other: &Self) -> bool {
        let sorted = |v: &[String]| {
            let mut v = v.to_vec();
            v.sort();
            v
        };
        sorted(&self.a_chains) != sorted(&other.a_chains) || sorted(&self.b_chains) != sorted(&other.b_chains)
    }
}

/// One open structure and everything derived from it.
pub struct LoadedStructure {
    pub id: String,
    pub title: String,
    pub source: StructureSource,
    /// Groups that produced `dataset`.
    pub groups: GroupAssignment,
    /// Parameters that produced `dataset` (a snapshot, for reproducibility).
    pub params: AnalysisParams,
    /// Groups being edited in the UI; applied by a recompute.
    pub draft: GroupAssignment,
    /// Parsed structure, kept for the live recompute server and future editing.
    pub complex: ComplexStructure,
    pub dataset: ViewerDataset,
}

impl LoadedStructure {
    /// Chain IDs in file order.
    pub fn chain_ids(&self) -> impl Iterator<Item = &str> {
        self.complex.chains.iter().map(|c| c.chain_id.as_str())
    }

    /// Copies draft group names into the rendered dataset (no recompute needed).
    pub fn sync_group_names(&mut self) {
        self.dataset.group_a_name.clone_from(&self.draft.a_name);
        self.dataset.group_b_name.clone_from(&self.draft.b_name);
        self.groups.a_name.clone_from(&self.draft.a_name);
        self.groups.b_name.clone_from(&self.draft.b_name);
    }
}

/// All open structures and which one is shown.
#[derive(Default)]
pub struct Workspace {
    pub structures: Vec<LoadedStructure>,
    pub active: usize,
}

impl Workspace {
    pub fn new(structures: Vec<LoadedStructure>) -> Self {
        Self { structures, active: 0 }
    }

    pub fn active(&self) -> Option<&LoadedStructure> {
        self.structures.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut LoadedStructure> {
        self.structures.get_mut(self.active)
    }

    /// Rendered datasets in order (for HTML export and the dataset picker).
    pub fn datasets(&self) -> impl Iterator<Item = &ViewerDataset> {
        self.structures.iter().map(|s| &s.dataset)
    }

    /// Appends structures and shows the first new one.
    pub fn append(&mut self, new: Vec<LoadedStructure>) {
        if !new.is_empty() {
            self.active = self.structures.len();
            self.structures.extend(new);
        }
    }

    /// Removes a structure, keeping `active` in range.
    pub fn close(&mut self, index: usize) {
        if index < self.structures.len() {
            self.structures.remove(index);
            if self.active >= index && self.active > 0 {
                self.active -= 1;
            }
        }
    }
}
