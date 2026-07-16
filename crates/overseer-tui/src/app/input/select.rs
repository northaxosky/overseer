//! The Select modal: launcher, profile, and instance pickers.

use anyhow::Result;
use camino::Utf8PathBuf;
use overseer_core::install;
use overseer_core::launch::{self, ToolKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{
    App, Confirm, ConfirmAction, Focus, LaunchRow, ListCursor, Modal, Select, SelectKind, Session,
};

impl App {
    /// Keys for Select modal: navigate the list, submit, or cancel
    pub(super) fn handle_select_key(&mut self, key: KeyEvent) {
        let Some(Modal::Select(select)) = &self.modal else {
            return;
        };
        let toggle = select.kind.toggle_key();
        match key.code {
            // Esc/q always cancel
            KeyCode::Esc | KeyCode::Char('q') => self.modal = None,
            KeyCode::Char('n') if select.kind == SelectKind::Profile => self.open_new_profile(),
            KeyCode::Char('r') if select.kind == SelectKind::Profile => self.open_rename_profile(),
            KeyCode::Char('a') if select.kind == SelectKind::Launch => self.open_add_exe(),
            KeyCode::Char('x') if select.kind == SelectKind::Launch => self.begin_remove_exe(),
            KeyCode::Char('e') if select.kind == SelectKind::Launch => self.begin_edit_exe(),
            KeyCode::Char(c) if c == toggle => self.modal = None,
            KeyCode::Down | KeyCode::Char('j') => self.move_in_modal_list(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_in_modal_list(-1),
            KeyCode::Enter => self.submit_modal(),
            _ => {}
        }
    }

    /// Open a Select modal of `kind`, selecting its first item
    pub(super) fn open_select(&mut self, kind: SelectKind) {
        match self.load_select_items(&kind) {
            Ok((items, launch_rows)) => {
                let state = ListCursor::first(items.len());
                self.modal = Some(Modal::Select(Select {
                    kind,
                    items,
                    launch_rows,
                    state,
                }));
            }
            Err(e) => self.fail(format!("Error: {e}")),
        }
    }

    /// Load a kind's items; fallible so a real listing error surfaces
    fn load_select_items(&self, kind: &SelectKind) -> Result<(Vec<String>, Vec<LaunchRow>)> {
        let mut launch_rows = Vec::new();
        let items = match kind {
            SelectKind::Launch => launch::targets(&self.session.instance)
                .into_iter()
                .map(|tool| {
                    launch_rows.push(LaunchRow {
                        key: tool.key.clone(),
                        kind: tool.kind,
                        display_name: tool.name.clone(),
                    });
                    format!("{}  [{}]", tool.name, tool.key)
                })
                .collect(),
            SelectKind::Profile => self.session.instance.profiles()?,
            // The recent instances, minus the one already open (record_opened puts it first)
            SelectKind::Instance => self
                .settings
                .recent_instances
                .iter()
                .filter(|p| {
                    !p.as_str()
                        .eq_ignore_ascii_case(self.session.instance.root.as_str())
                })
                .map(camino::Utf8PathBuf::to_string)
                .collect(),
            SelectKind::ReplaceArchive { .. } => install::list_downloads(&self.session.instance)?
                .into_iter()
                .map(|entry| entry.name)
                .collect(),
            SelectKind::JumpProvider { providers } => providers.clone(),
        };
        Ok((items, launch_rows))
    }

    /// Act on the active modal's submission, then close it
    fn submit_modal(&mut self) {
        let select = match self.modal.take() {
            Some(Modal::Select(select)) => select,
            // A Prompt submits via its own handler; a Confirm via `handle_confirm_key`
            Some(Modal::Prompt(_))
            | Some(Modal::Confirm(_))
            | Some(Modal::Info(_))
            | Some(Modal::Doctor(_))
            | None => {
                return;
            }
        };
        let chosen_index = select.state.index();
        let chosen = chosen_index.and_then(|i| select.items.get(i).cloned());
        match select.kind {
            SelectKind::Launch => {
                self.launch(chosen_index.and_then(|i| select.launch_rows.get(i).cloned()))
            }
            SelectKind::Profile => self.switch_profile(chosen),
            SelectKind::Instance => self.switch_instance(chosen),
            SelectKind::ReplaceArchive { target } => self.replace_mod(target, chosen),
            SelectKind::JumpProvider { .. } => {
                if let Some(name) = chosen {
                    self.reveal_mod(&name);
                }
            }
        }
    }

    /// Launch the target at `selected` or note when there is none
    fn launch(&mut self, selected: Option<LaunchRow>) {
        match selected {
            Some(row) => match launch::launch(&self.session.instance, &row.key) {
                Ok(()) => self.ok(format!("Launched {}", row.display_name)),
                Err(e) => self.fail(format!("Launch failed: {e}")),
            },
            None => self.note("No launch targets — add one with `overseer exe add`"),
        }
    }

    /// Ask to remove the highlighted launch target
    fn begin_remove_exe(&mut self) {
        let Some(Modal::Select(select)) = &self.modal else {
            return;
        };
        let Some(row) = select
            .state
            .index()
            .and_then(|i| select.launch_rows.get(i).cloned())
        else {
            self.note("No launch target to remove");
            return;
        };
        if row.kind != ToolKind::User {
            self.note("Game and script extender launch targets cannot be removed");
            return;
        }
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Remove launch target {}?", row.display_name),
            action: ConfirmAction::RemoveExe(row.key),
        }));
    }

    /// Confirm replacement after choosing an archive basename
    fn replace_mod(&mut self, target: String, chosen: Option<String>) {
        let Some(archive) = chosen else {
            self.note("No archive selected to replace with");
            return;
        };
        let mut message = format!("Replace {target} with {archive}?");
        self.append_deployment_advisory(&mut message);
        self.modal = Some(Modal::Confirm(Confirm {
            message,
            action: ConfirmAction::ReplaceMod {
                name: target,
                archive,
            },
        }));
    }

    /// Start editing the highlighted launch target: its name, then its args
    fn begin_edit_exe(&mut self) {
        let Some(Modal::Select(select)) = &self.modal else {
            return;
        };
        let Some(row) = select
            .state
            .index()
            .and_then(|index| select.launch_rows.get(index).cloned())
        else {
            self.note("No launch target to edit");
            return;
        };
        if row.kind != ToolKind::User {
            self.note("Game and script extender launch targets cannot be edited");
            return;
        }
        self.open_edit_exe_name(row.key);
    }

    /// Remove the launch target named `name`, persist, then reopen the picker
    pub(super) fn remove_exe(&mut self, key: &str) {
        let snapshot = self.session.instance.config.tools.clone();
        let removed = match self.session.instance.config.remove_tool(key) {
            Ok(tool) => tool,
            Err(error) => {
                self.fail(error.to_string());
                return;
            }
        };
        if let Err(e) = self.session.instance.save() {
            self.session.instance.config.tools = snapshot; // keep memory == disk
            self.fail(format!("Could not save instance: {e}"));
            return;
        }
        self.open_select(SelectKind::Launch);
        self.ok(format!("Removed launch target {}", removed.name));
    }

    /// Switch the active profile to the one at `selected`, reloading the session
    fn switch_profile(&mut self, selected: Option<String>) {
        let Some(name) = selected else {
            self.note("No profiles to switch to");
            return;
        };
        let dir = self.session.instance.root.clone();
        match Session::load(&dir, Some(&name)) {
            Ok(session) => {
                self.session = session;
                self.after_session_changed();
                self.focus = Focus::Mods;
                self.ok(format!("Switched to {name}"));
            }
            Err(e) => self.fail(format!("Error: {e}")),
        }
    }

    /// Switch to `chosen` with the current profile, re-opening the picker with a fail notice if load fails
    fn switch_instance(&mut self, chosen: Option<String>) {
        let Some(path) = chosen else {
            self.note("No other instances to switch to");
            return;
        };
        let dir = Utf8PathBuf::from(path);
        let profile_name = self.session.profile.name.clone();
        match Session::load(&dir, Some(&profile_name)) {
            Ok(session) => {
                self.session = session;
                self.after_session_changed();
                self.focus = Focus::Mods;
                self.settings.record_opened(&dir);
                if let Err(e) = self.settings.save() {
                    tracing::warn!(error = %e, "could not save settings");
                }
                self.ok("Switched instance");
            }
            Err(e) => {
                // Leave the picker visible so the failed switch is recoverable
                self.open_select(SelectKind::Instance);
                self.fail(format!("Error: {e}"));
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/select.rs"]
mod tests;
