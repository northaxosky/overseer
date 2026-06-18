//! Shared test helpers for the `plugins` module: generation of minimal but valid
//! Fallout 4 plugin files so metadata parsing can be tested without a game install.

use camino::{Utf8Path, Utf8PathBuf};

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
