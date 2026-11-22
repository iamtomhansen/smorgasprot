//! Secondary structure assignment: PDB/mmCIF header parsing (`_struct_conf`, `_struct_sheet_range`,
//! `HELIX`, `SHEET`), coordinate-based Kabsch & Sander (1983) DSSP hydrogen-bond patterns, and
//! Lovell et al. (2003) Ramachandran backbone dihedral contours.
//!
//! ## Works referenced
//! 1. **DSSP Electrostatic H-Bond & Turn/Bridge Assignment:**
//!    Kabsch, W., & Sander, C. (1983). Dictionary of protein secondary structure: Pattern
//!    recognition of hydrogen-bonded and geometrical features. *Biopolymers*, 22(12), 2577–2637.
//! 2. **Ramachandran $(\phi, \psi)$ Contours:**
//!    Lovell, S. C., Davis, I. W., Arendall, W. B., de Bakker, P. I. W., Word, J. M., Prisant, M. G.,
//!    Richardson, J. S., & Richardson, D. C. (2003). Structure validation by $C\alpha$ geometry:
//!    $\phi,\psi$ and $C\beta$ deviation. *Proteins: Structure, Function, and Bioinformatics*, 50(3), 437–450.

use crate::model::{ComplexStructure, SecStruct};
use glam::Vec3;
use std::fs;
use std::path::Path;

/// Controls whether secondary structure is read from PDB/mmCIF header records or computed from 3D coordinates.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SecStructMode {
    /// TODO: Only use explicit `_struct_conf` / `_struct_sheet_range` (mmCIF) or `HELIX` / `SHEET` (PDB) records
    FromFile,
    /// Fall back to coordinate-based DSSP if file does not assigned secondary structure (e.g. de novo models).
    FromFileOrComputeDssp,
    /// Force compute secondary structure from 3D coordinates (Kabsch & Sander (1983) DSSP + Lovell (2003) Ramachandran contours).
    ComputeDssp,
}

/// Coordinates in a backbone residue used in secondary structure assignment.
#[derive(Clone, Copy)]
struct BackboneFrame {
    n: Vec3,
    ca: Vec3,
    c: Vec3,
    o: Option<Vec3>,
    /// Reconstructed ideal amide hydrogen position (1.01 A antiparallel to preceding C=O)
    h: Option<Vec3>,
}

/// Assigns backbone $(\phi, \psi)$ angles and [`SecStruct`] (`Helix`, `Sheet`, `Loop`) to all
/// polypeptide residues in `complex` according to `mode`.
pub fn assign_secondary_structure(
    complex: &mut ComplexStructure,
    source_path: Option<&Path>,
    mode: SecStructMode,
) {
    let frames = compute_backbone_phi_psi_and_frames(complex);

    if mode == SecStructMode::FromFileOrComputeDssp
        && let Some(path) = source_path
        && try_parse_file_secondary_structure(path, complex)
    {
        return;
    }

    compute_dssp_from_frames(complex, &frames);
}

/// Extracts backbone frames, computes $(\phi, \psi)$ dihedral angles on `complex.residues`,
/// and reconstructs ideal amide hydrogen coordinates for non-Proline residues.
fn compute_backbone_phi_psi_and_frames(
    complex: &mut ComplexStructure,
) -> Vec<Option<BackboneFrame>> {
    let n_res = complex.residues.len();
    let mut frames: Vec<Option<BackboneFrame>> = Vec::with_capacity(n_res);

    for res in &complex.residues {
        if !res.kind.is_polypeptide() {
            frames.push(None);
            continue;
        }
        let mut n_pos = None;
        let mut ca_pos = None;
        let mut c_pos = None;
        let mut o_pos = None;
        for a_idx in res.atom_range.0..res.atom_range.1 {
            let atom = &complex.atoms[a_idx as usize];
            match &atom.atom_name {
                b"N   " => n_pos = Some(Vec3::from(atom.pos_3d)),
                b"CA  " => ca_pos = Some(Vec3::from(atom.pos_3d)),
                b"C   " => c_pos = Some(Vec3::from(atom.pos_3d)),
                b"O   " => o_pos = Some(Vec3::from(atom.pos_3d)),
                _ => {}
            }
        }
        match (n_pos, ca_pos, c_pos) {
            (Some(n), Some(ca), Some(c)) => frames.push(Some(BackboneFrame {
                n,
                ca,
                c,
                o: o_pos,
                h: None,
            })),
            _ => frames.push(None),
        }
    }

    for i in 0..n_res {
        let Some(frame_i) = frames[i] else {
            continue;
        };

        let same_chain_prev = i > 0
            && complex.residues[i - 1].chain_idx == complex.residues[i].chain_idx
            && (complex.residues[i].seq_num - complex.residues[i - 1].seq_num).abs() <= 1;

        let same_chain_next = i + 1 < n_res
            && complex.residues[i + 1].chain_idx == complex.residues[i].chain_idx
            && (complex.residues[i + 1].seq_num - complex.residues[i].seq_num).abs() <= 1;

        let phi = if same_chain_prev {
            frames[i - 1].map(|prev| dihedral_deg(prev.c, frame_i.n, frame_i.ca, frame_i.c))
        } else {
            None
        };

        let psi = if same_chain_next {
            frames[i + 1].map(|next| dihedral_deg(frame_i.n, frame_i.ca, frame_i.c, next.n))
        } else {
            None
        };

        if let (Some(p), Some(q)) = (phi, psi) {
            complex.residues[i].backbone_phi_psi = Some((p, q));
        }

        let is_proline = &complex.residues[i].res_name == b"PRO";
        if !is_proline
            && same_chain_prev
            && let Some(prev) = frames[i - 1]
            && let Some(o_prev) = prev.o
        {
            let co_dir = (o_prev - prev.c).normalize_or_zero();
            if co_dir.length_squared() > 0.5
                && let Some(f_mut) = &mut frames[i]
            {
                f_mut.h = Some(frame_i.n - 1.01 * co_dir);
            }
        }
    }

    frames
}

/// Parse secondary structure spans from mmCIF (`_struct_conf`, `_struct_sheet_range`)
/// or PDB (`HELIX`, `SHEET`) files.
/// Returns `true` if at least one secondary structure span was found.
fn try_parse_file_secondary_structure(path: &Path, complex: &mut ComplexStructure) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };

    // Reset all `ComplexStructure` residues to Loop before applying spans from file.
    for res in &mut complex.residues {
        res.sec_struct = SecStruct::Loop;
    }

    let mut spans: Vec<(String, i32, i32, SecStruct)> = Vec::new();
    let is_cif = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cif"));

    if is_cif {
        parse_mmcif_sec_struct_spans(&content, &mut spans);
    } else {
        parse_pdb_sec_struct_spans(&content, &mut spans);
    }

    if spans.is_empty() {
        return false;
    }

    let mut any_matched = false;
    for (chain_id, start_seq, end_seq, ss) in spans {
        let (lo, hi) = if start_seq <= end_seq {
            (start_seq, end_seq)
        } else {
            (end_seq, start_seq)
        };
        for res in &mut complex.residues {
            if res.chain_id == chain_id && res.seq_num >= lo && res.seq_num <= hi {
                res.sec_struct = ss;
                any_matched = true;
            }
        }
    }

    any_matched
}

fn parse_mmcif_sec_struct_spans(content: &str, spans: &mut Vec<(String, i32, i32, SecStruct)>) {
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed == "loop_" {
            let mut headers = Vec::new();
            while let Some(&next_line) = lines.peek() {
                let nl = next_line.trim();
                if nl.starts_with('_') {
                    headers.push(nl.to_string());
                    lines.next();
                } else {
                    break;
                }
            }

            let is_conf = headers.iter().any(|h| h.starts_with("_struct_conf."));
            let is_sheet = headers
                .iter()
                .any(|h| h.starts_with("_struct_sheet_range."));

            if is_conf || is_sheet {
                let prefix = if is_conf {
                    "_struct_conf."
                } else {
                    "_struct_sheet_range."
                };
                let find_col = |name: &str| -> Option<usize> {
                    let target = format!("{prefix}{name}");
                    headers.iter().position(|h| h == &target)
                };

                let beg_chain_col =
                    find_col("beg_auth_asym_id").or_else(|| find_col("beg_label_asym_id"));
                let beg_seq_col =
                    find_col("beg_auth_seq_id").or_else(|| find_col("beg_label_seq_id"));
                let end_seq_col =
                    find_col("end_auth_seq_id").or_else(|| find_col("end_label_seq_id"));
                let conf_type_col = find_col("conf_type_id");

                while let Some(&row_line) = lines.peek() {
                    let rl = row_line.trim();
                    if rl.is_empty()
                        || rl.starts_with('#')
                        || rl.starts_with("loop_")
                        || rl.starts_with('_')
                    {
                        break;
                    }
                    lines.next();
                    let tokens = tokenize_cif_row(rl);
                    if let (Some(c_col), Some(b_col), Some(e_col)) =
                        (beg_chain_col, beg_seq_col, end_seq_col)
                        && tokens.len() > c_col.max(b_col).max(e_col)
                    {
                        let ss = if is_sheet {
                            SecStruct::Sheet
                        } else if let Some(t_col) = conf_type_col {
                            if tokens
                                .get(t_col)
                                .is_some_and(|t| t.starts_with("HELX") || t.starts_with("helx"))
                            {
                                SecStruct::Helix
                            } else {
                                continue;
                            }
                        } else {
                            SecStruct::Helix
                        };

                        if let (Ok(s0), Ok(s1)) =
                            (tokens[b_col].parse::<i32>(), tokens[e_col].parse::<i32>())
                        {
                            spans.push((tokens[c_col].to_string(), s0, s1, ss));
                        }
                    }
                }
            }
        }
    }
}

fn tokenize_cif_row(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'\'' || bytes[i] == b'"' {
            let quote = bytes[i];
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            out.push(&line[start..i]);
            if i < bytes.len() {
                i += 1;
            }
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(&line[start..i]);
        }
    }
    out
}

fn parse_pdb_sec_struct_spans(content: &str, spans: &mut Vec<(String, i32, i32, SecStruct)>) {
    for line in content.lines() {
        if line.starts_with("HELIX ") && line.len() >= 37 {
            let chain = line[19..20].trim().to_string();
            if let (Ok(s0), Ok(s1)) = (
                line[21..25].trim().parse::<i32>(),
                line[33..37].trim().parse::<i32>(),
            ) {
                spans.push((chain, s0, s1, SecStruct::Helix));
            }
        } else if line.starts_with("SHEET ") && line.len() >= 37 {
            let chain = line[21..22].trim().to_string();
            if let (Ok(s0), Ok(s1)) = (
                line[22..26].trim().parse::<i32>(),
                line[33..37].trim().parse::<i32>(),
            ) {
                spans.push((chain, s0, s1, SecStruct::Sheet));
            }
        }
    }
}

/// Evaluates Kabsch & Sander (1983) DSSP hydrogen-bond patterns and Lovell et al. (2003) Ramachandran contours.
fn compute_dssp_from_frames(complex: &mut ComplexStructure, frames: &[Option<BackboneFrame>]) {
    let n_res = complex.residues.len();

    let is_hbonded = |donor_nh: usize, acceptor_co: usize| -> bool {
        if donor_nh == acceptor_co {
            return false;
        }
        if complex.residues[donor_nh].chain_idx == complex.residues[acceptor_co].chain_idx
            && (complex.residues[donor_nh].seq_num - complex.residues[acceptor_co].seq_num).abs()
                <= 2
        {
            return false;
        }
        let (Some(d), Some(a)) = (frames[donor_nh], frames[acceptor_co]) else {
            return false;
        };
        let (Some(h_pos), Some(o_pos)) = (d.h, a.o) else {
            return false;
        };
        if d.n.distance_squared(o_pos) > 27.0 {
            return false;
        }
        let r_on = o_pos.distance(d.n).max(0.5);
        let r_ch = a.c.distance(h_pos).max(0.5);
        let r_oh = o_pos.distance(h_pos).max(0.5);
        let r_cn = a.c.distance(d.n).max(0.5);

        let energy_kcal = 27.888 * (1.0 / r_on + 1.0 / r_ch - 1.0 / r_oh - 1.0 / r_cn);
        energy_kcal < -0.5
    };

    let mut turn4 = vec![false; n_res];
    let mut turn3 = vec![false; n_res];
    for i in 0..n_res {
        if i + 4 < n_res
            && complex.residues[i].chain_idx == complex.residues[i + 4].chain_idx
            && is_hbonded(i + 4, i)
        {
            turn4[i] = true;
        }
        if i + 3 < n_res
            && complex.residues[i].chain_idx == complex.residues[i + 3].chain_idx
            && is_hbonded(i + 3, i)
        {
            turn3[i] = true;
        }
    }

    let mut assigned = vec![SecStruct::Loop; n_res];

    for i in 1..(n_res.saturating_sub(3)) {
        if turn4[i - 1] && turn4[i] {
            for r in assigned.iter_mut().take(i + 4).skip(i) {
                *r = SecStruct::Helix;
            }
        }
    }
    for i in 1..(n_res.saturating_sub(2)) {
        if turn3[i - 1] && turn3[i] {
            for r in assigned.iter_mut().take(i + 3).skip(i) {
                if *r == SecStruct::Loop {
                    *r = SecStruct::Helix;
                }
            }
        }
    }

    for i in 1..(n_res.saturating_sub(1)) {
        if assigned[i] == SecStruct::Helix || frames[i].is_none() {
            continue;
        }
        for j in 1..(n_res.saturating_sub(1)) {
            if (i as isize - j as isize).abs() <= 2 || frames[j].is_none() {
                continue;
            }
            let antiparallel = (is_hbonded(i, j) && is_hbonded(j, i))
                || (is_hbonded(i + 1, j - 1) && is_hbonded(j + 1, i - 1));
            let parallel = (is_hbonded(i + 1, j) && is_hbonded(j, i - 1))
                || (is_hbonded(j + 1, i) && is_hbonded(i, j - 1));

            if antiparallel || parallel {
                assigned[i] = SecStruct::Sheet;
                break;
            }
        }
    }

    for i in 0..n_res {
        if assigned[i] != SecStruct::Loop {
            complex.residues[i].sec_struct = assigned[i];
        } else if let Some((phi, psi)) = complex.residues[i].backbone_phi_psi {
            complex.residues[i].sec_struct = classify_ramachandran_lovell(phi, psi);
        }
    }
}

/// TODO: Use an actual 2D lookup table from Lovell
/// Lovell et al. (2003) Top500 core Ramachandran region fallback (*Proteins* 50:437–450).
fn classify_ramachandran_lovell(phi: f32, psi: f32) -> SecStruct {
    let d_alpha = ((phi + 63.0) / 38.0).powi(2) + ((psi + 43.0) / 35.0).powi(2);
    if d_alpha <= 1.0 {
        return SecStruct::Helix;
    }

    let psi_wrapped = if psi < -135.0 { psi + 360.0 } else { psi };
    let d_beta = ((phi + 118.0) / 55.0).powi(2) + ((psi_wrapped - 135.0) / 48.0).powi(2);
    if d_beta <= 1.0 {
        return SecStruct::Sheet;
    }

    SecStruct::Loop
}

/// Calculates the signed IUPAC dihedral angle (in degrees, `[-180, 180]`) defined by four 3D points.
pub fn dihedral_deg(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3) -> f32 {
    let b0 = p0 - p1;
    let b1 = p2 - p1;
    let b2 = p3 - p2;
    let b1_norm = b1.normalize_or_zero();
    let v = b0 - b0.dot(b1_norm) * b1_norm;
    let w = b2 - b2.dot(b1_norm) * b1_norm;
    let x = v.dot(w);
    let y = b1_norm.cross(v).dot(w);
    y.atan2(x).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::load_complex;
    use std::path::Path;

    #[test]
    fn test_file_header_and_computed_dssp() {
        let mut c_7jvb =
            load_complex("../../examples/7JVB.cif", &["A".into()], &["C".into()])
                .unwrap();
        let file_helices = c_7jvb
            .residues
            .iter()
            .filter(|r| r.sec_struct == SecStruct::Helix)
            .count();
        let file_sheets = c_7jvb
            .residues
            .iter()
            .filter(|r| r.sec_struct == SecStruct::Sheet)
            .count();
        assert!(file_helices > 10);
        assert!(file_sheets > 50);

        // Manually recompute via coordinate-based DSSP
        assign_secondary_structure(&mut c_7jvb, None, SecStructMode::ComputeDssp);
        let dssp_helices = c_7jvb
            .residues
            .iter()
            .filter(|r| r.sec_struct == SecStruct::Helix)
            .count();
        let dssp_sheets = c_7jvb
            .residues
            .iter()
            .filter(|r| r.sec_struct == SecStruct::Sheet)
            .count();
        assert!(dssp_helices > 10);
        assert!(dssp_sheets > 50);

        // De novo protein structure has no _struct_conf header -> automatically uses coordinate DSSP
        let c_lx = load_complex(
            Path::new("../../examples/fold_r8s15_mp3_model_0.cif"),
            &["B".into()],
            &["A".into()],
        )
        .unwrap();
        let lx_structured = c_lx
            .residues
            .iter()
            .filter(|r| r.sec_struct != SecStruct::Loop)
            .count();
        assert!(lx_structured > 30);
    }
}
