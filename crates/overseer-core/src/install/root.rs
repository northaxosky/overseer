//! Locating the content root inside an extracted archive

use super::error::InstallError;
use crate::error::io_err;
use camino::{Utf8Path, Utf8PathBuf};

/// Top-level directory names that mark a valid Bethesda game data root
const DATA_DIRS: &[&str] = &[
    "textures",
    "meshes",
    "sound",
    "music",
    "video",
    "scripts",
    "materials",
    "interface",
    "strings",
    "f4se",
    "skse",
    "sfse",
    "mcm",
    "vis",
    "lodsettings",
    "terrain",
    "grass",
    "shadersfx",
];

/// File extensions that mark a valid data root (plugins and archives)
const DATA_EXTS: &[&str] = &["esp", "esm", "esl", "ba2", "bsa"];

/// A top-level entry, reduced to what is really needed
struct Entry {
    name: String,
    is_dir: bool,
}

/// The decision for one level of the tree
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// This directory is the content root
    Here,
    /// Descend into the named subdirectory and re-evaluate
    Into(String),
}

/// Cap on wrapper levels to descend so a pathologically nested archive can't recurse unbounded
const MAX_DESCENT_DEPTH: usize = 8;

/// Detect the content root inside an archive: the dir whose contents should become staging files
pub fn find_content_root(extracted: &Utf8Path) -> Result<Utf8PathBuf, InstallError> {
    let mut current = extracted.to_owned();
    for _ in 0..MAX_DESCENT_DEPTH {
        let entries = read_entries(&current)?;
        match classify(&entries) {
            Step::Here => return Ok(current),
            Step::Into(name) => current = current.join(name),
        }
    }
    Ok(current)
}

/// Decide whether a directory's entries are already the data root or we should descend
fn classify(entries: &[Entry]) -> Step {
    if entries.iter().any(is_indicator) {
        return Step::Here;
    }
    let dirs: Vec<&Entry> = entries.iter().filter(|e| e.is_dir).collect();

    // A lone wrapper directory => descend
    if let [only] = dirs.as_slice() {
        return Step::Into(only.name.clone());
    }

    // Multiple top level dirs, but one is `Data/` => descend
    if let Some(data) = dirs.iter().find(|e| e.name.eq_ignore_ascii_case("data")) {
        return Step::Into(data.name.clone());
    }
    Step::Here
}

/// Determines if this entry signals a valid content root
fn is_indicator(entry: &Entry) -> bool {
    if entry.is_dir {
        entry.name.eq_ignore_ascii_case("root")
            || DATA_DIRS.iter().any(|d| entry.name.eq_ignore_ascii_case(d))
    } else {
        Utf8Path::new(&entry.name)
            .extension()
            .is_some_and(|ext| DATA_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
    }
}

fn read_entries(dir: &Utf8Path) -> Result<Vec<Entry>, InstallError> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| io_err(dir, e))? {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        let is_dir = entry.file_type().map_err(|e| io_err(dir, e))?.is_dir();
        let name = entry.file_name().to_string_lossy().into_owned();
        entries.push(Entry { name, is_dir });
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "tests/root.rs"]
mod tests;
