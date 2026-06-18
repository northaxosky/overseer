use super::error::PluginError;
use camino::Utf8Path;
use esplugin::{GameId, ParseOptions, Plugin};

/// Whether a filename is a Bethesda plugin we manage
pub fn is_plugin_file(name: &str) -> bool {
    matches!(
        Utf8Path::new(name)
            .extension()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("esp" | "esm" | "esl")
    )
}

/// Metadata read from a plugin's header
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMeta {
    /// The plugin's file name
    pub name: String,
    /// A master file: loaded before normal plugins
    pub is_master: bool,
    /// A light (ESL) plugin: shares the `FE` load order slot
    pub is_light: bool,
    /// The plugins this one depends on (masters), in header order
    pub masters: Vec<String>,
}

/// Read a plugin's metadata from its header
pub fn read_metadata(name: &str, path: &Utf8Path) -> Result<PluginMeta, PluginError> {
    let mut plugin = Plugin::new(GameId::Fallout4, path.as_std_path());
    plugin
        .parse_file(ParseOptions::header_only())
        .map_err(|source| PluginError::Parse {
            path: path.to_owned(),
            source,
        })?;

    let masters = plugin.masters().map_err(|source| PluginError::Parse {
        path: path.to_owned(),
        source,
    })?;

    Ok(PluginMeta {
        name: name.to_owned(),
        is_master: plugin.is_master_file(),
        is_light: plugin.is_light_plugin(),
        masters,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    // FO4/Skyrim plugin header flags.
    const FLAG_MASTER: u32 = 0x1;
    const FLAG_LIGHT: u32 = 0x200;

    /// Build the bytes of a minimal but valid Fallout 4 plugin: a single `TES4` header
    /// record containing an `HEDR` subrecord and one `MAST`/`DATA` pair per master.
    /// Enough for esplugin's header-only parse to read flags and masters.
    fn tes4_bytes(flags: u32, masters: &[&str]) -> Vec<u8> {
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

    /// Write a generated plugin file into `dir` and return its path.
    fn write_plugin(dir: &Utf8Path, name: &str, flags: u32, masters: &[&str]) -> Utf8PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, tes4_bytes(flags, masters)).expect("write plugin");
        path
    }

    fn temp() -> (TempDir, Utf8PathBuf) {
        let d = TempDir::new().expect("temp");
        let base = Utf8PathBuf::from_path_buf(d.path().to_path_buf()).expect("utf8");
        (d, base)
    }

    // --- is_plugin_file ---

    #[test]
    fn recognizes_plugin_extensions_case_insensitively() {
        assert!(is_plugin_file("Mod.esp"));
        assert!(is_plugin_file("Mod.esm"));
        assert!(is_plugin_file("Mod.esl"));
        assert!(is_plugin_file("MOD.ESP"));
        assert!(!is_plugin_file("Texture.ba2"));
        assert!(!is_plugin_file("readme.txt"));
        assert!(!is_plugin_file("noext"));
    }

    // --- read_metadata ---

    #[test]
    fn plain_esp_is_neither_master_nor_light() {
        let (_t, base) = temp();
        let path = write_plugin(&base, "Plain.esp", 0, &[]);
        let meta = read_metadata("Plain.esp", &path).expect("parse");
        assert_eq!(meta.name, "Plain.esp");
        assert!(!meta.is_master);
        assert!(!meta.is_light);
        assert!(meta.masters.is_empty());
    }

    #[test]
    fn master_flag_marks_master() {
        let (_t, base) = temp();
        let path = write_plugin(&base, "Core.esp", FLAG_MASTER, &[]);
        let meta = read_metadata("Core.esp", &path).expect("parse");
        assert!(meta.is_master);
        assert!(!meta.is_light);
    }

    #[test]
    fn light_flag_marks_light_but_not_master() {
        // The 0x200 light flag does not imply master.
        let (_t, base) = temp();
        let path = write_plugin(&base, "Patch.esp", FLAG_LIGHT, &[]);
        let meta = read_metadata("Patch.esp", &path).expect("parse");
        assert!(meta.is_light);
        assert!(!meta.is_master);
    }

    #[test]
    fn esm_extension_implies_master() {
        // No flags set; the .esm extension alone marks it a master.
        let (_t, base) = temp();
        let path = write_plugin(&base, "Big.esm", 0, &[]);
        let meta = read_metadata("Big.esm", &path).expect("parse");
        assert!(meta.is_master);
        assert!(!meta.is_light);
    }

    #[test]
    fn esl_extension_implies_master_and_light() {
        // The .esl extension implies both the master and light flags.
        let (_t, base) = temp();
        let path = write_plugin(&base, "Small.esl", 0, &[]);
        let meta = read_metadata("Small.esl", &path).expect("parse");
        assert!(meta.is_master);
        assert!(meta.is_light);
    }

    #[test]
    fn reads_the_master_list_in_order() {
        let (_t, base) = temp();
        let path = write_plugin(&base, "Dependent.esp", 0, &["Fallout4.esm", "DLCCoast.esm"]);
        let meta = read_metadata("Dependent.esp", &path).expect("parse");
        assert_eq!(meta.masters, ["Fallout4.esm", "DLCCoast.esm"]);
    }

    #[test]
    fn corrupt_file_is_a_parse_error() {
        let (_t, base) = temp();
        let path = base.join("Garbage.esp");
        std::fs::write(&path, b"not a plugin").expect("write");
        let err = read_metadata("Garbage.esp", &path).expect_err("should fail");
        assert!(matches!(err, PluginError::Parse { .. }));
    }
}
