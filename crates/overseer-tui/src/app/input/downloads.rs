//! The downloads workspace's actions: listing archives and installing one

use crate::app::{App, Confirm, ConfirmAction, InstallJob, Modal, RefreshDownloadsJob};
use camino::Utf8Path;
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

    /// Act on Enter/Space in the downloads pane: open the install confirmation
    pub(super) fn begin_install_selected(&mut self) {
        let Some(entry) = self.selected_download() else {
            return;
        };

        // Copy out what the confirm needs so we stop borrowing `self.downloads`
        let name = entry.name.clone();
        let path = entry.path.clone();
        let stem = path.file_stem().unwrap_or(&name).to_owned();
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Install {name}? Creates mods/{stem}."),
            action: ConfirmAction::InstallDownload(path),
        }));
    }

    /// Start archive installation on the background worker
    pub(super) fn install_download(&mut self, path: &Utf8Path) {
        let Some(archive) = path.file_name().map(str::to_owned) else {
            self.fail("Could not identify the archive basename");
            return;
        };
        let Some(name) = path.file_stem().map(str::to_owned) else {
            self.fail("Could not derive a mod name from the archive");
            return;
        };

        self.start_operation(InstallJob::new(archive, name));
    }
}

#[cfg(test)]
#[path = "tests/downloads.rs"]
mod tests;
