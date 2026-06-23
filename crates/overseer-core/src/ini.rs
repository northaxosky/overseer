//! Reading the game's INI files (`Fallout4.ini`, `Fallout4Custom.ini`, `Fallout4Prefs.ini`)

use crate::instance::Instance;
use camino::Utf8PathBuf;
use std::collections::BTreeMap;
use thiserror::Error;

/// Something went wrong locating or reading the game INIs
#[derive(Debug, Error)]
pub enum IniError {
    #[error("could not locate the Documents folder to find the game's INI directory")]
    NoDocumentsDir,

    #[error("the Documents path is not valid UTF-8: {0}")]
    NonUtf8DocumentsPath(std::path::PathBuf),

    #[error("reading {path}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// A parsed INI file: sections of key/value pairs
#[derive(Debug, Clone, Default)]
pub struct Ini {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl Ini {
    /// Parse INI text: `[section]` headers and `key=value` lines; everything else ignored
    pub fn parse(text: &str) -> Self {
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = name.trim().to_lowercase();
            } else if let Some((key, value)) = line.split_once('=') {
                sections
                    .entry(current.clone())
                    .or_default()
                    .insert(key.trim().to_lowercase(), value.trim().to_owned());
            }
        }
        Self { sections }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_lowercase())?
            .get(&key.to_lowercase())
            .map(String::as_str)
    }

    pub fn merge(&mut self, other: Ini) {
        for (section, keys) in other.sections {
            let target = self.sections.entry(section).or_default();
            target.extend(keys);
        }
    }
}

/// The game INIs, parsed: `settings` is `<stem>.ini` merged with `<stem>Custom.ini`; prefs is `<stem>Prefs.ini`
#[derive(Debug, Clone, Default)]
pub struct GameInis {
    pub settings: Ini,
    pub prefs: Ini,
}

/// Locate the directory holding the game INIs
pub fn resolve_ini_dir(instance: &Instance) -> Result<Utf8PathBuf, IniError> {
    if let Some(dir) = &instance.config.ini_dir {
        return Ok(dir.clone());
    }

    #[cfg(windows)]
    {
        let docs = dirs::document_dir().ok_or(IniError::NoDocumentsDir)?;
        let docs = Utf8PathBuf::from_path_buf(docs).map_err(IniError::NonUtf8DocumentsPath)?;
        Ok(docs
            .join("My Games")
            .join(instance.config.game.my_games_dir()))
    }
    #[cfg(not(windows))]
    {
        Err(IniError::NoDocumentsDir)
    }
}

/// Read and parse the game's INIs from the resolved directory
pub fn read_game_inis(instance: &Instance) -> Result<GameInis, IniError> {
    let dir = resolve_ini_dir(instance)?;
    let stem = instance.config.game.ini_stem();

    let read = |name: String| -> Result<Ini, IniError> {
        let path = dir.join(name);
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Ini::parse(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ini::default()),
            Err(source) => Err(IniError::Io { path, source }),
        }
    };

    let mut settings = read(format!("{stem}.ini"))?;
    settings.merge(read(format!("{stem}Custom.ini"))?);
    let prefs = read(format!("{stem}Prefs.ini"))?;

    Ok(GameInis { settings, prefs })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::Instance;
    use camino::Utf8Path;
    use tempfile::TempDir;

    // --- parser ---

    #[test]
    fn parses_sections_and_keys() {
        let ini = Ini::parse("[General]\nsFoo=Bar\n[Archive]\nbInvalidateOlderFiles=1\n");
        assert_eq!(ini.get("General", "sFoo"), Some("Bar"));
        assert_eq!(ini.get("Archive", "bInvalidateOlderFiles"), Some("1"));
    }

    #[test]
    fn section_and_key_lookups_are_case_insensitive() {
        let ini = Ini::parse("[ARCHIVE]\nSResourceDataDirsFinal=STRINGS\\\n");
        assert_eq!(
            ini.get("archive", "sresourcedatadirsfinal"),
            Some("STRINGS\\")
        );
        assert_eq!(
            ini.get("Archive", "SResourceDataDirsFinal"),
            Some("STRINGS\\")
        );
    }

    #[test]
    fn values_keep_their_casing_and_inner_equals() {
        // split_once('=') splits on the first '=' only, so a value with '=' survives.
        let ini = Ini::parse("[General]\nsKey=A=B=C\n");
        assert_eq!(ini.get("general", "skey"), Some("A=B=C"));
    }

    #[test]
    fn blank_and_comment_lines_are_ignored() {
        let ini = Ini::parse("\n; a comment\n[General]\n\n; another\nsFoo=1\n");
        assert_eq!(ini.get("general", "sFoo"), Some("1"));
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let ini = Ini::parse("  [General]  \n  sFoo  =  bar baz \n");
        assert_eq!(ini.get("general", "sfoo"), Some("bar baz"));
    }

    #[test]
    fn missing_keys_and_sections_return_none() {
        let ini = Ini::parse("[General]\nsFoo=1\n");
        assert_eq!(ini.get("general", "missing"), None);
        assert_eq!(ini.get("nope", "sFoo"), None);
    }

    // --- merge ---

    #[test]
    fn merge_lets_the_other_file_win() {
        let mut base = Ini::parse("[Archive]\nsResourceDataDirsFinal=STRINGS\\\nbKeep=1\n");
        base.merge(Ini::parse("[Archive]\nsResourceDataDirsFinal=\n"));
        // The shared key is overridden...
        assert_eq!(base.get("archive", "sResourceDataDirsFinal"), Some(""));
        // ...but a key the other file doesn't mention is left alone.
        assert_eq!(base.get("archive", "bKeep"), Some("1"));
    }

    #[test]
    fn merge_adds_new_sections() {
        let mut base = Ini::parse("[General]\nsFoo=1\n");
        base.merge(Ini::parse("[Archive]\nbBar=1\n"));
        assert_eq!(base.get("general", "sFoo"), Some("1"));
        assert_eq!(base.get("archive", "bBar"), Some("1"));
    }

    // --- resolve_ini_dir + read_game_inis (driven through the ini_dir override) ---

    fn temp() -> (TempDir, Utf8PathBuf) {
        let d = TempDir::new().expect("temp");
        let base = Utf8PathBuf::from_path_buf(d.path().to_path_buf()).expect("utf8");
        (d, base)
    }

    fn instance_with_ini_dir(ini_dir: &Utf8Path) -> Instance {
        let mut instance = Instance::new("inst", "game");
        instance.config.ini_dir = Some(ini_dir.to_owned());
        instance
    }

    #[test]
    fn resolve_uses_the_override_when_set() {
        let (_t, base) = temp();
        let instance = instance_with_ini_dir(&base);
        assert_eq!(resolve_ini_dir(&instance).unwrap(), base);
    }

    #[test]
    fn reads_and_merges_the_game_inis() {
        let (_t, dir) = temp();
        // The default game is Fallout4, so stem = "Fallout4".
        std::fs::write(
            dir.join("Fallout4.ini"),
            "[Archive]\nsResourceDataDirsFinal=STRINGS\\\nbInvalidateOlderFiles=0\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("Fallout4Custom.ini"),
            "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        )
        .unwrap();
        std::fs::write(dir.join("Fallout4Prefs.ini"), "[NVFlex]\nbNVFlexEnable=1\n").unwrap();

        let inis = read_game_inis(&instance_with_ini_dir(&dir)).unwrap();
        // Custom overrides base within `settings`.
        assert_eq!(
            inis.settings.get("archive", "bInvalidateOlderFiles"),
            Some("1")
        );
        assert_eq!(
            inis.settings.get("archive", "sResourceDataDirsFinal"),
            Some("")
        );
        // Prefs is kept separate.
        assert_eq!(inis.prefs.get("nvflex", "bNVFlexEnable"), Some("1"));
    }

    #[test]
    fn missing_ini_files_parse_as_empty() {
        let (_t, dir) = temp(); // nothing written
        let inis = read_game_inis(&instance_with_ini_dir(&dir)).unwrap();
        assert_eq!(inis.settings.get("archive", "bInvalidateOlderFiles"), None);
        assert_eq!(inis.prefs.get("nvflex", "bNVFlexEnable"), None);
    }
}
