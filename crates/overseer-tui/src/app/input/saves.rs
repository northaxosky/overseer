//! The saves workspace's actions: listing the profile's `.fos` saves and deleting one

use crate::app::sort::sort_saves;
use crate::app::{App, Confirm, ConfirmAction, Focus, Modal, Workspace};
use camino::Utf8Path;
use overseer_core::saves::{self, SaveInfo};

impl App {
    /// List the current profile's saves in the saved sort order, selecting the first row
    pub(super) fn refresh_saves(&mut self) {
        let game = self.session.instance.config.game;
        let listed = self
            .session
            .instance
            .saves_dir(&self.session.profile.name)
            .map_err(|e| format!("Could not locate saves: {e}"))
            .and_then(|dir| {
                saves::list_saves(&dir, game).map_err(|e| format!("Could not list saves: {e}"))
            });
        match listed {
            Ok(mut entries) => {
                sort_saves(&mut entries, self.settings.saves_sort);
                self.saves.list.select_first(entries.len());
                self.saves.entries = entries;
            }
            Err(msg) => {
                self.saves.entries.clear();
                self.saves.list.select(None);
                self.fail(msg);
            }
        }
    }

    /// The currently selected save entry, if any
    fn selected_save(&self) -> Option<&SaveInfo> {
        let i = self.saves.list.index()?;
        self.saves.entries.get(i)
    }

    /// Confirm deleting the selected save; inert unless the Saves pane is focused
    pub(super) fn begin_delete_selected_save(&mut self) {
        // `X` is a main-view key, so guard it to the one pane it acts on
        if !self.on_saves_pane() {
            return;
        }
        let Some(save) = self.selected_save() else {
            return;
        };

        // Copy out what the confirm needs so we stop borrowing `self.saves`
        let file_name = save.file_name.clone();
        let path = save.path.clone();
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Delete {file_name}? This cannot be undone."),
            action: ConfirmAction::DeleteSave(path),
        }));
    }

    /// Delete the save at `path`, re-list, and keep the selection near where it was
    pub(super) fn delete_selected_save(&mut self, path: &Utf8Path) {
        let name = path.file_name().unwrap_or(path.as_str()).to_owned();
        let prev = self.saves.list.index().unwrap_or(0);
        match saves::delete_save(path, self.session.instance.config.game) {
            Ok(()) => {
                self.refresh_saves();
                // The deleted row is gone; clamp the selection to the new bounds
                let len = self.saves.entries.len();
                self.saves.list.select(Some(prev));
                self.saves.list.clamp(len);
                self.ok(format!("Deleted {name}"));
            }
            Err(e) => self.fail(format!("Delete failed: {e}")),
        }
    }

    /// Toggle the current profile's LocalSaves flag; inert unless the Saves pane is focused
    pub(super) fn toggle_local_saves(&mut self) {
        if !self.on_saves_pane() {
            return;
        }
        self.session.profile.local_saves = !self.session.profile.local_saves;
        match self.session.profile.save(&self.session.instance) {
            Ok(()) => {
                let state = if self.session.profile.local_saves {
                    "on"
                } else {
                    "off"
                };
                self.ok(format!("Local saves {state}"));
            }
            Err(e) => {
                self.session.profile.local_saves = !self.session.profile.local_saves;
                self.fail(format!("Could not save profile: {e}"));
            }
        }
    }

    /// True when the Saves workspace pane is focused
    fn on_saves_pane(&self) -> bool {
        self.focus == Focus::Workspace && self.workspace == Workspace::Saves
    }
}

#[cfg(test)]
#[path = "tests/saves.rs"]
mod tests;
