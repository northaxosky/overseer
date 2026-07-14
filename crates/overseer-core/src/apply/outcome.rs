//! Structured results from reversing a deployment

use crate::deploy::{PreservedConflict, ReversalIssue};
use crate::restore::Restore;
use camino::Utf8PathBuf;

/// One game-folder path delivered to the global overwrite directory
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedPath {
    /// Original path relative to the game-directory target
    pub game_relative: Utf8PathBuf,
    /// Delivered path relative to the global overwrite directory
    pub overwrite_relative: Utf8PathBuf,
}

/// Complete user-facing result of a purge attempt
#[derive(Debug)]
pub struct ReversalOutcome {
    /// Owned links removed from the target
    pub removed: Vec<Utf8PathBuf>,
    /// Original files restored from deterministic backups
    pub restored: Vec<Utf8PathBuf>,
    /// Foreign output delivered to the global overwrite directory
    pub captured: Vec<CapturedPath>,
    /// Foreign paths preserved instead of being overwritten or deleted
    pub preserved_conflicts: Vec<PreservedConflict>,
    /// Path-aware failures that require a retry
    pub unresolved: Vec<ReversalIssue>,
    /// Content-aware Plugins.txt restore result
    pub plugins_txt: Restore,
    /// Content-aware save-redirection restore result
    pub save_redirect: Restore,
}

impl Default for ReversalOutcome {
    fn default() -> Self {
        Self {
            removed: Vec::new(),
            restored: Vec::new(),
            captured: Vec::new(),
            preserved_conflicts: Vec::new(),
            unresolved: Vec::new(),
            plugins_txt: Restore::Restored,
            save_redirect: Restore::Restored,
        }
    }
}

impl ReversalOutcome {
    /// Whether every blocking filesystem path was resolved
    pub fn is_complete(&self) -> bool {
        self.unresolved.is_empty()
            && !self
                .preserved_conflicts
                .iter()
                .any(|conflict| conflict.blocking)
    }
}
