//! The Plugins workspace's separators: view-model, CRUD, and collapse

use crate::app::{App, Confirm, ConfirmAction, Focus, Modal, Prompt, PromptKind, Workspace};
use overseer_core::plugins::{PluginRow, SeparatorError, merge_rows};

/// A plugin separator's collapse key: its display name, lowercased
fn plugin_group_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

impl App {
    /// True when the Plugins workspace pane holds focus
    pub(super) fn on_plugins_pane(&self) -> bool {
        self.focus == Focus::Workspace && self.workspace == Workspace::Plugins
    }

    /// The merged plugins rows in display order, hiding plugins under a collapsed separator
    pub(crate) fn plugins_visible_rows(&self) -> Vec<PluginRow> {
        let rows = merge_rows(
            &self.session.order.plugins,
            &self.session.plugin_separators.items,
        );
        let mut out = Vec::with_capacity(rows.len());
        let mut hidden = false;
        for row in rows {
            match row {
                PluginRow::Separator(s) => {
                    hidden = self.is_plugin_collapsed(s);
                    out.push(row);
                }
                PluginRow::Plugin(_) if !hidden => out.push(row),
                PluginRow::Plugin(_) => {}
            }
        }
        out
    }

    /// The merged row selected in the plugins pane, translating from display space
    pub(crate) fn selected_plugin_row(&self) -> Option<PluginRow> {
        self.plugins_visible_rows()
            .get(self.plugins_state.selected()?)
            .copied()
    }

    /// Whether the plugin separator at `sep_index` is collapsed
    pub(crate) fn is_plugin_collapsed(&self, sep_index: usize) -> bool {
        let name = &self.session.plugin_separators.items[sep_index].name;
        self.plugins_collapsed.contains(&plugin_group_key(name))
    }

    /// The number of plugins grouped under the plugin separator at `sep_index`
    pub(crate) fn plugin_group_members(&self, sep_index: usize) -> usize {
        let rows = merge_rows(
            &self.session.order.plugins,
            &self.session.plugin_separators.items,
        );
        let mut counting = false;
        let mut members = 0;
        for row in rows {
            match row {
                PluginRow::Separator(s) if s == sep_index => counting = true,
                PluginRow::Separator(_) if counting => break,
                PluginRow::Plugin(_) if counting => members += 1,
                _ => {}
            }
        }
        members
    }

    /// Toggle the plugin separator at `sep_index` between collapsed and expanded
    fn toggle_plugin_collapsed(&mut self, sep_index: usize) {
        let key = plugin_group_key(&self.session.plugin_separators.items[sep_index].name);
        if !self.plugins_collapsed.remove(&key) {
            self.plugins_collapsed.insert(key);
        }
        self.clamp_plugins_selection();
    }

    /// Re-clamp the plugins display selection into the visible bounds
    pub(super) fn clamp_plugins_selection(&mut self) {
        let len = self.plugins_visible_rows().len();
        super::clamp_selection(&mut self.plugins_state, len);
    }

    /// Space/Enter in the Plugins pane: collapse a separator row, or toggle a plugin active
    pub(super) fn toggle_selected_plugin_row(&mut self) -> bool {
        match self.selected_plugin_row() {
            Some(PluginRow::Separator(s)) => {
                self.toggle_plugin_collapsed(s);
                false
            }
            Some(PluginRow::Plugin(i)) => {
                let p = &mut self.session.order.plugins[i];
                p.active = !p.active;
                true
            }
            None => false,
        }
    }

    /// Open the new-plugin-separator prompt from the Plugins pane
    pub(super) fn open_new_plugin_separator(&mut self) {
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::NewPluginSeparator,
            input: String::new(),
            error: None,
        }));
    }

    /// Insert the plugin separator named in the open prompt above the selection; stay open on err
    pub(super) fn submit_new_plugin_separator(&mut self) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let name = prompt.input.trim().to_owned();
        match self.insert_selected_plugin_separator(&name) {
            Ok(()) => {
                self.modal = None;
                self.ok(format!("Added plugin separator: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Insert a plugin separator anchored above the selection and persist; revert on save failure
    fn insert_selected_plugin_separator(&mut self, name: &str) -> Result<(), String> {
        let anchor = self.anchor_below_selection();
        let at = self.session.plugin_separators.items.len();
        self.session
            .plugin_separators
            .insert(at, anchor, name)
            .map_err(|e| e.to_string())?;
        self.clamp_plugins_selection();
        if let Err(e) = self.save_plugin_separators() {
            self.session.plugin_separators.items.remove(at);
            return Err(format!("Could not save: {e}"));
        }
        self.reselect_plugin_separator(at);
        Ok(())
    }

    /// The name of the first plugin at or below the current selection, or None for a trailing group
    fn anchor_below_selection(&self) -> Option<String> {
        let rows = self.plugins_visible_rows();
        let start = self.plugins_state.selected().unwrap_or(0);
        rows.get(start..)?.iter().find_map(|row| match row {
            PluginRow::Plugin(i) => Some(self.session.order.plugins[*i].name.clone()),
            PluginRow::Separator(_) => None,
        })
    }

    /// Open the rename prompt for the selected plugin separator; note when the row is a plugin
    pub(super) fn open_rename_plugin_separator(&mut self) {
        match self.selected_plugin_row() {
            Some(PluginRow::Separator(index)) => {
                let name = self.session.plugin_separators.items[index].name.clone();
                self.modal = Some(Modal::Prompt(Prompt {
                    kind: PromptKind::RenamePluginSeparator { index, name },
                    input: String::new(),
                    error: None,
                }));
            }
            _ => self.note("Select a plugin separator to rename"),
        }
    }

    /// Rename the plugin separator named in the open prompt; stay open on any error
    pub(super) fn submit_rename_plugin_separator(&mut self, index: usize) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let name = prompt.input.trim().to_owned();
        match self.rename_plugin_separator(index, &name) {
            Ok(()) => {
                self.modal = None;
                self.ok(format!("Renamed plugin separator: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Rename the plugin separator at `index` and persist; revert the in-memory rename on failure
    fn rename_plugin_separator(&mut self, index: usize, name: &str) -> Result<(), String> {
        let prev = self
            .session
            .plugin_separators
            .items
            .get(index)
            .map(|s| s.name.clone())
            .ok_or_else(|| "That separator is gone".to_owned())?;
        self.session
            .plugin_separators
            .rename(index, name)
            .map_err(|e| e.to_string())?;
        if let Err(e) = self.save_plugin_separators() {
            self.session.plugin_separators.items[index].name = prev;
            return Err(format!("Could not save: {e}"));
        }
        self.reselect_plugin_separator(index);
        Ok(())
    }

    /// Confirm deleting the selected plugin separator; note when the row is a plugin
    pub(super) fn begin_delete_selected_plugin_separator(&mut self) {
        match self.selected_plugin_row() {
            Some(PluginRow::Separator(index)) => {
                let name = self.session.plugin_separators.items[index].name.clone();
                self.modal = Some(Modal::Confirm(Confirm {
                    message: format!(
                        "Delete plugin separator {name}? Its plugins keep their order."
                    ),
                    action: ConfirmAction::DeletePluginSeparator { index },
                }));
            }
            _ => self.note("Select a plugin separator to delete"),
        }
    }

    /// Remove the plugin separator at `index` and persist; re-insert it in memory on save failure
    pub(super) fn delete_plugin_separator(&mut self, index: usize) {
        let Some(removed) = self.session.plugin_separators.items.get(index).cloned() else {
            self.note("That separator is gone");
            return;
        };
        if let Err(e) = self.session.plugin_separators.remove(index) {
            self.fail(format!("Delete failed: {e}"));
            return;
        }
        if let Err(e) = self.save_plugin_separators() {
            self.session.plugin_separators.items.insert(index, removed);
            self.fail(format!("Could not save: {e}"));
            return;
        }
        self.clamp_plugins_selection();
        self.ok(format!("Deleted plugin separator {}", removed.name));
    }

    /// Select the display row for the plugin separator at items index `index`
    fn reselect_plugin_separator(&mut self, index: usize) {
        let display = self
            .plugins_visible_rows()
            .iter()
            .position(|row| *row == PluginRow::Separator(index));
        self.plugins_state.select(display);
    }

    /// Persist the plugin separators sidecar for the active profile
    fn save_plugin_separators(&self) -> Result<(), SeparatorError> {
        let dir = self
            .session
            .instance
            .profile_dir(&self.session.profile.name);
        self.session.plugin_separators.save(&dir)
    }
}

#[cfg(test)]
#[path = "tests/plugins.rs"]
mod tests;
