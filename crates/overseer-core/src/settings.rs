//! Persistent, app-level settings (not the same as per instance `overseer.toml`)

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;
use strum::{Display, EnumIter, EnumString};
use thiserror::Error;

/// How many recent instances to remember
const MAX_RECENT: usize = 10;

/// Errors from loading or saving settings
#[derive(Debug, Error)]
pub enum SettingsError {
    #[error(transparent)]
    Io(#[from] crate::error::IoError),

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

/// Persistent app level settings: The schema is intentionally open & every field has a default
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Instances the user has opened, most recent first
    pub recent_instances: Vec<Utf8PathBuf>,
    /// Sort preference for front ends that show saves
    pub saves_sort: SavesSort,
    /// Sort preference for front ends that show downloads
    pub downloads_sort: DownloadsSort,
}

/// Sort direction for persisted list preferences
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

/// Sort key for persisted saves list preferences
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Display, EnumIter, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SavesSortKey {
    #[default]
    Date,
    Name,
    Character,
    Level,
}

/// Sort key for persisted downloads list preferences
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Display, EnumIter, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum DownloadsSortKey {
    #[default]
    Name,
    Date,
    Size,
    Installed,
}

/// Persisted list sort preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[serde(bound(deserialize = "K: FromStr + Default, SortPref<K>: Default"))]
pub struct SortPref<K> {
    #[serde(deserialize_with = "lenient")]
    pub key: K,
    #[serde(deserialize_with = "lenient")]
    pub dir: SortDir,
}

/// Persisted saves list sort preference
pub type SavesSort = SortPref<SavesSortKey>;

/// Persisted downloads list sort preference
pub type DownloadsSort = SortPref<DownloadsSortKey>;

impl Default for SavesSort {
    fn default() -> Self {
        Self {
            key: SavesSortKey::Date,
            dir: SortDir::Desc,
        }
    }
}

impl Default for DownloadsSort {
    fn default() -> Self {
        Self {
            key: DownloadsSortKey::Name,
            dir: SortDir::Asc,
        }
    }
}

fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + Default,
{
    Ok(String::deserialize(deserializer)?
        .parse()
        .unwrap_or_default())
}

impl Settings {
    /// Load from the default path, and use defaults if the file is missing
    pub fn load() -> Self {
        match Self::load_from(&config_path()) {
            Ok(settings) => settings,
            Err(e) => {
                // Defaults would overwrite recents on next save; keep a copy first
                if let SettingsError::Parse { path, .. } = &e {
                    let _ = crate::fs::backup_corrupt(path);
                }
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
        let Some(text) = crate::fs::read_to_string_opt(path)? else {
            return Ok(Self::default());
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
        crate::fs::write_atomic(path, text.as_bytes())?;
        Ok(())
    }

    /// The most recently opened instance, if any
    pub fn last_instance(&self) -> Option<&Utf8Path> {
        self.recent_instances
            .first()
            .map(camino::Utf8PathBuf::as_path)
    }

    /// Record that `instance` was opened, move to front
    pub fn record_opened(&mut self, instance: &Utf8Path) {
        self.recent_instances
            .retain(|p| !p.as_str().eq_ignore_ascii_case(instance.as_str()));
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
#[path = "tests/settings.rs"]
mod tests;
