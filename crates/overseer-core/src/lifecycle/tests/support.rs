//! Shared fixtures for lifecycle remove tests

use camino::Utf8PathBuf;
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
    let path = instance
        .mods_dir()
        .join(name)
        .join("nested")
        .join("file.txt");
    std::fs::create_dir_all(path.parent().expect("tree parent")).expect("create mod tree");
    std::fs::write(path, "mod bytes").expect("write mod tree");
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

/// Assert that the installed test tree still has its original bytes
pub(super) fn assert_live_tree(instance: &Instance, name: &str) {
    let path = instance
        .mods_dir()
        .join(name)
        .join("nested")
        .join("file.txt");
    assert_eq!(
        std::fs::read_to_string(path).expect("read mod tree"),
        "mod bytes"
    );
}
