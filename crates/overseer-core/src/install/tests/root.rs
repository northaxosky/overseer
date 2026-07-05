//! Tests for locating a mod's content root

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
