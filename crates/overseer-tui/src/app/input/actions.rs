//! Main-view mutations: toggling, reordering, deploying, and purging.

use overseer_core::instance::{CommitOutcome, ModKind, ModRow, Profile};

use crate::app::{App, Confirm, ConfirmAction, Focus, ModPaneRow, Modal, SelectKind, Workspace};

impl App {
    /// Toggle the selected item in the focused pane & report the outcome
    pub(super) fn toggle_selected(&mut self) {
        match self.focus {
            Focus::Mods => self.toggle_selected_mod(),
            Focus::Workspace => {
                let workspace = self.workspace;
                workspace.primary(self);
            }
        }
    }

    /// Move the selected mod or plugin in display order
    pub(super) fn reorder_selected(&mut self, delta: isize) {
        if self.on_plugins_pane() {
            self.reorder_selected_plugin(delta);
            return;
        }

        let Some((profile, selection)) = self.reordered_profile(delta) else {
            return;
        };

        if let Err(err) = profile.save_modlist(&self.session.instance) {
            self.fail(format!("Could not save mod list: {err}"));
            return;
        }

        self.session.profile = profile;
        self.mods.select(Some(selection));
        self.mark_conflicts_stale();
        self.ok("Saved");
    }

    /// Stage a selected mod move in display order without changing live state
    fn reordered_profile(&mut self, display_delta: isize) -> Option<(Profile, usize)> {
        if self.focus != Focus::Mods {
            return None;
        }

        let rows = self.mods.project(self.session.profile.rows());
        let selected = self.mods.index()?;
        let destination = selected as isize + display_delta;

        if destination < 0 || destination >= rows.len() as isize {
            return None;
        }

        let source_index = rows.get(selected).copied().map(ModPaneRow::model_index)?;

        let target_row = rows[destination as usize];
        let target_index = target_row.model_index();

        let source = self
            .session
            .profile
            .item_at_row(source_index)
            .map(|item| item.kind);
        let target = self.session.profile.rows().get(target_index);

        if source != Some(ModKind::Managed) {
            self.note("Only mods can be reordered");
            return None;
        }
        if matches!(target, Some(ModRow::Item(item)) if item.kind == ModKind::Foreign) {
            self.note("Can't reorder past a base-game entry");
            return None;
        }

        if matches!(
            target_row,
            ModPaneRow::Separator {
                collapsed: true,
                ..
            }
        ) {
            self.note("Expand the group to move past it");
            return None;
        }

        // Both endpoints visible, plain swap is clean
        let mut profile = self.session.profile.clone();
        profile.swap_rows(source_index, target_index);

        Some((profile, destination as usize))
    }

    /// Toggle or collapse the selected Mods row and persist managed mod changes
    fn toggle_selected_mod(&mut self) {
        let rows = self.mods.project(self.session.profile.rows());
        let Some(row) = self.mods.index().and_then(|index| rows.get(index)).copied() else {
            return;
        };

        match row {
            ModPaneRow::Separator {
                separator_index, ..
            } => {
                self.mods.toggle_separator(separator_index);
                let len = self.mods.project(self.session.profile.rows()).len();
                self.mods.clamp(len);
            }
            ModPaneRow::Mod { model_index } => {
                let Some(entry) = self.session.profile.item_at_row(model_index) else {
                    self.note("Only managed mods can be toggled");
                    return;
                };
                if entry.kind != ModKind::Managed {
                    self.note("Only managed mods can be toggled");
                    return;
                }

                let mut profile = self.session.profile.clone();
                profile
                    .set_item_enabled_at_row(model_index, !entry.enabled)
                    .expect("selected managed row remains valid");

                if let Err(e) = profile.save_modlist(&self.session.instance) {
                    self.fail(format!("Could not save mod list: {e}"));
                    return;
                }

                self.session.profile = profile;
                self.mark_conflicts_stale();

                match self
                    .session
                    .profile
                    .commit_load_order(&self.session.instance)
                {
                    Ok(CommitOutcome { discovered, order }) => {
                        self.session.discovered = discovered;
                        self.session.order = order;
                        self.clamp_plugins_selection();
                        self.ok("Saved");
                    }
                    Err(e) => {
                        self.fail(format!("Saved mod list, but plugin refresh failed: {e}"));
                    }
                }
            }
        }
    }

    /// Ask before deploying the active profile on the background worker
    pub(super) fn begin_deploy(&mut self) {
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Deploy profile {}?", self.session.profile.name),
            action: ConfirmAction::Deploy,
        }));
    }

    /// Ask before purging the live deployment on the background worker
    pub(super) fn begin_purge(&mut self) {
        self.modal = Some(Modal::Confirm(Confirm {
            message: "Purge the live deployment?".to_owned(),
            action: ConfirmAction::Purge,
        }));
    }

    /// Ask before removing the selected managed mod
    pub(super) fn begin_remove_mod(&mut self) {
        let Some(name) = self.managed_mod_name("removed") else {
            return;
        };
        let mut message = format!("Remove {name}?");
        self.append_deployment_advisory(&mut message);
        self.modal = Some(Modal::Confirm(Confirm {
            message,
            action: ConfirmAction::RemoveMod(name),
        }));
    }

    /// Open the archive picker to replace the selected managed mod
    pub(super) fn begin_replace_mod(&mut self) {
        let Some(name) = self.managed_mod_name("replaced") else {
            return;
        };
        self.open_select(SelectKind::ReplaceArchive { target: name });
    }

    /// Return the selected managed mod name or explain why it can't be changed
    fn managed_mod_name(&mut self, verb: &str) -> Option<String> {
        if self.focus != Focus::Mods {
            self.note(format!("Only managed mods can be {verb}"));
            return None;
        }
        let rows = self.mods.project(self.session.profile.rows());
        let row = self
            .mods
            .index()
            .and_then(|index| rows.get(index))
            .copied()?;
        let ModPaneRow::Mod { model_index } = row else {
            self.note(format!("Only managed mods can be {verb}"));
            return None;
        };
        let entry = self
            .session
            .profile
            .item_at_row(model_index)
            .expect("mod pane row maps to an item");
        if entry.kind != ModKind::Managed {
            self.note(format!("Only managed mods can be {verb}"));
            return None;
        }
        Some(entry.name.clone())
    }

    /// Append the live-deployment advisory to a confirm message when one looks active
    pub(super) fn append_deployment_advisory(&self, message: &mut String) {
        if self.session.status.is_some() {
            message
                .push_str(" Note: a deployment looks live — this will fail unless you purge first");
        }
    }
}

impl Workspace {
    /// Run the Enter/Space primary action for this workspace
    fn primary(self, app: &mut App) {
        match self {
            Workspace::Plugins => app.toggle_selected_plugin_row(),
            Workspace::Conflicts => {
                app.note("Conflicts are read-only");
            }
            Workspace::Downloads => {
                app.begin_install_download();
            }
            Workspace::Saves => {
                app.note("Press X to delete a save");
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/actions.rs"]
mod tests;
