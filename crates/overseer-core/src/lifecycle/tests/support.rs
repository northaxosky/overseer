//! Shared fixtures for lifecycle remove tests

use std::io::Write;

use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;

use super::*;
use crate::instance::Instance;
use crate::test_support::temp_instance;

/// Create a throwaway instance without touching a real game
pub(super) fn instance() -> (TempDir, Instance) {
    temp_instance()
}

/// Install a small tree under one actual mod name
pub(super) fn install_tree(instance: &Instance, name: &str) {
    let path = installed_file(instance, name);
    std::fs::create_dir_all(path.parent().expect("tree parent")).expect("create mod tree");
    std::fs::write(path, "mod bytes").expect("write mod tree");
}

/// Return the fixed content path used by lifecycle tree fixtures
pub(super) fn installed_file(instance: &Instance, name: &str) -> Utf8PathBuf {
    instance.mods_dir().join(name).join("nested/file.txt")
}

/// Build a zip archive from path and byte pairs
pub(super) fn make_zip(path: &Utf8Path, entries: &[(&str, &[u8])]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create archive parent");
    }
    let file = std::fs::File::create(path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for &(name, bytes) in entries {
        zip.start_file(name, options).expect("start zip entry");
        zip.write_all(bytes).expect("write zip entry");
    }
    zip.finish().expect("finish zip");
}

/// Create one direct Downloads zip archive
pub(super) fn download_zip(
    instance: &Instance,
    name: &str,
    entries: &[(&str, &[u8])],
) -> Utf8PathBuf {
    let path = instance.downloads_dir().join(name);
    make_zip(&path, entries);
    path
}

/// Write exact profile modlist text
pub(super) fn write_modlist(instance: &Instance, profile: &str, text: &str) -> Utf8PathBuf {
    let path = modlist_path(instance, profile);
    std::fs::create_dir_all(path.parent().expect("modlist parent")).expect("create profile");
    std::fs::write(&path, text).expect("write modlist");
    path
}

/// Return one profile's modlist path
pub(super) fn modlist_path(instance: &Instance, profile: &str) -> Utf8PathBuf {
    instance.profile_dir(profile).join("modlist.txt")
}

/// Read one profile's exact modlist text
pub(super) fn read_modlist(instance: &Instance, profile: &str) -> String {
    std::fs::read_to_string(modlist_path(instance, profile)).expect("read modlist")
}

/// Return the fixed pending bundle path
pub(super) fn pending_path(instance: &Instance) -> Utf8PathBuf {
    bundle::path(instance)
}

/// Build one path-aware synthetic operation failure
pub(super) fn operation_failure(path: &Utf8Path) -> crate::IoError {
    crate::error::io_err(
        path,
        std::io::Error::other("injected lifecycle operation failure"),
    )
}

/// Assert that the installed test tree still has its original bytes
pub(super) fn assert_live_tree(instance: &Instance, name: &str) {
    let path = installed_file(instance, name);
    assert_eq!(
        std::fs::read_to_string(path).expect("read mod tree"),
        "mod bytes"
    );
}
