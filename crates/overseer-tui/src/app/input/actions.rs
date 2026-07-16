//! Main-view mutations: toggling, reordering, deploying, and purging.

use overseer_core::instance::{ModKind, Profile};

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

    /// Move the selected mod up or down in priority
    pub(super) fn reorder_selected(&mut self, delta: isize) {
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

        let rows = self.mods.project(&self.session.profile.mods);
        let selected = self.mods.index()?;
        let destination = selected as isize + display_delta;

        if destination < 0 || destination >= rows.len() as isize {
            return None;
        }

        let source_index = rows.get(selected).copied().map(ModPaneRow::model_index)?;

        let target_row = rows[destination as usize];
        let target_index = target_row.model_index();

        let (source, target) = {
            let mods = &self.session.profile.mods;
            (mods[source_index].kind, mods[target_index].kind)
        };

        if source != ModKind::Managed {
            self.note("Only mods can be reordered");
            return None;
        }
        if target == ModKind::Foreign {
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
        profile.mods.swap(source_index, target_index);

        Some((profile, destination as usize))
    }

    /// Toggle or collapse the selected Mods row and persist managed mod changes
    fn toggle_selected_mod(&mut self) {
        let rows = self.mods.project(&self.session.profile.mods);
        let Some(row) = self.mods.index().and_then(|index| rows.get(index)).copied() else {
            return;
        };

        match row {
            ModPaneRow::Separator {
                separator_index, ..
            } => {
                self.mods.toggle_separator(separator_index);
                let len = self.mods.project(&self.session.profile.mods).len();
                self.mods.clamp(len);
            }
            ModPaneRow::Mod { model_index } => {
                if self.session.profile.mods[model_index].kind != ModKind::Managed {
                    self.note("Only managed mods can be toggled");
                    return;
                }

                let mut profile = self.session.profile.clone();
                profile.mods[model_index].enabled = !profile.mods[model_index].enabled;

                if let Err(e) = profile.save_modlist(&self.session.instance) {
                    self.fail(format!("Could not save mod list: {e}"));
                    return;
                }

                self.session.profile = profile;
                self.mark_conflicts_stale();

                match self.session.profile.sync_plugins(&self.session.instance) {
                    Ok((discovered, order)) => {
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
        let rows = self.mods.project(&self.session.profile.mods);
        let row = self
            .mods
            .index()
            .and_then(|index| rows.get(index))
            .copied()?;
        let ModPaneRow::Mod { model_index } = row else {
            self.note(format!("Only managed mods can be {verb}"));
            return None;
        };
        let entry = &self.session.profile.mods[model_index];
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
