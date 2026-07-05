//! Tests for listing download archives

use super::*;
use crate::test_support::{set_mtime, temp_instance, touch};
use std::time::{Duration, SystemTime};

#[test]
fn missing_downloads_dir_is_an_empty_list() {
    let (_tmp, instance) = temp_instance();
    assert!(list_downloads(&instance).expect("list").is_empty());
}

#[test]
fn lists_supported_archives_sorted_ignoring_other_entries() {
    let (_tmp, instance) = temp_instance();
    let downloads = instance.downloads_dir();
    touch(&downloads.join("Zeta.zip"));
    touch(&downloads.join("alpha.7z"));
    touch(&downloads.join("readme.txt")); // not an archive
    std::fs::create_dir_all(downloads.join("Nested.zip")).expect("subdir"); // a dir, ignored

    let names: Vec<String> = list_downloads(&instance)
        .expect("list")
        .into_iter()
        .map(|e| e.name)
        .collect();
    // Case-insensitive sort puts `alpha.7z` before `Zeta.zip`; non-archives gone
    assert_eq!(names, ["alpha.7z", "Zeta.zip"]);
}

#[test]
fn installed_flag_tracks_the_mods_directory() {
    let (_tmp, instance) = temp_instance();
    touch(&instance.downloads_dir().join("CoolMod.zip"));
    touch(&instance.downloads_dir().join("Other.zip"));
    // A mods/<stem>/ folder marks the first archive as already installed
    std::fs::create_dir_all(instance.mods_dir().join("CoolMod")).expect("mkdir");

    let entries = list_downloads(&instance).expect("list");
    let installed: Vec<(&str, bool)> = entries
        .iter()
        .map(|e| (e.name.as_str(), e.installed))
        .collect();
    assert_eq!(installed, [("CoolMod.zip", true), ("Other.zip", false)]);
}

#[test]
fn entries_include_size_and_modified_time() {
    let (_tmp, instance) = temp_instance();
    let archive = instance.downloads_dir().join("Sized.zip");
    std::fs::create_dir_all(archive.parent().expect("parent")).expect("mkdir");
    std::fs::write(&archive, b"abc").expect("write");
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    set_mtime(&archive, modified);

    let entries = list_downloads(&instance).expect("list");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].size, 3);
    assert_eq!(entries[0].modified, modified);
}
