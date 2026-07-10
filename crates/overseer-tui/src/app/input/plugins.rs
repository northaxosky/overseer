//! The Plugins workspace's separator CRUD, collapse, and insertion policy

use crate::app::{
    App, Confirm, ConfirmAction, Focus, Modal, PluginPaneRow, Prompt, PromptKind, Workspace,
};
use overseer_core::plugins::SeparatorError;

#[derive(Debug, PartialEq, Eq)]
struct PluginSeparatorInsert {
    at: usize,
    anchor: Option<String>,
}

impl App {
    /// True when the Plugins workspace pane holds focus
    pub(super) fn on_plugins_pane(&self) -> bool {
        self.focus == Focus::Workspace && self.workspace == Workspace::Plugins
    }

    /// Re-clamp the plugins display selection into the visible bounds
    pub(super) fn clamp_plugins_selection(&mut self) {
        let len = self
            .plugins
            .project(&self.session.order.plugins, &self.session.plugin_separators)
            .len();
        self.plugins.clamp(len);
    }

    /// Space/Enter in the Plugins pane: collapse a separator row, or toggle a plugin active
    pub(super) fn toggle_selected_plugin_row(&mut self) {
        let rows = self
            .plugins
            .project(&self.session.order.plugins, &self.session.plugin_separators);
        let Some(row) = self
            .plugins
            .index()
            .and_then(|index| rows.get(index))
            .copied()
        else {
            return;
        };

        match row {
            PluginPaneRow::Separator {
                separator_index, ..
            } => {
                self.plugins.toggle_separator(separator_index);
                self.clamp_plugins_selection();
            }
            PluginPaneRow::Plugin { plugin_index } => {
                let mut order = self.session.order.clone();
                order.plugins[plugin_index].active = !order.plugins[plugin_index].active;

                match order.save(&self.session.instance) {
                    Ok(()) => {
                        self.session.order = order;
                        self.ok("Saved");
                    }
                    Err(e) => {
                        self.fail(format!("Could not save load order: {e}"));
                    }
                }
            }
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

    /// Insert a plugin separator above the semantic selection and persist with rollback
    fn insert_selected_plugin_separator(&mut self, name: &str) -> Result<(), String> {
        let rows = self
            .plugins
            .project(&self.session.order.plugins, &self.session.plugin_separators);
        let insert = self.resolve_plugin_separator_insert(&rows);
        self.session
            .plugin_separators
            .insert(insert.at, insert.anchor, name)
            .map_err(|e| e.to_string())?;
        if let Err(e) = self.save_plugin_separators() {
            self.session.plugin_separators.items.remove(insert.at);
            return Err(format!("Could not save: {e}"));
        }
        self.plugins.insert_separator(insert.at);
        self.reselect_plugin_separator(insert.at);
        Ok(())
    }

    /// Resolve sidecar position and anchor for a separator inserted above the selected row
    fn resolve_plugin_separator_insert(&self, rows: &[PluginPaneRow]) -> PluginSeparatorInsert {
        let selected = self
            .plugins
            .index()
            .and_then(|index| rows.get(index))
            .copied()
            .or_else(|| rows.first().copied());
        match selected {
            Some(PluginPaneRow::Plugin { plugin_index }) => PluginSeparatorInsert {
                at: self.session.plugin_separators.items.len(),
                anchor: Some(self.session.order.plugins[plugin_index].name.clone()),
            },
            Some(PluginPaneRow::Separator {
                separator_index, ..
            }) => PluginSeparatorInsert {
                at: separator_index,
                anchor: self.session.plugin_separators.items[separator_index]
                    .anchor
                    .clone(),
            },
            None => PluginSeparatorInsert {
                at: self.session.plugin_separators.items.len(),
                anchor: None,
            },
        }
    }

    /// Open the rename prompt for the selected plugin separator; note when the row is a plugin
    pub(super) fn open_rename_plugin_separator(&mut self) {
        let rows = self
            .plugins
            .project(&self.session.order.plugins, &self.session.plugin_separators);
        let selected = self
            .plugins
            .index()
            .and_then(|index| rows.get(index))
            .copied();
        match selected {
            Some(PluginPaneRow::Separator {
                separator_index, ..
            }) => {
                let name = self.session.plugin_separators.items[separator_index]
                    .name
                    .clone();
                self.modal = Some(Modal::Prompt(Prompt {
                    kind: PromptKind::RenamePluginSeparator {
                        index: separator_index,
                        name,
                    },
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

    /// Rename the plugin separator at `index` and persist with rollback
    fn rename_plugin_separator(&mut self, index: usize, name: &str) -> Result<(), String> {
        let previous = self
            .session
            .plugin_separators
            .items
            .get(index)
            .map(|separator| separator.name.clone())
            .ok_or_else(|| "That separator is gone".to_owned())?;
        self.session
            .plugin_separators
            .rename(index, name)
            .map_err(|e| e.to_string())?;
        if let Err(e) = self.save_plugin_separators() {
            self.session.plugin_separators.items[index].name = previous;
            return Err(format!("Could not save: {e}"));
        }
        self.reselect_plugin_separator(index);
        Ok(())
    }

    /// Confirm deleting the selected plugin separator; note when the row is a plugin
    pub(super) fn begin_delete_selected_plugin_separator(&mut self) {
        let rows = self
            .plugins
            .project(&self.session.order.plugins, &self.session.plugin_separators);
        let selected = self
            .plugins
            .index()
            .and_then(|index| rows.get(index))
            .copied();
        match selected {
            Some(PluginPaneRow::Separator {
                separator_index, ..
            }) => {
                let name = self.session.plugin_separators.items[separator_index]
                    .name
                    .clone();
                self.modal = Some(Modal::Confirm(Confirm {
                    message: format!(
                        "Delete plugin separator {name}? Its plugins keep their order."
                    ),
                    action: ConfirmAction::DeletePluginSeparator {
                        index: separator_index,
                    },
                }));
            }
            _ => self.note("Select a plugin separator to delete"),
        }
    }

    /// Remove a plugin separator and persist with rollback
    pub(super) fn delete_plugin_separator(&mut self, index: usize) {
        let Some(removed) = self.session.plugin_separators.items.get(index).cloned() else {
            self.note("That separator is gone");
            return;
        };
        let prior_selection = self.plugins.index();
        if let Err(e) = self.session.plugin_separators.remove(index) {
            self.fail(format!("Delete failed: {e}"));
            return;
        }
        if let Err(e) = self.save_plugin_separators() {
            self.session.plugin_separators.items.insert(index, removed);
            self.fail(format!("Could not save: {e}"));
            return;
        }
        self.plugins.remove_separator(index);
        self.plugins.select(prior_selection);
        self.clamp_plugins_selection();
        self.ok(format!("Deleted plugin separator {}", removed.name));
    }

    /// Select the display row for the plugin separator at sidecar index `index`
    fn reselect_plugin_separator(&mut self, index: usize) {
        let display = self
            .plugins
            .project(&self.session.order.plugins, &self.session.plugin_separators)
            .iter()
            .position(|row| {
                matches!(
                    row,
                    PluginPaneRow::Separator {
                        separator_index,
                        ..
                    } if *separator_index == index
                )
            });
        self.plugins.select(display);
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
