//! Keyboard handling and the actions it drives on [`App`].

mod actions;
mod command;
mod confirm;
mod doctor;
mod downloads;
mod info;
mod plugins;
mod prompt;
mod saves;
mod select;

use super::sort::{DownloadsPane, SavesPane};
use super::{
    App, ConflictsStatus, Focus, ListCursor, Modal, OperationKind, PluginPaneRow, ScanConflictsJob,
    SelectKind, Workspace,
};
use command::{BusyPolicy, Context};
use overseer_core::deploy::ProviderOrigin;
use overseer_core::instance::ModKind;
use overseer_core::plugins::plugin_provider;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy)]
enum RefreshCause {
    Shown,
    Explicit,
}

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        // A modal blocks everything beneath it: it gets keys before the main view
        if self.modal.is_some() {
            self.handle_modal_key(key);
            return;
        }
        self.handle_main_key(key);
    }

    /// Handle one key press. Input is read by the run loop in `main`
    pub(crate) fn handle_main_key(&mut self, key: KeyEvent) {
        // Enter dismisses a finished operation's completion banner first
        if key.code == KeyCode::Enter && self.dismiss_completed_operation() {
            return;
        }
        if key.code == KeyCode::Enter && self.dismiss_launch_notice() {
            return;
        }
        // A mutating operation blocks quit before anything else consumes the key
        if let Some(active) = self.running_operation_kind()
            && is_quit(key)
            && active.is_mutating()
        {
            self.note_busy();
            return;
        }
        // Esc clears an active Conflicts filter before it can quit
        if key.code == KeyCode::Esc
            && self.workspace == Workspace::Conflicts
            && self.conflicts.filter.is_some()
        {
            self.conflicts.filter = None;
            let len = self.conflicts.visible_indices().len();
            self.conflicts.list.select_first(len);
            return;
        }
        if is_quit(key) {
            self.detach_launch();
            self.should_quit = true;
            return;
        }
        let command = self.command_for(key);
        if let Some(kind) = command.as_ref().and_then(|command| {
            command.launch_block(Context {
                focus: self.focus,
                workspace: self.workspace,
            })
        }) && self.game_running()
        {
            self.note_blocked_operation(kind);
            return;
        }
        // Busy-gate a recognized command before clearing the notice
        if let (Some(command), Some(_active)) = (command.as_ref(), self.running_operation_kind()) {
            let ctx = Context {
                focus: self.focus,
                workspace: self.workspace,
            };
            match command.busy_policy(ctx) {
                BusyPolicy::Allowed => {}
                BusyPolicy::Blocked(kind) => {
                    self.note_blocked_operation(kind);
                    return;
                }
                BusyPolicy::BlockedGeneric => {
                    self.note_busy();
                    return;
                }
            }
        }
        // Any accepted key clears the last notice, even when it maps to no command
        self.message = None;
        if let Some(command) = command {
            self.execute(command);
        }
    }

    /// Route a key while a model is open
    fn handle_modal_key(&mut self, key: KeyEvent) {
        match self.modal {
            Some(Modal::Select(_)) => self.handle_select_key(key),
            Some(Modal::Prompt(_)) => self.handle_prompt_key(key),
            Some(Modal::Confirm(_)) => self.handle_confirm_key(key),
            Some(Modal::Info(_)) => self.handle_info_key(key),
            Some(Modal::Doctor(_)) => self.handle_doctor_key(key),
            None => {}
        }
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Mods => Focus::Workspace,
            Focus::Workspace => Focus::Mods,
        };
    }

    /// Switch the active workspace, refreshing its listed data whenever it shows
    fn switch_workspace(&mut self, ws: Workspace) {
        self.workspace = ws;
        self.refresh_visible_lazy_data();
    }

    /// Reload the active workspace's lazy list; Plugins/Conflicts do nothing here, and Conflicts rescans on `r`
    fn refresh_visible_lazy_data(&mut self) {
        let ws = self.workspace;
        ws.refresh(self, RefreshCause::Shown);
    }

    /// Move the selection within the focused pane, clamped to its bounds
    fn move_main_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Mods => {
                let len = self.mods.project(self.session.profile.rows()).len();
                self.mods.move_by(len, delta);
            }
            Focus::Workspace => {
                let ws = self.workspace;
                ws.move_selection(self, delta);
            }
        }
    }

    /// Start a profile-global conflict scan on the background worker
    fn scan_conflicts(&mut self) {
        self.start_operation(ScanConflictsJob);
    }

    /// Invalidate the last conflicts scan after the enabled mod set changes
    pub(super) fn mark_conflicts_stale(&mut self) {
        self.conflicts.status = ConflictsStatus::Stale;
    }

    /// After replacing `self.session`, reset the per-pane selection and refresh workspace
    pub(super) fn after_session_changed(&mut self) {
        self.mods.reset(self.session.profile.rows());
        self.plugins
            .reset(&self.session.order.plugins, &self.session.plugin_separators);
        self.conflicts.list.reset_first(0);
        self.downloads.list.reset_first(0);
        self.saves.list.reset_first(0);
        self.mark_conflicts_stale();
        self.refresh_visible_lazy_data();
    }

    fn move_in_modal_list(&mut self, delta: isize) {
        let Some((selection, len)) = self.modal.as_mut().and_then(Modal::list_parts_mut) else {
            return;
        };
        selection.move_by(len, delta);
    }

    /// Filter the Conflicts list to the managed mod selected in the Mods pane
    fn filter_conflicts_to_selection(&mut self) {
        if self.workspace != Workspace::Conflicts {
            return;
        }
        let rows = self.mods.project(self.session.profile.rows());
        let Some(row) = self.mods.index().and_then(|i| rows.get(i)).copied() else {
            return;
        };
        let Some(entry) = self.session.profile.item_at_row(row.model_index()) else {
            self.note("Select a managed mod to filter conflicts");
            return;
        };
        if entry.kind != ModKind::Managed {
            self.note("Select a managed mod to filter conflicts");
            return;
        }
        self.conflicts.filter = Some(entry.name.clone());
        let len = self.conflicts.visible_indices().len();
        self.conflicts.list.select_first(len);
    }

    /// Select a unique managed mod by name and reveal its owning group
    fn reveal_mod(&mut self, name: &str) {
        let matches: Vec<usize> = self
            .session
            .profile
            .rows()
            .iter()
            .enumerate()
            .filter_map(|(index, _row)| {
                self.session.profile.item_at_row(index).and_then(|entry| {
                    (entry.kind == ModKind::Managed && entry.name.eq_ignore_ascii_case(name))
                        .then_some(index)
                })
            })
            .collect();
        let model_index = match matches.len() {
            0 => {
                self.note(format!("{name} is not in the mod list"));
                return;
            }
            1 => matches[0],
            _ => {
                self.note(format!("{name} matches multiple mods"));
                return;
            }
        };

        self.mods
            .reveal_group(self.session.profile.rows(), model_index);
        let display = self
            .mods
            .project(self.session.profile.rows())
            .iter()
            .position(|row| row.model_index() == model_index);
        self.mods.select(display);
        self.focus = Focus::Mods;
    }

    /// Jump from the active workspace row to its deployed provider
    fn reveal_provider(&mut self) {
        match self.workspace {
            Workspace::Conflicts => self.reveal_conflict_provider(),
            Workspace::Plugins => self.reveal_plugin_provider(),
            Workspace::Downloads | Workspace::Saves => {}
        }
    }

    /// Jump from the selected conflict to one of its managed providers
    fn reveal_conflict_provider(&mut self) {
        let Some(conflict) = self.conflicts.selected() else {
            return;
        };
        let names: Vec<String> = conflict
            .providers
            .iter()
            .rev()
            .filter_map(|provider| match &provider.origin {
                ProviderOrigin::Mod { name } => Some(name.clone()),
                ProviderOrigin::Overwrite => None,
            })
            .collect();
        match names.len() {
            0 => self.note("No mod provider to jump to"),
            1 => {
                let name = names[0].clone();
                self.reveal_mod(&name);
            }
            _ => self.open_select(SelectKind::JumpProvider { providers: names }),
        }
    }

    /// Jump from the selected plugin to its winning deployment source
    fn reveal_plugin_provider(&mut self) {
        if let Some(active) = self.running_operation_kind()
            && matches!(
                active,
                OperationKind::Install | OperationKind::Remove | OperationKind::Replace
            )
        {
            self.note(format!(
                "{} is running; wait to resolve plugin providers",
                active.label()
            ));
            return;
        }
        let rows = self
            .plugins
            .project(&self.session.order.plugins, &self.session.plugin_separators);
        let Some(PluginPaneRow::Plugin { plugin_index }) = self
            .plugins
            .index()
            .and_then(|index| rows.get(index))
            .copied()
        else {
            return;
        };
        let name = self.session.order.plugins[plugin_index].name.clone();

        match plugin_provider(&self.session.instance, &self.session.profile, &name) {
            Ok(Some(ProviderOrigin::Mod { name: mod_name })) => self.reveal_mod(&mod_name),
            Ok(Some(ProviderOrigin::Overwrite)) => {
                self.note(format!("{name} is deployed from the Overwrite bucket"))
            }
            Ok(None) => self.note(format!("{name} is not from a managed mod")),
            Err(error) => self.fail(format!("Could not resolve provider: {error}")),
        }
    }
}

impl Workspace {
    /// Move this workspace's list selection within its current row count
    fn move_selection(self, app: &mut App, delta: isize) {
        if self == Workspace::Plugins {
            let len = app
                .plugins
                .project(&app.session.order.plugins, &app.session.plugin_separators)
                .len();
            app.plugins.move_by(len, delta);
            return;
        }
        let (selection, len) = self
            .list_parts_mut(app)
            .expect("non-Plugins workspaces own ListCursor");
        selection.move_by(len, delta);
    }

    /// Refresh this workspace for either a view-shown or explicit user refresh
    fn refresh(self, app: &mut App, cause: RefreshCause) {
        if app.operation_running() {
            if matches!(cause, RefreshCause::Shown)
                && matches!(self, Workspace::Downloads | Workspace::Saves)
            {
                let active = app
                    .running_operation_kind()
                    .expect("running state has an operation kind");

                app.note(format!(
                    "{} is running; press r to refresh {} afterward",
                    active.label(),
                    self.label()
                ));
            }

            return;
        }

        match self {
            Workspace::Plugins => {}
            Workspace::Conflicts if matches!(cause, RefreshCause::Explicit) => {
                app.scan_conflicts();
            }
            Workspace::Conflicts => {}
            Workspace::Downloads => app.refresh_downloads(),
            Workspace::Saves => app.refresh_saves(),
        }
    }

    /// Cycle the active workspace's sort key when that workspace owns a sortable list
    fn cycle_sort(self, app: &mut App) {
        match self {
            Workspace::Plugins | Workspace::Conflicts => app.note("Only Saves and Downloads sort"),
            Workspace::Downloads => app.cycle_sort::<DownloadsPane>(),
            Workspace::Saves => app.cycle_sort::<SavesPane>(),
        }
    }

    /// Toggle the active workspace's sort direction when that workspace owns a sortable list
    fn toggle_sort_dir(self, app: &mut App) {
        match self {
            Workspace::Plugins | Workspace::Conflicts => app.note("Only Saves and Downloads sort"),
            Workspace::Downloads => app.toggle_sort_dir::<DownloadsPane>(),
            Workspace::Saves => app.toggle_sort_dir::<SavesPane>(),
        }
    }

    /// This workspace's list selection and row count for cursor movement
    fn list_parts_mut(self, app: &mut App) -> Option<(&mut ListCursor, usize)> {
        match self {
            Workspace::Plugins => None,
            Workspace::Conflicts => {
                let len = app.conflicts.visible_indices().len();
                Some((&mut app.conflicts.list, len))
            }
            Workspace::Downloads => {
                let len = app.downloads.entries.len();
                Some((&mut app.downloads.list, len))
            }
            Workspace::Saves => {
                let len = app.saves.entries.len();
                Some((&mut app.saves.list, len))
            }
        }
    }
}

/// Whether a key event should quit the app: `q`, `Esc`, or `Ctrl-C`
fn is_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
}

/// Shared fixtures for the input submodule tests
#[cfg(test)]
pub(crate) mod test_helpers {
    use crate::app::{App, Modal};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// The selected index of an open list modal, or `None`
    pub(crate) fn modal_selection(app: &App) -> Option<usize> {
        match &app.modal {
            Some(Modal::Select(s)) => s.state.index(),
            Some(Modal::Info(i)) => i.state.index(),
            Some(Modal::Doctor(d)) => d.list.index(),
            Some(Modal::Prompt(_)) | Some(Modal::Confirm(_)) | None => None,
        }
    }

    /// A key event with no modifiers
    pub(crate) fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Open the profile picker, then the new-profile prompt, then type `name`
    pub(crate) fn open_prompt_and_type(app: &mut App, name: &str) {
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('n')));
        for c in name.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    /// The open Prompt's input + error, or `None` when no prompt is open
    pub(crate) fn prompt_state(app: &App) -> Option<(&str, Option<&str>)> {
        match &app.modal {
            Some(Modal::Prompt(p)) => Some((p.input.as_str(), p.error.as_deref())),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "tests/input.rs"]
mod tests;
