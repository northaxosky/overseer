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

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> Entry {
        Entry {
            name: name.to_owned(),
            is_dir: true,
        }
    }

    fn file(name: &str) -> Entry {
        Entry {
            name: name.to_owned(),
            is_dir: false,
        }
    }

    // --- classify (pure) ---

    #[test]
    fn recognized_folder_means_here() {
        // Case A: already a valid data layout
        let entries = [dir("Textures"), dir("Meshes"), file("MyMod.esp")];
        assert_eq!(classify(&entries), Step::Here);
    }

    #[test]
    fn recognized_plugin_file_means_here() {
        // A lone plugin with no recognized folder is still a data root
        assert_eq!(classify(&[file("MyMod.esp")]), Step::Here);
        assert_eq!(classify(&[file("Textures.ba2")]), Step::Here);
    }

    #[test]
    fn folder_and_extension_matching_is_case_insensitive() {
        assert_eq!(classify(&[dir("TEXTURES")]), Step::Here);
        assert_eq!(classify(&[file("Mod.ESP")]), Step::Here);
    }

    #[test]
    fn single_wrapper_directory_descends() {
        // Case B: one wrapper folder (the mod name)
        assert_eq!(classify(&[dir("MyMod")]), Step::Into("MyMod".to_owned()));
    }

    #[test]
    fn single_data_directory_descends() {
        // Case C arrives via the single-dir rule
        assert_eq!(classify(&[dir("Data")]), Step::Into("Data".to_owned()));
    }

    #[test]
    fn data_folder_among_several_descends_into_data() {
        // Step 3: multiple dirs, but one is Data/
        let entries = [dir("docs"), dir("Data"), file("readme.txt")];
        assert_eq!(classify(&entries), Step::Into("Data".to_owned()));
    }

    #[test]
    fn a_lone_wrapper_with_loose_junk_files_still_descends() {
        // Only one *directory*; the loose file isn't counted as a wrapper
        let entries = [dir("MyMod"), file("readme.txt")];
        assert_eq!(classify(&entries), Step::Into("MyMod".to_owned()));
    }

    #[test]
    fn ambiguous_variant_folders_fall_back_to_here() {
        // The "2K vs 4K" case: no confident root, so don't guess
        let entries = [dir("2K Textures"), dir("4K Textures"), file("readme.txt")];
        assert_eq!(classify(&entries), Step::Here);
    }

    #[test]
    fn empty_listing_falls_back_to_here() {
        assert_eq!(classify(&[]), Step::Here);
    }

    // --- find_content_root (real temp-dir trees) ---

    use crate::test_support::{temp, touch};

    #[test]
    fn flat_archive_root_is_the_extraction_dir() {
        let (_t, base) = temp();
        touch(&base.join("Textures/a.dds"));
        touch(&base.join("MyMod.esp"));
        assert_eq!(find_content_root(&base).unwrap(), base);
    }

    #[test]
    fn single_name_wrapper_is_stripped() {
        let (_t, base) = temp();
        touch(&base.join("MyMod/Textures/a.dds"));
        touch(&base.join("MyMod/MyMod.esp"));
        assert_eq!(find_content_root(&base).unwrap(), base.join("MyMod"));
    }

    #[test]
    fn data_wrapper_is_stripped() {
        let (_t, base) = temp();
        touch(&base.join("Data/Textures/a.dds"));
        touch(&base.join("Data/MyMod.esp"));
        assert_eq!(find_content_root(&base).unwrap(), base.join("Data"));
    }

    #[test]
    fn double_wrapper_descends_twice() {
        let (_t, base) = temp();
        touch(&base.join("Outer/Inner/Meshes/a.nif"));
        assert_eq!(
            find_content_root(&base).unwrap(),
            base.join("Outer").join("Inner")
        );
    }

    #[test]
    fn texture_only_mod_keeps_its_data_folder() {
        // The case the naive "strip one wrapper" rule would break
        let (_t, base) = temp();
        touch(&base.join("Textures/armor/a.dds"));
        assert_eq!(find_content_root(&base).unwrap(), base);
    }

    #[test]
    fn texture_only_mod_wrapped_strips_only_the_wrapper() {
        let (_t, base) = temp();
        touch(&base.join("Wrapper/Textures/armor/a.dds"));
        assert_eq!(find_content_root(&base).unwrap(), base.join("Wrapper"));
    }

    #[test]
    fn top_level_root_folder_is_a_content_root() {
        // A `Root/` deploy folder marks the content root, so it is never stripped
        assert_eq!(classify(&[dir("Root")]), Step::Here);
        assert_eq!(classify(&[dir("ROOT")]), Step::Here);
    }

    #[test]
    fn root_only_mod_keeps_its_root_folder() {
        let (_t, base) = temp();
        touch(&base.join("Root/f4se_loader.exe"));
        assert_eq!(find_content_root(&base).unwrap(), base);
    }

    #[test]
    fn wrapped_root_only_mod_strips_only_the_wrapper() {
        let (_t, base) = temp();
        touch(&base.join("MyMod/Root/dxgi.dll"));
        assert_eq!(find_content_root(&base).unwrap(), base.join("MyMod"));
    }

    #[test]
    fn descent_stops_at_the_depth_cap_on_pathological_nesting() {
        let (_t, base) = temp();
        // More single-wrapper levels than the cap allows, with the real data at the bottom
        let nested = (0..MAX_DESCENT_DEPTH + 3).fold(base.clone(), |p, _| p.join("w"));
        touch(&nested.join("Meshes/a.nif"));
        let capped = (0..MAX_DESCENT_DEPTH).fold(base.clone(), |p, _| p.join("w"));
        assert_eq!(find_content_root(&base).unwrap(), capped);
    }
}
