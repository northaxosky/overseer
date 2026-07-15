//! Read-only conflict detection: which enabled mods provide the same file

use super::error::DeployError;
use super::plan::{DestinationEntry, ModSource, enumerate_destinations};

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

    /// Wrap ready-made entries as a snapshot, for adapters and tests
    pub fn from_entries(conflicts: Vec<DestinationEntry>) -> Self {
        Self { conflicts }
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
