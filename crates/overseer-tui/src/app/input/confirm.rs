//! The Confirm modal: a yes/no gate that runs a [`ConfirmAction`] on accept

use crate::app::{App, Confirm, ConfirmAction, Focus, Modal, separator_display};
use overseer_core::instance::ModKind;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

impl App {
    /// Keys for the Confirm modal: `y`/Enter run the action, `n`/Esc cancel
    pub(super) fn handle_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => self.run_confirmed_action(),
            KeyCode::Char('n') | KeyCode::Esc => self.modal = None,
            _ => {}
        }
    }

    /// Take the confirm modal and run its action
    fn run_confirmed_action(&mut self) {
        let Some(Modal::Confirm(confirm)) = self.modal.take() else {
            return;
        };
        match confirm.action {
            ConfirmAction::InstallDownload(path) => self.install_download(&path),
            ConfirmAction::DeleteSave(path) => self.delete_selected_save(&path),
            ConfirmAction::RemoveExe(name) => self.remove_exe(&name),
            ConfirmAction::DeleteModSeparator { index } => self.delete_mod_separator(index),
            ConfirmAction::DeletePluginSeparator { index } => self.delete_plugin_separator(index),
        }
    }

    /// Delete key dispatcher: act on the focused pane's separator, else note
    pub(super) fn begin_delete_selected_separator(&mut self) {
        if self.focus == Focus::Mods {
            self.begin_delete_selected_mod_separator();
        } else if self.on_plugins_pane() {
            self.begin_delete_selected_plugin_separator();
        } else {
            self.note("Switch to the mods or plugins pane to delete a separator");
        }
    }

    /// Confirm deleting the selected mod separator; noop unless the focused Mods row is a separator
    fn begin_delete_selected_mod_separator(&mut self) {
        let Some(i) = self.selected_mod() else {
            return;
        };
        let entry = &self.session.profile.mods[i];
        if entry.kind != ModKind::Separator {
            self.note("Only separators can be deleted here");
            return;
        }
        let display = separator_display(&entry.name).to_owned();
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Delete separator {display}? Its members keep their order."),
            action: ConfirmAction::DeleteModSeparator { index: i },
        }));
    }

    /// Remove the separator at `index` and persist; re-insert it in memory if the save fails
    fn delete_mod_separator(&mut self, index: usize) {
        let Some(removed) = self.session.profile.mods.get(index).cloned() else {
            self.note("That separator is gone");
            return;
        };
        if let Err(e) = self.session.profile.remove_separator(index) {
            self.fail(format!("Delete failed: {e}"));
            return;
        }
        if let Err(e) = self.session.profile.save(&self.session.instance) {
            self.session.profile.mods.insert(index, removed);
            self.fail(format!("Could not save: {e}"));
            return;
        }
        self.clamp_mod_selection();
        self.ok(format!(
            "Deleted separator {}",
            separator_display(&removed.name)
        ));
    }
}

#[cfg(test)]
#[path = "tests/confirm.rs"]
mod tests;
