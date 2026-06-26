//! Shared test fixtures: synthetic plugins, throwaway instances, and profile
//! helpers, so unit tests, integration tests, and dependent crates don't each
//! re-roll them.
//!
//! Available to in-crate tests (`cfg(test)`) and, for integration tests and
//! other crates, behind the `test-support` feature.

use crate::instance::{Instance, ModKind, ModListEntry, Profile};
use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;

/// TES4 header flag: master file.
pub const FLAG_MASTER: u32 = 0x1;
/// TES4 header flag: light (ESL) plugin (Fallout 4 / Skyrim SE).
pub const FLAG_LIGHT: u32 = 0x200;

/// Build the bytes of a minimal but valid Fallout 4 plugin: a single `TES4` header
/// record containing an `HEDR` subrecord and one `MAST`/`DATA` pair per master.
/// Enough for esplugin's header-only parse to read flags and masters.
pub fn tes4_bytes(flags: u32, masters: &[&str]) -> Vec<u8> {
    // Subrecord data block first, so we can compute the record's data size.
    let mut data = Vec::new();

    // HEDR: version (f32) + num records (i32) + next object id (u32) = 12 bytes.
    data.extend_from_slice(b"HEDR");
    data.extend_from_slice(&12u16.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());

    // One MAST (null-terminated name) + DATA (u64) per master.
    for m in masters {
        let mut name = m.as_bytes().to_vec();
        name.push(0);
        data.extend_from_slice(b"MAST");
        data.extend_from_slice(&(name.len() as u16).to_le_bytes());
        data.extend_from_slice(&name);
        data.extend_from_slice(b"DATA");
        data.extend_from_slice(&8u16.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
    }

    // 24-byte TES4 record header: sig, data size, flags, form id, vcs, version, unknown.
    let mut out = Vec::new();
    out.extend_from_slice(b"TES4");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Write a generated plugin file into `dir` (creating it if needed) and return its path.
pub fn write_plugin(dir: &Utf8Path, name: &str, flags: u32, masters: &[&str]) -> Utf8PathBuf {
    std::fs::create_dir_all(dir).expect("create mod dir");
    let path = dir.join(name);
    std::fs::write(&path, tes4_bytes(flags, masters)).expect("write plugin");
    path
}

/// A throwaway temp directory and its UTF-8 root path.
pub fn temp() -> (TempDir, Utf8PathBuf) {
    let dir = TempDir::new().expect("temp dir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
    (dir, root)
}

/// A throwaway instance whose `mods/` and `game/` share one volume (so hardlinks
/// work) and whose `Plugins.txt` is redirected to a temp dir, never the real
/// `%LOCALAPPDATA%`.
pub fn temp_instance() -> (TempDir, Instance) {
    let (dir, root) = temp();
    let mut instance = Instance::new(root.join("instance"), root.join("game"));
    instance.config.local_dir = Some(root.join("local"));
    (dir, instance)
}

/// Write `contents` to `path`, creating parent directories first.
pub fn write(path: &Utf8Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parents");
    }
    std::fs::write(path, contents).expect("write file");
}

/// Create a mod folder under `mods/` holding the given relative files and contents.
pub fn install_mod(instance: &Instance, name: &str, files: &[(&str, &str)]) {
    for (rel, contents) in files {
        let path = instance.mods_dir().join(name).join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write file");
    }
}

/// Stage a single valid Fallout 4 plugin (no flags) inside a mod's folder.
pub fn install_plugin(instance: &Instance, mod_name: &str, plugin: &str) {
    write_plugin(&instance.mods_dir().join(mod_name), plugin, 0, &[]);
}

/// Save a profile (highest priority first) so loaders can read it from disk.
pub fn save_profile(instance: &Instance, name: &str, mods: &[(&str, bool)]) {
    let profile = Profile {
        name: name.to_owned(),
        mods: mods
            .iter()
            .map(|(n, enabled)| ModListEntry {
                name: (*n).to_owned(),
                enabled: *enabled,
                kind: ModKind::Managed,
            })
            .collect(),
    };
    profile.save(instance).expect("save profile");
}
