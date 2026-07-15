//! Read-only conflict detection: which enabled mods provide the same file

use super::error::DeployError;
use super::plan::{DestinationEntry, ModSource, enumerate_destinations, walk_mod_files};
use camino::Utf8PathBuf;
use std::collections::BTreeMap;

/// A relative path provided by more than one mod; `providers` are in priority order
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileConflict {
    /// The winner's cased relative path, kept for display
    pub relative: Utf8PathBuf,
    /// Mod names in priority order, winner last
    pub providers: Vec<String>,
}

/// Files that more than one mod provides, compared case insensitively; `mods` are in priority order
pub fn detect_conflicts(mods: &[ModSource]) -> Result<Vec<FileConflict>, DeployError> {
    // Per lowercased path: the latest cased relative path plus every provider in order
    let mut providers: BTreeMap<String, (Utf8PathBuf, Vec<String>)> = BTreeMap::new();

    for m in mods {
        walk_mod_files(m, |relative, _abs| {
            let key = relative.as_str().to_lowercase();
            let entry = providers.entry(key).or_default();
            entry.0 = relative; // latest casing wins
            if entry.1.last().map(String::as_str) != Some(m.display_name()) {
                entry.1.push(m.display_name().to_owned());
            }
            Ok(())
        })?;
    }

    Ok(providers
        .into_values()
        .filter(|(_, names)| names.len() > 1)
        .map(|(relative, names)| FileConflict {
            relative,
            providers: names,
        })
        .collect())
}

/// A read-only snapshot of every destination provided by more than one source
#[derive(Debug, Clone)]
pub struct ConflictSnapshot {
    conflicts: Vec<DestinationEntry>,
}

impl ConflictSnapshot {
    /// Enumerate destinations for `mods` (low->high) and keep only contested ones
    pub fn build(mods: &[ModSource]) -> Result<Self, DeployError> {
        let conflicts = enumerate_destinations(mods)?
            .into_values()
            .filter(|entry| entry.providers.len() > 1)
            .collect();
        Ok(Self { conflicts })
    }

    /// The contested destinations, sorted by destination, providers low->high (winner last)
    pub fn conflicts(&self) -> &[DestinationEntry] {
        &self.conflicts
    }

    pub fn is_empty(&self) -> bool {
        self.conflicts().is_empty()
    }

    pub fn len(&self) -> usize {
        self.conflicts().len()
    }
}

#[cfg(test)]
#[path = "tests/conflict.rs"]
mod tests;
