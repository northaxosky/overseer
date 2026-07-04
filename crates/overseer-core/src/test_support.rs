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

/// Build minimal Fallout 4 plugin bytes with a `TES4` header and enough data for esplugin's header parse.
pub fn tes4_bytes(flags: u32, masters: &[&str]) -> Vec<u8> {
    tes4_bytes_versioned(flags, masters, 1.0)
}

/// Like [`tes4_bytes`], but writes a chosen `HEDR` module version so header-version fixtures can be built.
pub fn tes4_bytes_versioned(flags: u32, masters: &[&str], header_version: f32) -> Vec<u8> {
    // Subrecord data block first, so we can compute the record's data size.
    let mut data = Vec::new();

    // HEDR: version (f32) + num records (i32) + next object id (u32) = 12 bytes.
    data.extend_from_slice(b"HEDR");
    data.extend_from_slice(&12u16.to_le_bytes());
    data.extend_from_slice(&header_version.to_le_bytes());
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
    write_plugin_versioned(dir, name, flags, masters, 1.0)
}

/// Like [`write_plugin`], but stamps a chosen `HEDR` module version into the plugin.
pub fn write_plugin_versioned(
    dir: &Utf8Path,
    name: &str,
    flags: u32,
    masters: &[&str],
    header_version: f32,
) -> Utf8PathBuf {
    std::fs::create_dir_all(dir).expect("create mod dir");
    let path = dir.join(name);
    std::fs::write(&path, tes4_bytes_versioned(flags, masters, header_version))
        .expect("write plugin");
    path
}

/// A throwaway temp directory and its UTF-8 root path.
pub fn temp() -> (TempDir, Utf8PathBuf) {
    let dir = TempDir::new().expect("temp dir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
    (dir, root)
}

/// A throwaway same-volume instance with temp local/INI dirs, never real `%LOCALAPPDATA%` or `Documents\My Games`.
pub fn temp_instance() -> (TempDir, Instance) {
    let (dir, root) = temp();
    let mut instance = Instance::new(root.join("instance"), root.join("game"));
    instance.config.local_dir = Some(root.join("local"));
    instance.config.ini_dir = Some(root.join("my_games"));
    (dir, instance)
}

/// Write `contents` to `path`, creating parent directories first.
pub fn write(path: &Utf8Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parents");
    }
    std::fs::write(path, contents).expect("write file");
}

/// Build a throwaway `.zip` at `path` from `(entry path, bytes)` pairs for install/download tests.
pub fn write_zip(path: &Utf8Path, entries: &[(&str, &[u8])]) {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parents");
    }
    let file = std::fs::File::create(path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    for &(name, data) in entries {
        zip.start_file(name.to_owned(), opts).expect("start file");
        zip.write_all(data).expect("write entry");
    }
    zip.finish().expect("finish zip");
}

/// Build bytes for a valid, uncompressed Fallout 4 `.fos` header with the fields `parse_header` reads.
pub fn fos_bytes(
    save_number: u32,
    name: &str,
    level: u32,
    location: &str,
    game_date: &str,
) -> Vec<u8> {
    // A Bethesda wstring: a u16 LE *byte length*, then that many UTF-8 bytes.
    fn wstring(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u16).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    // Fields the header-size count covers: version onward, up to the game date.
    let mut header = Vec::new();
    header.extend_from_slice(&14u32.to_le_bytes()); // version, within FO4's 11..=15
    header.extend_from_slice(&save_number.to_le_bytes());
    wstring(&mut header, name);
    header.extend_from_slice(&level.to_le_bytes());
    wstring(&mut header, location);
    wstring(&mut header, game_date);

    let mut out = Vec::with_capacity(16 + header.len());
    out.extend_from_slice(b"FO4_SAVEGAME"); // 12-byte magic, no length prefix
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out
}

/// Write a synthetic `.fos` save (see [`fos_bytes`]) to `path`, creating parents.
pub fn write_fos(
    path: &Utf8Path,
    save_number: u32,
    name: &str,
    level: u32,
    location: &str,
    game_date: &str,
) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parents");
    }
    let bytes = fos_bytes(save_number, name, level, location, game_date);
    std::fs::write(path, bytes).expect("write fos");
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
        local_saves: false,
    };
    profile.save(instance).expect("save profile");
}

/// A 24-byte BA2 header (`BTDX` + version + tag + file_count 0 + name-table 0) then `body`.
pub fn ba2_bytes(version: u32, tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(24 + body.len());
    b.extend_from_slice(b"BTDX");
    b.extend_from_slice(&version.to_le_bytes());
    b.extend_from_slice(tag);
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u64.to_le_bytes());
    b.extend_from_slice(body);
    b
}

/// An in-memory `PluginMeta`, defaulting non-master/non-light with the given masters.
pub fn plugin_meta(
    name: &str,
    is_master: bool,
    is_light: bool,
    masters: &[&str],
) -> crate::plugins::PluginMeta {
    crate::plugins::PluginMeta {
        name: name.to_owned(),
        is_master,
        is_light,
        masters: masters.iter().map(|m| (*m).to_owned()).collect(),
        header_version: None,
    }
}

// Testbed: a declarative synthetic instance

/// One synthetic plugin: its filename, header flags, masters, and HEDR version.
struct PluginSpec {
    name: String,
    flags: u32,
    masters: Vec<String>,
    header_version: f32,
}

/// A synthetic mod's staged content: plugins plus files (loose assets or archives).
#[derive(Default)]
pub struct ModSpec {
    plugins: Vec<PluginSpec>,
    files: Vec<(String, Vec<u8>)>,
}

impl ModSpec {
    /// Add a plugin with the given flags and masters (HEDR version 1.0).
    pub fn plugin(self, name: &str, flags: u32, masters: &[&str]) -> Self {
        self.plugin_versioned(name, flags, masters, 1.0)
    }

    /// Add a plugin with a chosen HEDR module version, for header-version fixtures.
    pub fn plugin_versioned(
        mut self,
        name: &str,
        flags: u32,
        masters: &[&str],
        header_version: f32,
    ) -> Self {
        self.plugins.push(PluginSpec {
            name: name.to_owned(),
            flags,
            masters: masters.iter().map(|m| (*m).to_owned()).collect(),
            header_version,
        });
        self
    }

    /// Add a loose file at a mod-relative path with exact bytes.
    pub fn loose(mut self, rel: &str, bytes: &[u8]) -> Self {
        self.files.push((rel.to_owned(), bytes.to_vec()));
        self
    }

    /// Add a stub BA2 archive (24-byte `BTDX` header) at the mod root.
    pub fn archive(mut self, name: &str, version: u32, tag: &[u8; 4]) -> Self {
        self.files
            .push((name.to_owned(), ba2_bytes(version, tag, &[])));
        self
    }
}

/// A declarative synthetic instance: managed mods in priority order (first is highest) and a profile.
#[derive(Default)]
pub struct TestbedSpec {
    profile: String,
    mods: Vec<(String, bool, ModSpec)>,
}

impl TestbedSpec {
    /// An empty spec whose single profile is named `Default`.
    pub fn new() -> Self {
        Self {
            profile: "Default".to_owned(),
            mods: Vec::new(),
        }
    }

    /// Append a managed mod (built by `build`) at the next-lower priority, enabled or not.
    pub fn managed(
        mut self,
        name: &str,
        enabled: bool,
        build: impl FnOnce(ModSpec) -> ModSpec,
    ) -> Self {
        self.mods
            .push((name.to_owned(), enabled, build(ModSpec::default())));
        self
    }
}

/// Generate the instance described by `spec` under `dir`, with temp local/INI dirs, and return it.
pub fn build_testbed(dir: &Utf8Path, spec: &TestbedSpec) -> Instance {
    let mut instance = Instance::new(dir.join("instance"), dir.join("game"));
    instance.config.local_dir = Some(dir.join("local"));
    instance.config.ini_dir = Some(dir.join("my_games"));

    for (name, _enabled, m) in &spec.mods {
        let mod_dir = instance.mods_dir().join(name);
        std::fs::create_dir_all(&mod_dir).expect("create mod dir");
        for p in &m.plugins {
            let masters: Vec<&str> = p.masters.iter().map(String::as_str).collect();
            write_plugin_versioned(&mod_dir, &p.name, p.flags, &masters, p.header_version);
        }
        for (rel, bytes) in &m.files {
            let path = mod_dir.join(rel);
            std::fs::create_dir_all(path.parent().expect("file parent")).expect("create parents");
            std::fs::write(path, bytes).expect("write file");
        }
    }

    let order: Vec<(&str, bool)> = spec
        .mods
        .iter()
        .map(|(name, enabled, _)| (name.as_str(), *enabled))
        .collect();
    save_profile(&instance, &spec.profile, &order);
    instance
}
