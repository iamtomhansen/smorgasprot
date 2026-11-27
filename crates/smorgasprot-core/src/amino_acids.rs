//! Amino acid definitions, physicochemical properties, and PDB/mmCIF residue classification.
//! 
//! ## Summary
//! [`pdbtbx::Element`] provides most of the data, while this module provides convenience methods
//! and high-level classifications.
//! 
//! ## Considerations
//! Bondi VDW radii and Kyte-Doolittle hydropahty are de facto standards, but others exist.
//!
//! ## References
//! 1. Kyte, J., & Doolittle, R. F. (1982). A simple method for displaying the hydropathic
//!    character of a protein. https://doi.org/10.1016/0022-2836(82)90515-0
//! 2. Bondi, A. (1964). van der Waals Volumes and Radii. https://doi.org/10.1021/j100785a001
//!
//! ## TODO:
//! - This is not exhaustive... I should use the entire PDB chemical component dictionary
//! - These are extensive, flat structs. There's probably a clever way to generate them from a
//!   mmCIF dictionary or something.

use pdbtbx::Element;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Extension methods on [`pdbtbx::Element`] for biophysical classification and van der Waals radii.
pub trait ElementExt {
    /// Returns true if the element is hydrogen (`Element::H`).
    fn is_hydrogen_atom(self) -> bool;

    /// Returns true if the element is a non-hydrogen heavy atom.
    fn is_heavy_atom(self) -> bool;

    /// Returns true if the element is a polar hydrogen-bond donor/acceptor heavy atom (`N` or `O`).
    fn is_h_bond_partner(self) -> bool;

    /// Returns true if the element is a biologically common metal ion (`Zn`, `Mg`, `Ca`, `Fe`, `Cu`, `Co`, `Mn`, `Ni`, `Na`, `K`, etc.).
    fn is_metal_ion(self) -> bool;

    /// Returns the Bondi (1964) / Mantina (2009) van der Waals radius in Angstroms via `pdbtbx::Element::atomic_radius()`.
    fn vdw_radius_a(self) -> f32;

    /// Returns the single-bond covalent radius (A) for this element (`pdbtbx::Element::atomic_radius().covalent_single`).
    fn covalent_radius_a(self) -> f32;
}

impl ElementExt for Element {
    #[inline]
    fn is_hydrogen_atom(self) -> bool {
        self == Element::H
    }

    #[inline]
    fn is_heavy_atom(self) -> bool {
        self != Element::H
    }

    #[inline]
    fn is_h_bond_partner(self) -> bool {
        matches!(self, Element::N | Element::O)
    }

    #[inline]
    fn is_metal_ion(self) -> bool {
        matches!(
            self,
            Element::Li
                | Element::Na
                | Element::K
                | Element::Rb
                | Element::Cs
                | Element::Mg
                | Element::Ca
                | Element::Sr
                | Element::Ba
                | Element::V
                | Element::Cr
                | Element::Mn
                | Element::Fe
                | Element::Co
                | Element::Ni
                | Element::Cu
                | Element::Zn
                | Element::Mo
                | Element::Cd
                | Element::Pt
                | Element::Au
                | Element::Hg
        )
    }

    #[inline]
    fn vdw_radius_a(self) -> f32 {
        self.atomic_radius().van_der_waals.unwrap_or(1.75) as f32
    }

    #[inline]
    fn covalent_radius_a(self) -> f32 {
        self.atomic_radius().covalent_single as f32
    }
}

/// High-level chemical grouping for presentation lenses and mutation matrices.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChemistryClass {
    Aliphatic,
    Aromatic,
    PolarNeutral,
    Positive,
    Negative,
    SpecialGlyPro,
}

/// The 20 canonical amino acids.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AminoAcid {
    Ala,
    Arg,
    Asn,
    Asp,
    Cys,
    Gln,
    Glu,
    Gly,
    His,
    Ile,
    Leu,
    Lys,
    Met,
    Phe,
    Pro,
    Ser,
    Thr,
    Trp,
    Tyr,
    Val,
}

impl AminoAcid {
    /// All 20 canonical amino acids in alphabetical order of their 3-letter codes.
    pub const ALL: [Self; 20] = [
        Self::Ala,
        Self::Arg,
        Self::Asn,
        Self::Asp,
        Self::Cys,
        Self::Gln,
        Self::Glu,
        Self::Gly,
        Self::His,
        Self::Ile,
        Self::Leu,
        Self::Lys,
        Self::Met,
        Self::Phe,
        Self::Pro,
        Self::Ser,
        Self::Thr,
        Self::Trp,
        Self::Tyr,
        Self::Val,
    ];

    /// Parses a 3-letter amino acid string (e.g. `"GLU"` or `"Glu"`).
    pub fn from_three_letter(code: &str) -> Option<Self> {
        match code.trim().to_ascii_uppercase().as_str() {
            "ALA" => Some(Self::Ala),
            "ARG" => Some(Self::Arg),
            "ASN" => Some(Self::Asn),
            "ASP" => Some(Self::Asp),
            "CYS" => Some(Self::Cys),
            "GLN" => Some(Self::Gln),
            "GLU" => Some(Self::Glu),
            "GLY" => Some(Self::Gly),
            "HIS" | "HID" | "HIE" | "HIP" => Some(Self::His),
            "ILE" => Some(Self::Ile),
            "LEU" => Some(Self::Leu),
            "LYS" => Some(Self::Lys),
            "MET" => Some(Self::Met),
            "PHE" => Some(Self::Phe),
            "PRO" => Some(Self::Pro),
            "SER" => Some(Self::Ser),
            "THR" => Some(Self::Thr),
            "TRP" => Some(Self::Trp),
            "TYR" => Some(Self::Tyr),
            "VAL" => Some(Self::Val),
            _ => None,
        }
    }

    /// Parses a 3-byte ASCII array (e.g. `b"GLU"`).
    pub fn from_bytes(bytes: &[u8; 3]) -> Option<Self> {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(Self::from_three_letter)
    }

    /// 3-letter PDB/mmCIF code, upper-case (`"ALA"`, `"GLU"`, ...).
    pub const fn three_letter(self) -> &'static str {
        match self {
            Self::Ala => "ALA",
            Self::Arg => "ARG",
            Self::Asn => "ASN",
            Self::Asp => "ASP",
            Self::Cys => "CYS",
            Self::Gln => "GLN",
            Self::Glu => "GLU",
            Self::Gly => "GLY",
            Self::His => "HIS",
            Self::Ile => "ILE",
            Self::Leu => "LEU",
            Self::Lys => "LYS",
            Self::Met => "MET",
            Self::Phe => "PHE",
            Self::Pro => "PRO",
            Self::Ser => "SER",
            Self::Thr => "THR",
            Self::Trp => "TRP",
            Self::Tyr => "TYR",
            Self::Val => "VAL",
        }
    }

    /// Fixed 3-byte ASCII representation (`*b"ALA"`, `*b"GLU"`, ...).
    pub const fn three_letter_bytes(self) -> [u8; 3] {
        match self {
            Self::Ala => *b"ALA",
            Self::Arg => *b"ARG",
            Self::Asn => *b"ASN",
            Self::Asp => *b"ASP",
            Self::Cys => *b"CYS",
            Self::Gln => *b"GLN",
            Self::Glu => *b"GLU",
            Self::Gly => *b"GLY",
            Self::His => *b"HIS",
            Self::Ile => *b"ILE",
            Self::Leu => *b"LEU",
            Self::Lys => *b"LYS",
            Self::Met => *b"MET",
            Self::Phe => *b"PHE",
            Self::Pro => *b"PRO",
            Self::Ser => *b"SER",
            Self::Thr => *b"THR",
            Self::Trp => *b"TRP",
            Self::Tyr => *b"TYR",
            Self::Val => *b"VAL",
        }
    }

    /// Title-case 3-letter code for UI territory badges (`"Ala"`, `"Glu"`, ...).
    pub const fn title_case(self) -> &'static str {
        match self {
            Self::Ala => "Ala",
            Self::Arg => "Arg",
            Self::Asn => "Asn",
            Self::Asp => "Asp",
            Self::Cys => "Cys",
            Self::Gln => "Gln",
            Self::Glu => "Glu",
            Self::Gly => "Gly",
            Self::His => "His",
            Self::Ile => "Ile",
            Self::Leu => "Leu",
            Self::Lys => "Lys",
            Self::Met => "Met",
            Self::Phe => "Phe",
            Self::Pro => "Pro",
            Self::Ser => "Ser",
            Self::Thr => "Thr",
            Self::Trp => "Trp",
            Self::Tyr => "Tyr",
            Self::Val => "Val",
        }
    }

    /// Full chemical name of the canonical amino acid (`"Alanine"`, `"Glutamate"`, ...).
    pub const fn full_name(self) -> &'static str {
        match self {
            Self::Ala => "Alanine",
            Self::Arg => "Arginine",
            Self::Asn => "Asparagine",
            Self::Asp => "Aspartate",
            Self::Cys => "Cysteine",
            Self::Gln => "Glutamine",
            Self::Glu => "Glutamate",
            Self::Gly => "Glycine",
            Self::His => "Histidine",
            Self::Ile => "Isoleucine",
            Self::Leu => "Leucine",
            Self::Lys => "Lysine",
            Self::Met => "Methionine",
            Self::Phe => "Phenylalanine",
            Self::Pro => "Proline",
            Self::Ser => "Serine",
            Self::Thr => "Threonine",
            Self::Trp => "Tryptophan",
            Self::Tyr => "Tyrosine",
            Self::Val => "Valine",
        }
    }

    /// 1-letter IUPAC code (`'A'`, `'E'`, ...).
    pub const fn one_letter(self) -> char {
        match self {
            Self::Ala => 'A',
            Self::Arg => 'R',
            Self::Asn => 'N',
            Self::Asp => 'D',
            Self::Cys => 'C',
            Self::Gln => 'Q',
            Self::Glu => 'E',
            Self::Gly => 'G',
            Self::His => 'H',
            Self::Ile => 'I',
            Self::Leu => 'L',
            Self::Lys => 'K',
            Self::Met => 'M',
            Self::Phe => 'F',
            Self::Pro => 'P',
            Self::Ser => 'S',
            Self::Thr => 'T',
            Self::Trp => 'W',
            Self::Tyr => 'Y',
            Self::Val => 'V',
        }
    }

    /// Physicochemical sidechain class.
    pub const fn chemistry_class(self) -> ChemistryClass {
        match self {
            Self::Ala | Self::Val | Self::Leu | Self::Ile | Self::Met => ChemistryClass::Aliphatic,
            Self::Phe | Self::Tyr | Self::Trp => ChemistryClass::Aromatic,
            Self::Ser | Self::Thr | Self::Asn | Self::Gln | Self::Cys => {
                ChemistryClass::PolarNeutral
            }
            Self::Lys | Self::Arg | Self::His => ChemistryClass::Positive,
            Self::Asp | Self::Glu => ChemistryClass::Negative,
            Self::Gly | Self::Pro => ChemistryClass::SpecialGlyPro,
        }
    }

    /// Kyte & Doolittle (1982) hydropathy index (*J. Mol. Biol.* 157:105–132).
    pub const fn hydropathy_kyte_doolittle(self) -> f32 {
        match self {
            Self::Ile => 4.5,
            Self::Val => 4.2,
            Self::Leu => 3.8,
            Self::Phe => 2.8,
            Self::Cys => 2.5,
            Self::Met => 1.9,
            Self::Ala => 1.8,
            Self::Gly => -0.4,
            Self::Thr => -0.7,
            Self::Ser => -0.8,
            Self::Trp => -0.9,
            Self::Tyr => -1.3,
            Self::Pro => -1.6,
            Self::His => -3.2,
            Self::Glu => -3.5,
            Self::Gln => -3.5,
            Self::Asp => -3.5,
            Self::Asn => -3.5,
            Self::Lys => -3.9,
            Self::Arg => -4.5,
        }
    }

    /// Returns true if `atom_name` is a positively charged sidechain atom on this residue.
    pub fn is_positive_functional_atom(self, atom_name: &str) -> bool {
        let trimmed = atom_name.trim();
        match self {
            Self::Arg => matches!(trimmed, "NE" | "NH1" | "NH2"),
            Self::Lys => trimmed == "NZ",
            Self::His => matches!(trimmed, "ND1" | "NE2"),
            _ => false,
        }
    }

    /// Returns true if `atom_name` is a negatively charged sidechain atom on this residue.
    pub fn is_negative_functional_atom(self, atom_name: &str) -> bool {
        let trimmed = atom_name.trim();
        match self {
            Self::Asp => matches!(trimmed, "OD1" | "OD2"),
            Self::Glu => matches!(trimmed, "OE1" | "OE2"),
            _ => false,
        }
    }
}

impl FromStr for AminoAcid {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_three_letter(s).ok_or("Unknown canonical amino acid code")
    }
}

/// Classification of any residue/component encountered in a PDB or mmCIF file.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResidueKind {
    /// Standard canonical proteinogenic amino acid.
    CanonicalAminoAcid(AminoAcid),
    /// Non-standard / post-translationally modified amino acid (e.g. `MSE` selenomethionine, `SEP`, `TPO`, `PTR`, `CSO`).
    ModifiedAminoAcid(Option<AminoAcid>),
    /// Coordinating or structural metal ion (e.g. `ZN`, `MG`, `CA`).
    MetalIon,
    /// Non-protein ligand, cofactor, glycan, etc. (e.g. `HEM`, `NAG`, `ATP`).
    Ligand,
    /// Water or crystallization buffer ion/cryoprotectant (e.g. `HOH`, `PO4`, `GOL`).
    SolventOrBuffer,
}

impl ResidueKind {
    /// Classifies a PDB/mmCIF 3-letter component code and its constituent element profile.
    pub fn classify(res_name: &str, has_metal_element: bool) -> Self {
        let code = res_name.trim().to_ascii_uppercase();
        if let Some(aa) = AminoAcid::from_three_letter(&code) {
            return Self::CanonicalAminoAcid(aa);
        }

        // Common post-translational or crystallographic amino acid modifications
        match code.as_str() {
            "MSE" => return Self::ModifiedAminoAcid(Some(AminoAcid::Met)),
            "SEP" => return Self::ModifiedAminoAcid(Some(AminoAcid::Ser)),
            "TPO" => return Self::ModifiedAminoAcid(Some(AminoAcid::Thr)),
            "PTR" => return Self::ModifiedAminoAcid(Some(AminoAcid::Tyr)),
            "CSO" | "CME" | "CSS" | "CYX" => return Self::ModifiedAminoAcid(Some(AminoAcid::Cys)),
            "HYP" => return Self::ModifiedAminoAcid(Some(AminoAcid::Pro)),
            "MLY" => return Self::ModifiedAminoAcid(Some(AminoAcid::Lys)),
            "PCA" => return Self::ModifiedAminoAcid(Some(AminoAcid::Glu)),
            _ => {}
        }

        // Common waters, crystallization buffers, salts, and cryoprotectants
        if matches!(
            code.as_str(),
            "HOH"
                | "WAT"
                | "DOD"
                | "CAC"
                | "SO4"
                | "PO4"
                | "GOL"
                | "EDO"
                | "PEG"
                | "ACT"
                | "FMT"
                | "DMS"
                | "CL"
                | "BR"
                | "IOD"
                | "TRS"
                | "MES"
                | "EPE"
        ) {
            return Self::SolventOrBuffer;
        }

        if has_metal_element
            || matches!(
                code.as_str(),
                "ZN" | "MG" | "CA" | "FE" | "FE2" | "CU" | "CO" | "MN" | "NI" | "NA" | "K" | "CD"
            )
        {
            return Self::MetalIon;
        }

        Self::Ligand
    }

    /// Returns the associated canonical (or parent) [`AminoAcid`] if this residue is part of the polypeptide chain.
    pub const fn amino_acid(self) -> Option<AminoAcid> {
        match self {
            Self::CanonicalAminoAcid(aa) => Some(aa),
            Self::ModifiedAminoAcid(aa) => aa,
            _ => None,
        }
    }

    /// Returns true if this component is part of the polypeptide chain (canonical or modified amino acid).
    pub const fn is_polypeptide(self) -> bool {
        matches!(
            self,
            Self::CanonicalAminoAcid(_) | Self::ModifiedAminoAcid(_)
        )
    }

    /// Returns true if this component is water or a crystallization buffer artifact.
    pub const fn is_solvent_or_buffer(self) -> bool {
        matches!(self, Self::SolventOrBuffer)
    }
}

/// Resolves a 3-letter PDB/mmCIF residue or component code into a full descriptive name.
pub fn residue_full_name(res_name: &str) -> String {
    let code = res_name.trim().to_ascii_uppercase();
    if let Some(aa) = AminoAcid::from_three_letter(&code) {
        return aa.full_name().to_string();
    }
    match code.as_str() {
        // Modified amino acids
        "MSE" => "Selenomethionine".into(),
        "SEP" => "Phosphoserine".into(),
        "TPO" => "Phosphothreonine".into(),
        "PTR" => "Phosphotyrosine".into(),
        "CSO" => "S-Hydroxycysteine".into(),
        "CME" => "S-(2-Hydroxyethyl)-L-cysteine".into(),
        "HYP" => "4-Hydroxyproline".into(),
        "MLY" => "N-Dimethyl-lysine".into(),
        "PCA" => "Pyroglutamic acid".into(),
        // Common ligands, cofactors, and antibiotics
        "KEF" => "Cefuroxime".into(),
        "HEM" | "HEC" => "Heme".into(),
        "NAG" => "N-Acetyl-D-glucosamine".into(),
        "BMA" | "MAN" => "Mannose".into(),
        "FUC" => "Fucose".into(),
        "GAL" => "Galactose".into(),
        "GLC" => "Glucose".into(),
        "ATP" => "Adenosine triphosphate".into(),
        "ADP" => "Adenosine diphosphate".into(),
        "AMP" => "Adenosine monophosphate".into(),
        "GTP" => "Guanosine triphosphate".into(),
        "GDP" => "Guanosine diphosphate".into(),
        "NAD" => "Nicotinamide adenine dinucleotide".into(),
        "NAP" => "NADP+".into(),
        "FAD" => "Flavin adenine dinucleotide".into(),
        "FMN" => "Flavin mononucleotide".into(),
        "COA" => "Coenzyme A".into(),
        "SAM" => "S-Adenosylmethionine".into(),
        "SAH" => "S-Adenosylhomocysteine".into(),
        "PLP" => "Pyridoxal 5'-phosphate".into(),
        "TPP" => "Thiamine diphosphate".into(),
        "BTN" => "Biotin".into(),
        "RET" => "Retinal".into(),
        "CLR" => "Cholesterol".into(),
        "PLM" => "Palmitic acid".into(),
        // Metals and ions
        "ZN" => "Zinc ion".into(),
        "MG" => "Magnesium ion".into(),
        "CA" => "Calcium ion".into(),
        "FE" | "FE2" => "Iron ion".into(),
        "CU" => "Copper ion".into(),
        "CO" => "Cobalt ion".into(),
        "MN" => "Manganese ion".into(),
        "NI" => "Nickel ion".into(),
        "NA" => "Sodium ion".into(),
        "K" => "Potassium ion".into(),
        "CD" => "Cadmium ion".into(),
        _ => code,
    }
}

/// Formats any 3-byte residue name into Title Case (using a zero-allocation static string for canonical amino acids).
pub fn print_three_letter_code(res_name: &[u8; 3]) -> String {
    if let Some(aa) = AminoAcid::from_bytes(res_name) {
        return aa.title_case().to_string();
    }
    let s = std::str::from_utf8(res_name).unwrap_or("Unk").trim();
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::with_capacity(3);
            out.extend(first.to_uppercase());
            for c in chars {
                out.extend(c.to_lowercase());
            }
            out
        }
        None => "Unk".to_string(),
    }
}
