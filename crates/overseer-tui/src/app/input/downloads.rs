//! The downloads workspace's actions: listing archives and installing one

use crate::app::sort::sort_downloads;
use crate::app::{App, Confirm, ConfirmAction, Modal, Session, select_first};
use camino::Utf8Path;
use overseer_core::install::{self, DownloadEntry, InstallError};

impl App {
    /// List the instance's downloads, selecting the first row
    pub(super) fn refresh_downloads(&mut self) {
        match install::list_downloads(&self.session.instance) {
            Ok(mut entries) => {
                sort_downloads(&mut entries, self.settings.downloads_sort);
                select_first(&mut self.downloads.list, entries.len());
                self.downloads.entries = entries;
            }
            Err(e) => {
                self.downloads.entries.clear();
                self.downloads.list.select(None);
                self.fail(format!("Could not list downloads: {e}"));
            }
        }
    }

    /// The currently selected download entry, if any
    fn selected_download(&self) -> Option<&DownloadEntry> {
        let i = self.downloads.list.selected()?;
        self.downloads.entries.get(i)
    }

    /// Act on Enter/Space in the downloads pane: note an already installed archive, else open confirm
    pub(super) fn begin_install_selected(&mut self) {
        let Some(entry) = self.selected_download() else {
            return;
        };

        // Copy out what the confirm needs so we stop borrowing `self.downloads`
        let installed = entry.installed;
        let name = entry.name.clone();
        let path = entry.path.clone();
        if installed {
            self.note("Already installed");
            return;
        }
        let stem = path.file_stem().unwrap_or(&name).to_owned();
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Install {name}? Creates mods/{stem}."),
            action: ConfirmAction::InstallDownload(path),
        }));
    }

    /// Install the archive at `path`, then reload the session in place
    pub(super) fn install_download(&mut self, path: &Utf8Path) {
        self.note("Installing…");
        let Some(name) = path.file_stem().map(|s| s.to_owned()) else {
            self.fail("Could not derive a mod name from the archive");
            return;
        };
        match install::install(&self.session.instance, path, &name) {
            Ok(_) => self.reload_after_install(name),
            Err(InstallError::Fomod) => self.fail("FOMOD installers aren't supported yet"),
            Err(e) => self.fail(format!("Install failed: {e}")),
        }
    }

    /// Reload the domain data after a successful install
    fn reload_after_install(&mut self, name: String) {
        let dir = self.session.instance.root.clone();
        let profile = self.session.profile.name.clone();
        match Session::load(&dir, &profile) {
            Ok(session) => {
                self.session = session;
                self.after_session_changed();
                self.ok(format!("Installed {name}"));
            }
            Err(e) => self.fail(format!("Installed {name}, but reloading failed: {e}")),
        }
    }
}

#[cfg(test)]
#[path = "tests/downloads.rs"]
mod tests;
