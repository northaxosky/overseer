//! The downloads workspace's actions: listing archives and installing one

use crate::app::{App, Modal, Prompt, PromptKind, RefreshDownloadsJob};
use overseer_core::install::DownloadEntry;

impl App {
    /// List the instance's downloads on the background worker
    pub(super) fn refresh_downloads(&mut self) {
        self.start_operation(RefreshDownloadsJob);
    }

    /// The currently selected download entry, if any
    fn selected_download(&self) -> Option<&DownloadEntry> {
        let i = self.downloads.list.index()?;
        self.downloads.entries.get(i)
    }

    /// Act on Enter/Space in the downloads pane: prompt for the install name
    pub(super) fn begin_install_selected(&mut self) {
        let Some(entry) = self.selected_download() else {
            return;
        };
        let path = entry.path.clone();
        let stem = path.file_stem().unwrap_or_default().to_owned();
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::InstallName { archive: path },
            input: stem,
            error: None,
        }));
    }
}

#[cfg(test)]
#[path = "tests/downloads.rs"]
mod tests;
