//! File input/output: native dialogs (via `rfd`), drag-and-drop, and file-type detection.
//! Dropped files carry a path natively and raw bytes on the web; [`InputFile`] hides the difference.

use std::sync::Arc;

/// Extensions offered by the Open Structure dialog.
const STRUCTURE_EXTENSIONS: &[&str] = &["pdb", "ent", "cif", "mmcif", "gz"];
/// Session file suffix; kept in sync with `persistence::session::FILE_SUFFIX` (not
/// depended on directly since session persistence isn't wired up yet).
const FILE_SUFFIX: &str = ".smorgasprot.json";

/// What a file contains, judged by its name.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FileKind {
    Structure,
    Session,
    Unknown,
}

/// True if `lower` (an already-lowercased file name) looks like a PDB/mmCIF structure,
/// optionally gzipped (e.g. `foo.cif`, `foo.pdb.gz`).
fn is_structure_file_name(lower: &str) -> bool {
    let stripped = lower.strip_suffix(".gz").unwrap_or(lower);
    STRUCTURE_EXTENSIONS
        .iter()
        .filter(|ext| **ext != "gz")
        .any(|ext| stripped.ends_with(&format!(".{ext}")))
}

/// A file chosen by the user, by path (native) or by content (web drops).
#[derive(Clone, Debug)]
pub struct InputFile {
    pub name: String,
    pub path: Option<String>,
    pub bytes: Option<Arc<[u8]>>,
}

impl InputFile {
    pub fn from_path(path: impl Into<String>) -> Self {
        let path = path.into();
        let name = std::path::Path::new(&path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        Self { name, path: Some(path), bytes: None }
    }

    pub fn kind(&self) -> FileKind {
        let lower = self.name.to_ascii_lowercase();
        if lower.ends_with(FILE_SUFFIX) || lower.ends_with(".json") {
            FileKind::Session
        } else if is_structure_file_name(&lower) {
            FileKind::Structure
        } else {
            FileKind::Unknown
        }
    }

    /// Returns the file contents, reading from disk if needed (call off the UI thread).
    pub fn read(&self) -> Result<Arc<[u8]>, String> {
        if let Some(bytes) = &self.bytes {
            return Ok(bytes.clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &self.path {
            return std::fs::read(path).map(Arc::from).map_err(|e| format!("Failed to read {path}: {e}"));
        }
        Err(format!("{} has no readable contents", self.name))
    }
}

/// Takes files dropped onto the window this frame.
pub fn take_dropped(ctx: &egui::Context) -> Vec<InputFile> {
    ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files))
        .into_iter()
        .map(|f| {
            let path = f.path.as_ref().map(|p| p.to_string_lossy().into_owned());
            let name = match (&path, f.name.is_empty()) {
                (Some(p), true) => InputFile::from_path(p.clone()).name,
                _ => f.name,
            };
            InputFile { name, path, bytes: f.bytes }
        })
        .collect()
}

/// True while files are dragged over the window (for the drop overlay).
pub fn is_hovering_files(ctx: &egui::Context) -> bool {
    ctx.input(|i| !i.raw.hovered_files.is_empty())
}

/// Native file dialogs; they block the UI thread while open, which is acceptable for modal pickers.
#[cfg(not(target_arch = "wasm32"))]
pub mod dialogs {
    use super::{FILE_SUFFIX, InputFile, STRUCTURE_EXTENSIONS};
    use std::path::PathBuf;

    pub fn pick_structures() -> Vec<InputFile> {
        rfd::FileDialog::new()
            .set_title("Open Structure")
            .add_filter("PDB / mmCIF", STRUCTURE_EXTENSIONS)
            .add_filter("All files", &["*"])
            .pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|p| InputFile::from_path(p.to_string_lossy().into_owned()))
            .collect()
    }

    pub fn pick_session() -> Option<InputFile> {
        rfd::FileDialog::new()
            .set_title("Open Session")
            .add_filter("Smorgasprot session", &["json"])
            .pick_file()
            .map(|p| InputFile::from_path(p.to_string_lossy().into_owned()))
    }

    pub fn pick_session_save() -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Save Session")
            .add_filter("Smorgasprot session", &["json"])
            .set_file_name(format!("session{FILE_SUFFIX}"))
            .save_file()
    }

    pub fn pick_html_save() -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Export HTML Report")
            .add_filter("HTML", &["html"])
            .set_file_name("viewer.html")
            .save_file()
    }

    pub fn pick_png_save(default_name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Export Viewport PNG")
            .add_filter("PNG image", &["png"])
            .set_file_name(default_name)
            .save_file()
    }
}
