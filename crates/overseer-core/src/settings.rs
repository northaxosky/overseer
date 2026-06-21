//! Persistent, app-level settings (not the same as per instance `overseer.toml`)

use atomicwrites::{AtomicFile, OverwriteBehavior};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::io::Write;
use thiserror::Error;

/// How many recent instances to remember
const MAX_RECENT: usize = 10;

/// Errors from loading or saving settings
#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("io error at `{path}`")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse settings at `{path}`")]
    Parse {
        path: Utf8PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("could not serialize settings for `{path}`")]
    Serialize {
        path: Utf8PathBuf,
        #[source]
        source: Box<toml::ser::Error>,
    },
}

fn io_err(path: &Utf8Path, source: std::io::Error) -> SettingsError {
    SettingsError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Persistent app level settings: The schema is intentionally open & every field has a default
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Instances the user has opened, most recent first
    pub recent_instances: Vec<Utf8PathBuf>,
}

impl Settings {
    /// Load from the default path, and use defaults if the file is missing
    pub fn load() -> Self {
        match Self::load_from(&config_path()) {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!(error = %e, "using default settings");
                Self::default()
            }
        }
    }

    /// Persist to the default path
    pub fn save(&self) -> Result<(), SettingsError> {
        self.save_to(&config_path())
    }

    /// Load from a specific file (missing file is defaults)
    pub fn load_from(path: &Utf8Path) -> Result<Self, SettingsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(io_err(path, source));
            }
        };
        toml::from_str(&text).map_err(|source| SettingsError::Parse {
            path: path.to_owned(),
            source: Box::new(source),
        })
    }

    /// Atomically write to a specific file, creating parent dirs
    pub fn save_to(&self, path: &Utf8Path) -> Result<(), SettingsError> {
        let text = toml::to_string_pretty(self).map_err(|e| SettingsError::Serialize {
            path: path.to_owned(),
            source: Box::new(e),
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| io_err(path, source))?;
        }
        let file = AtomicFile::new(path, OverwriteBehavior::AllowOverwrite);
        file.write(|f| f.write_all(text.as_bytes()))
            .map_err(|e: atomicwrites::Error<std::io::Error>| io_err(path, e.into()))
    }

    /// The most recently opened instance, if any
    pub fn last_instance(&self) -> Option<&Utf8Path> {
        self.recent_instances
            .first()
            .map(camino::Utf8PathBuf::as_path)
    }

    /// Record that `instance` was opened, move to front
    pub fn record_opened(&mut self, instance: &Utf8Path) {
        self.recent_instances.retain(|p| p.as_path() != instance);
        self.recent_instances.insert(0, instance.to_owned());
        self.recent_instances.truncate(MAX_RECENT);
    }

    /// Resolve which instance to open: explicit choice wins, or the most recent one does
    pub fn resolve_instance(&self, explicit: Option<Utf8PathBuf>) -> Option<Utf8PathBuf> {
        explicit.or_else(|| self.last_instance().map(std::borrow::ToOwned::to_owned))
    }
}

/// The settings file path
fn config_path() -> Utf8PathBuf {
    config_dir().join("config.toml")
}

/// `$OVERSEER_CONFIG_DIR`, else `%APPDATA%\Overseer`
fn config_dir() -> Utf8PathBuf {
    if let Ok(dir) = std::env::var("OVERSEER_CONFIG_DIR") {
        return Utf8PathBuf::from(dir);
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Utf8PathBuf::from(appdata).join("Overseer");
    }
    Utf8PathBuf::from_path_buf(std::env::temp_dir())
        .unwrap_or_else(|_| Utf8PathBuf::from("."))
        .join("overseer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A temp dir plus a config path inside it (the dir guards the file's lifetime).
    fn temp_config() -> (TempDir, Utf8PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let path = Utf8PathBuf::from_path_buf(dir.path().join("config.toml")).expect("utf8 path");
        (dir, path)
    }

    #[test]
    fn record_opened_dedupes_and_moves_to_front() {
        let mut s = Settings::default();
        s.record_opened(Utf8Path::new("/a"));
        s.record_opened(Utf8Path::new("/b"));
        s.record_opened(Utf8Path::new("/a")); // re-open `a`: to the front, no duplicate
        assert_eq!(
            s.recent_instances,
            vec![Utf8PathBuf::from("/a"), Utf8PathBuf::from("/b")]
        );
    }

    #[test]
    fn record_opened_caps_the_list() {
        let mut s = Settings::default();
        for i in 0..(MAX_RECENT + 5) {
            s.record_opened(Utf8Path::new(&format!("/i{i}")));
        }
        assert_eq!(s.recent_instances.len(), MAX_RECENT);
        // The most recent open is at the front.
        assert_eq!(
            s.last_instance(),
            Some(Utf8Path::new(&format!("/i{}", MAX_RECENT + 4)))
        );
    }

    #[test]
    fn resolve_prefers_explicit_then_last() {
        let mut s = Settings::default();
        assert_eq!(s.resolve_instance(None), None); // first run: nothing to open
        s.record_opened(Utf8Path::new("/last"));
        assert_eq!(s.resolve_instance(None), Some(Utf8PathBuf::from("/last")));
        assert_eq!(
            s.resolve_instance(Some(Utf8PathBuf::from("/explicit"))),
            Some(Utf8PathBuf::from("/explicit"))
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_dir, path) = temp_config();
        let mut s = Settings::default();
        s.record_opened(Utf8Path::new("/x"));
        s.save_to(&path).expect("save");
        let loaded = Settings::load_from(&path).expect("load");
        assert_eq!(loaded.recent_instances, s.recent_instances);
    }

    #[test]
    fn loading_a_missing_file_yields_defaults() {
        let (_dir, path) = temp_config();
        let loaded = Settings::load_from(&path).expect("load");
        assert!(loaded.recent_instances.is_empty());
    }
}
