//! The Confirm modal: a yes/no gate that runs a [`ConfirmAction`] on accept

use crate::app::{
    App, Confirm, ConfirmAction, DeployJob, Focus, ModPaneRow, Modal, PurgeJob, RemoveJob,
    ReplaceJob, separator_display,
};
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
            ConfirmAction::DeleteSave(path) => self.delete_selected_save(&path),
            ConfirmAction::RemoveExe(name) => self.remove_exe(&name),
            ConfirmAction::RemoveMod(name) => self.start_operation(RemoveJob::new(name)),
            ConfirmAction::ReplaceMod { name, archive } => {
                self.start_operation(ReplaceJob::new(name, archive))
            }
            ConfirmAction::DeleteModSeparator { index } => self.delete_mod_separator(index),
            ConfirmAction::DeletePluginSeparator { index } => self.delete_plugin_separator(index),
            ConfirmAction::Deploy => self.start_operation(DeployJob),
            ConfirmAction::Purge => self.start_operation(PurgeJob),
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
        let rows = self.mods.project(&self.session.profile.mods);
        let Some(row) = self.mods.index().and_then(|index| rows.get(index)).copied() else {
            return;
        };
        let ModPaneRow::Separator { model_index, .. } = row else {
            self.note("Only separators can be deleted here");
            return;
        };
        let entry = &self.session.profile.mods[model_index];
        let display = separator_display(&entry.name).to_owned();
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Delete separator {display}? Its members keep their order."),
            action: ConfirmAction::DeleteModSeparator { index: model_index },
        }));
    }

    /// Remove the separator at `index` and persist; re-insert it in memory if the save fails
    fn delete_mod_separator(&mut self, index: usize) {
        let separator_index = self
            .mods
            .project(&self.session.profile.mods)
            .iter()
            .find_map(|row| match row {
                ModPaneRow::Separator {
                    model_index,
                    separator_index,
                    ..
                } if *model_index == index => Some(*separator_index),
                _ => None,
            });
        let Some(separator_index) = separator_index else {
            self.note("That separator is gone");
            return;
        };
        let Some(removed) = self.session.profile.mods.get(index).cloned() else {
            self.note("That separator is gone");
            return;
        };
        let prior_selection = self.mods.index();
        if let Err(e) = self.session.profile.remove_separator(index) {
            self.fail(format!("Delete failed: {e}"));
            return;
        }
        if let Err(e) = self.session.profile.save_modlist(&self.session.instance) {
            self.session.profile.mods.insert(index, removed);
            self.fail(format!("Could not save: {e}"));
            return;
        }
        self.mods.remove_separator(separator_index);
        self.mods.select(prior_selection);
        let len = self.mods.project(&self.session.profile.mods).len();
        self.mods.clamp(len);
        self.ok(format!(
            "Deleted separator {}",
            separator_display(&removed.name)
        ));
    }
}

#[cfg(test)]
#[path = "tests/confirm.rs"]
mod tests;
