//! Declarative selection algebra and atom query engine for molecular complexes.

use crate::model::{AtomRecord, ChainName, ComplexStructure, Element, GlobalAtomIdx, ResidueRecord};
use serde::{Deserialize, Serialize};
use std::ops::{BitAnd, BitOr, Not, Sub};

/// Matches fixed byte record names against trimmed or exact query strings.
#[inline]
fn match_name_bytes(record_bytes: &[u8], query: &str) -> bool {
    let q_bytes = query.as_bytes();
    if q_bytes == record_bytes {
        return true;
    }
    let end = record_bytes.iter().rposition(|&b| b != b' ').map_or(0, |i| i + 1);
    let start = record_bytes[..end].iter().position(|&b| b != b' ').unwrap_or(0);
    &record_bytes[start..end] == q_bytes
}

/// Declarative selection algebra for specifying atom subsets in a molecular complex.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub enum AtomSelection {
    /// Matches all atoms.
    All,
    /// Matches no atoms.
    #[default]
    None,
    /// Matches all atoms belonging to the specified chain ID.
    Chain(ChainName),
    /// Matches all atoms belonging to any of the specified chain IDs.
    Chains(Vec<ChainName>),
    /// Matches residues with sequence numbers in [start, end] on the specified chain.
    ResidueRange { chain: ChainName, start: i32, end: i32 },
    /// Matches residues with the specified sequence numbers on the specified chain.
    Residues { chain: ChainName, seq_nums: Vec<i32> },
    /// Matches residues with the specified 3-letter name (e.g. "ALA", "GLU").
    ResidueName(String),
    /// Matches atoms with the specified name (e.g. "CA", "CB").
    AtomName(String),
    /// Matches atoms of a specific chemical element.
    Element(Element),
    /// Matches standard protein backbone atoms (N, CA, C, O, OXT).
    Backbone,
    /// Matches non-backbone heavy atoms.
    Sidechain,
    /// Matches non-hydrogen atoms.
    Heavy,
    /// Explicit list of atom serial numbers (from source PDB/mmCIF, 1-based).
    AtomSerials(Vec<usize>),
    /// Explicit list of internal GlobalAtomIdx.
    ExplicitAtoms(Vec<GlobalAtomIdx>),
    /// Logical AND (intersection) of two selections.
    And(Box<AtomSelection>, Box<AtomSelection>),
    /// Logical OR (union) of two selections.
    Or(Box<AtomSelection>, Box<AtomSelection>),
    /// Logical NOT (complement) of a selection.
    Not(Box<AtomSelection>),
    /// Set difference: `self` AND NOT `other`.
    Difference(Box<AtomSelection>, Box<AtomSelection>),
}

impl AtomSelection {
    pub fn all() -> Self { Self::All }
    pub fn none() -> Self { Self::None }
    pub fn chain(name: impl Into<ChainName>) -> Self { Self::Chain(name.into()) }
    pub fn chains(names: impl IntoIterator<Item = impl Into<ChainName>>) -> Self {
        Self::Chains(names.into_iter().map(Into::into).collect())
    }
    pub fn residue_range(chain: impl Into<ChainName>, start: i32, end: i32) -> Self {
        Self::ResidueRange { chain: chain.into(), start, end }
    }
    pub fn residues(chain: impl Into<ChainName>, seq_nums: impl IntoIterator<Item = i32>) -> Self {
        Self::Residues { chain: chain.into(), seq_nums: seq_nums.into_iter().collect() }
    }
    pub fn residue_name(name: impl Into<String>) -> Self { Self::ResidueName(name.into()) }
    pub fn atom_name(name: impl Into<String>) -> Self { Self::AtomName(name.into()) }
    pub fn element(elem: Element) -> Self { Self::Element(elem) }
    pub fn backbone() -> Self { Self::Backbone }
    pub fn sidechain() -> Self { Self::Sidechain }
    pub fn heavy() -> Self { Self::Heavy }
    pub fn atom_serials(serials: impl IntoIterator<Item = usize>) -> Self {
        Self::AtomSerials(serials.into_iter().collect())
    }
    pub fn explicit(atoms: impl IntoIterator<Item = GlobalAtomIdx>) -> Self {
        Self::ExplicitAtoms(atoms.into_iter().collect())
    }

    #[must_use]
    pub fn and(self, other: Self) -> Self {
        Self::And(Box::new(self), Box::new(other))
    }

    #[must_use]
    pub fn or(self, other: Self) -> Self {
        Self::Or(Box::new(self), Box::new(other))
    }

    #[must_use]
    pub fn not(self) -> Self {
        Self::Not(Box::new(self))
    }

    #[must_use]
    pub fn difference(self, other: Self) -> Self {
        Self::Difference(Box::new(self), Box::new(other))
    }

    /// Evaluates whether an atom in its parent residue matches this selection.
    pub fn matches_atom(&self, atom: &AtomRecord, res: &ResidueRecord) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::Chain(c) => &res.chain_id == c,
            Self::Chains(cs) => cs.iter().any(|c| c == &res.chain_id),
            Self::ResidueRange { chain, start, end } => {
                &res.chain_id == chain && res.seq_num >= *start && res.seq_num <= *end
            }
            Self::Residues { chain, seq_nums } => {
                &res.chain_id == chain && seq_nums.contains(&res.seq_num)
            }
            Self::ResidueName(name) => match_name_bytes(&res.res_name, name),
            Self::AtomName(name) => match_name_bytes(&atom.atom_name, name),
            Self::Element(elem) => atom.element == *elem,
            Self::Backbone => {
                matches!(&atom.atom_name, b"N   " | b"CA  " | b"C   " | b"O   " | b"OXT ")
            }
            Self::Sidechain => {
                !matches!(&atom.atom_name, b"N   " | b"CA  " | b"C   " | b"O   " | b"OXT ")
            }
            Self::Heavy => atom.element != Element::H,
            Self::AtomSerials(serials) => serials.contains(&atom.atom_serial),
            Self::ExplicitAtoms(indices) => indices.contains(&atom.idx),
            Self::And(a, b) => a.matches_atom(atom, res) && b.matches_atom(atom, res),
            Self::Or(a, b) => a.matches_atom(atom, res) || b.matches_atom(atom, res),
            Self::Not(a) => !a.matches_atom(atom, res),
            Self::Difference(a, b) => a.matches_atom(atom, res) && !b.matches_atom(atom, res),
        }
    }

    /// Evaluates this selection across all atoms in `complex`, returning matching indices.
    pub fn select_indices(&self, complex: &ComplexStructure) -> Vec<GlobalAtomIdx> {
        let mut result = Vec::new();
        for atom in &complex.atoms {
            if let Some(res) = complex.residues.get(atom.residue_idx.0 as usize) {
                if self.matches_atom(atom, res) {
                    result.push(atom.idx);
                }
            }
        }
        result
    }

    /// Evaluates this selection across all atoms in `complex`, returning a boolean mask.
    pub fn select_mask(&self, complex: &ComplexStructure) -> Vec<bool> {
        let mut mask = vec![false; complex.atoms.len()];
        for (i, atom) in complex.atoms.iter().enumerate() {
            if let Some(res) = complex.residues.get(atom.residue_idx.0 as usize) {
                if self.matches_atom(atom, res) {
                    mask[i] = true;
                }
            }
        }
        mask
    }
}

impl BitAnd for AtomSelection {
    type Output = Self;
    #[inline]
    fn bitand(self, rhs: Self) -> Self::Output {
        self.and(rhs)
    }
}

impl BitOr for AtomSelection {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        self.or(rhs)
    }
}

impl Not for AtomSelection {
    type Output = Self;
    #[inline]
    fn not(self) -> Self::Output {
        AtomSelection::not(self)
    }
}

impl Sub for AtomSelection {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        self.difference(rhs)
    }
}
