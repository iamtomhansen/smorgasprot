//! Declarative interface demarcation and atom-level group partitioning.

use crate::model::{ChainName, ComplexStructure, GlobalAtomIdx};
pub use crate::selection::AtomSelection;
use serde::{Deserialize, Serialize};

/// Which side of an interface partition a given chain, residue, or atom belongs to.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum InterfaceSide {
    SideA,
    SideB,
    #[default]
    None,
}

impl InterfaceSide {
    #[inline]
    pub const fn opposite(self) -> Self {
        match self {
            Self::SideA => Self::SideB,
            Self::SideB => Self::SideA,
            Self::None => Self::None,
        }
    }

    #[inline]
    pub const fn is_assigned(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// One rule by which an atom assigned to either side qualifies as interfacial.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq)]
pub enum InterfaceCriterion {
    /// Buries more than `min_a2` Å² of solvent-accessible surface upon binding (ΔSASA).
    BuriedSasa { min_a2: f32 },
    /// Lies within `cutoff_a` Å (atom center to atom center) of any opposite-side atom.
    Proximity { cutoff_a: f32 },
}

impl InterfaceCriterion {
    pub const DEFAULT_MIN_BURIED_A2: f32 = 0.15;
    pub const DEFAULT_PROXIMITY_A: f32 = 4.5;

    pub const fn buried_sasa() -> Self {
        Self::BuriedSasa { min_a2: Self::DEFAULT_MIN_BURIED_A2 }
    }

    pub const fn proximity() -> Self {
        Self::Proximity { cutoff_a: Self::DEFAULT_PROXIMITY_A }
    }

    /// Standard ΔSASA criterion, with proximity retaining boundary contacts whose burial is low.
    pub fn defaults() -> Vec<Self> {
        vec![Self::buried_sasa(), Self::proximity()]
    }

    #[inline]
    pub fn admits(self, buried_sasa_a2: f32, min_cross_dist_a: f32) -> bool {
        match self {
            Self::BuriedSasa { min_a2 } => buried_sasa_a2 > min_a2,
            Self::Proximity { cutoff_a } => min_cross_dist_a <= cutoff_a,
        }
    }
}

/// Complete demarcation of an interface partition between two groups of atoms.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InterfacePartition {
    /// User-facing name of the interface partition.
    pub name: String,
    /// User-facing name of the first group of atoms.
    pub group_a_name: String,
    /// User-facing name of the second group of atoms.
    pub group_b_name: String,
    /// The inclusion criteria for atoms in group A.
    pub rule_a: AtomSelection,
    /// The inclusion criteria for atoms in group B.
    pub rule_b: AtomSelection,
    /// An assigned atom is interfacial if it satisfies any of these.
    #[serde(default = "InterfaceCriterion::defaults")]
    pub criteria: Vec<InterfaceCriterion>,
    /// The indices of all atoms in group A.
    pub side_a_atoms: Vec<GlobalAtomIdx>,
    /// The indices of all atoms in group B.
    pub side_b_atoms: Vec<GlobalAtomIdx>,
    /// The chain IDs of all chains to which atoms in group A belong.
    pub group_a_chains: Vec<ChainName>,
    /// The chain IDs of all chains to which atoms in group B belong.
    pub group_b_chains: Vec<ChainName>,
}

impl InterfacePartition {
    pub fn new(
        name: impl Into<String>,
        group_a_name: impl Into<String>,
        group_a_chains: Vec<ChainName>,
        group_b_name: impl Into<String>,
        group_b_chains: Vec<ChainName>,
    ) -> Self {
        let rule_a = AtomSelection::chains(group_a_chains.clone());
        let rule_b = AtomSelection::chains(group_b_chains.clone());
        Self {
            name: name.into(),
            group_a_name: group_a_name.into(),
            group_b_name: group_b_name.into(),
            rule_a,
            rule_b,
            criteria: InterfaceCriterion::defaults(),
            side_a_atoms: Vec::new(),
            side_b_atoms: Vec::new(),
            group_a_chains,
            group_b_chains,
        }
    }

    /// Constructs an interface partition from whole chains and evaluates atomic membership on `complex`.
    pub fn from_chains(
        complex: &ComplexStructure,
        name: impl Into<String>,
        group_a_name: impl Into<String>,
        group_a_chains: Vec<ChainName>,
        group_b_name: impl Into<String>,
        group_b_chains: Vec<ChainName>,
    ) -> Self {
        let mut part = Self::new(name, group_a_name, group_a_chains, group_b_name, group_b_chains);
        part.recompute_membership(complex);
        part
    }

    /// Constructs an interface partition from selection rules and evaluates atomic membership on `complex`.
    pub fn from_rules(
        complex: &ComplexStructure,
        name: impl Into<String>,
        group_a_name: impl Into<String>,
        rule_a: AtomSelection,
        group_b_name: impl Into<String>,
        rule_b: AtomSelection,
    ) -> Self {
        let mut part = Self {
            name: name.into(),
            group_a_name: group_a_name.into(),
            group_b_name: group_b_name.into(),
            rule_a,
            rule_b,
            criteria: InterfaceCriterion::defaults(),
            side_a_atoms: Vec::new(),
            side_b_atoms: Vec::new(),
            group_a_chains: Vec::new(),
            group_b_chains: Vec::new(),
        };
        part.recompute_membership(complex);
        part
    }

    #[must_use]
    pub fn with_criteria(mut self, criteria: Vec<InterfaceCriterion>) -> Self {
        self.criteria = criteria;
        self
    }

    /// Whether an assigned atom with these metrics satisfies any of `criteria`.
    #[inline]
    pub fn is_interfacial(&self, buried_sasa_a2: f32, min_cross_dist_a: f32) -> bool {
        self.criteria.iter().any(|c| c.admits(buried_sasa_a2, min_cross_dist_a))
    }

    /// Largest proximity cutoff among `criteria`, bounding the cross-side distance search.
    pub fn max_proximity_cutoff_a(&self) -> Option<f32> {
        self.criteria
            .iter()
            .filter_map(|c| match *c {
                InterfaceCriterion::Proximity { cutoff_a } => Some(cutoff_a),
                InterfaceCriterion::BuriedSasa { .. } => None,
            })
            .reduce(f32::max)
    }

    /// Re-evaluates `rule_a` and `rule_b` against `complex`, updating `side_a_atoms` and `side_b_atoms`.
    pub fn recompute_membership(&mut self, complex: &ComplexStructure) {
        self.side_a_atoms = self.rule_a.select_indices(complex);
        self.side_b_atoms = self.rule_b.select_indices(complex);
    }

    /// Dense lookup table of `InterfaceSide` for every atom in `complex`.
    pub fn atom_sides(&self, complex: &ComplexStructure) -> Vec<InterfaceSide> {
        let mut sides = vec![InterfaceSide::None; complex.atoms.len()];
        if self.side_a_atoms.is_empty() && self.side_b_atoms.is_empty() {
            for (i, atom) in complex.atoms.iter().enumerate() {
                if let Some(res) = complex.residues.get(atom.residue_idx.0 as usize) {
                    if self.rule_a.matches_atom(atom, res) {
                        sides[i] = InterfaceSide::SideA;
                    } else if self.rule_b.matches_atom(atom, res) {
                        sides[i] = InterfaceSide::SideB;
                    }
                }
            }
        } else {
            for &idx in &self.side_a_atoms {
                if let Some(s) = sides.get_mut(idx.0 as usize) {
                    *s = InterfaceSide::SideA;
                }
            }
            for &idx in &self.side_b_atoms {
                if let Some(s) = sides.get_mut(idx.0 as usize) {
                    *s = InterfaceSide::SideB;
                }
            }
        }
        sides
    }

    /// User-facing display name of one interface side.
    pub fn group_name(&self, side: InterfaceSide) -> &str {
        match side {
            InterfaceSide::SideA => &self.group_a_name,
            InterfaceSide::SideB => &self.group_b_name,
            InterfaceSide::None => "Unassigned",
        }
    }

    /// Returns `InterfaceSide::SideA` if `chain_id` is in Group A, `SideB` if in Group B, or `None`.
    #[inline]
    pub fn chain_side(&self, chain_id: &str) -> InterfaceSide {
        if self.group_a_chains.iter().any(|c| c.as_str() == chain_id) {
            InterfaceSide::SideA
        } else if self.group_b_chains.iter().any(|c| c.as_str() == chain_id) {
            InterfaceSide::SideB
        } else {
            InterfaceSide::None
        }
    }

    /// Returns true if `chain_id` belongs to either side of the partition.
    #[inline]
    pub fn is_assigned(&self, chain_id: &str) -> bool {
        self.chain_side(chain_id).is_assigned()
    }

    /// Precomputes a dense lookup table of `InterfaceSide` for every chain in `complex`.
    #[inline]
    pub fn chain_sides(&self, complex: &ComplexStructure) -> Vec<InterfaceSide> {
        complex
            .chains
            .iter()
            .map(|c| self.chain_side(c.chain_id.as_str()))
            .collect()
    }
}
