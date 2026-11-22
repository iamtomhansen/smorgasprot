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

/// Complete demarcation of an interface partition between two groups of atoms.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
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
            side_a_atoms: Vec::new(),
            side_b_atoms: Vec::new(),
            group_a_chains: Vec::new(),
            group_b_chains: Vec::new(),
        };
        part.recompute_membership(complex);
        part
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
