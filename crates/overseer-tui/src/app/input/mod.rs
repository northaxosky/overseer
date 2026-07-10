//! Keyboard handling and the actions it drives on [`App`].

mod actions;
mod confirm;
mod doctor;
mod downloads;
mod info;
mod plugins;
mod prompt;
mod saves;
mod select;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use overseer_core::deploy::detect_conflicts;

use super::sort::{DownloadsPane, SavesPane};
use super::{
    App, ConflictsStatus, Focus, ListCursor, Modal, SelectKind, Workspace, initial_selection,
};

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
        if is_quit(key) {
            self.should_quit = true;
            return;
        }
        // Any key stroke clears the last message, toggle sets a fresh one
        self.message = None;
        // A toggle key opens its Select modal, resolved once before the literal-key match
        if let KeyCode::Char(c) = key.code
            && let Some(kind) = SelectKind::from_toggle_key(c)
        {
            self.open_select(kind);
            return;
        }
        match key.code {
            // Modal-opening keys
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('R') => self.open_rename_mod(),
            KeyCode::Char('A') => self.open_new_separator(),

            // Workspace view related controls
            KeyCode::Char('d') => self.open_doctor(),
            KeyCode::Char(']') => self.switch_workspace(self.workspace.cycle(1)),
            KeyCode::Char('[') => self.switch_workspace(self.workspace.cycle(-1)),
            KeyCode::Char(c) if Workspace::from_key(c).is_some() => {
                let ws = Workspace::from_key(c).expect("guard ensured a workspace key");
                self.switch_workspace(ws);
            }
            // `r` refreshes the active workspace's data; inert in Plugins
            KeyCode::Char('r') => {
                let ws = self.workspace;
                ws.refresh(self, RefreshCause::Explicit);
            }
            KeyCode::Char('o') => {
                let ws = self.workspace;
                ws.cycle_sort(self);
            }
            KeyCode::Char('O') => {
                let ws = self.workspace;
                ws.toggle_sort_dir(self);
            }

            // `X` deletes the selected save; self-guards to the focused Saves pane
            KeyCode::Char('X') => self.begin_delete_selected_save(),
            // `x` / `Del` delete the selected separator; self-guard to the focused Mods pane
            KeyCode::Char('x') | KeyCode::Delete => self.begin_delete_selected_separator(),
            // `L` toggles the profile's LocalSaves; self-guards to the focused Saves pane
            KeyCode::Char('L') => self.toggle_local_saves(),

            // Main view related controls
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected(),
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Down | KeyCode::Char('j') => self.move_main_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_main_selection(-1),
            KeyCode::Char('J') => self.reorder_selected(1),
            KeyCode::Char('K') => self.reorder_selected(-1),
            KeyCode::Char('D') => self.deploy(),
            KeyCode::Char('P') => self.purge(),
            _ => {}
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
                let len = self.mods.project(&self.session.profile.mods).len();
                self.mods.move_by(len, delta);
            }
            Focus::Workspace => {
                let ws = self.workspace;
                ws.move_selection(self, delta);
            }
        }
    }

    /// Walk the enabled mods' staging dirs and record any file they both provide
    fn scan_conflicts(&mut self) {
        let sources = self.session.profile.deploy_sources(&self.session.instance);
        match detect_conflicts(&sources) {
            Ok(found) => {
                self.conflicts.list.select_first(found.len());
                self.conflicts.status = ConflictsStatus::Ready(found);
            }
            Err(e) => self.conflicts.status = ConflictsStatus::Error(e.to_string()),
        }
    }

    /// Invalidate the last conflicts scan after the enabled mod set changes
    pub(super) fn mark_conflicts_stale(&mut self) {
        self.conflicts.status = ConflictsStatus::Stale;
    }

    /// After replacing `self.session`, reset the per-pane selection and refresh workspace
    pub(super) fn after_session_changed(&mut self) {
        self.plugins_collapsed.clear();
        self.mods.reset(&self.session.profile.mods);
        self.plugins_state = initial_selection(self.plugins_visible_rows().len());
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
}

impl Workspace {
    /// Move this workspace's list selection within its current row count
    fn move_selection(self, app: &mut App, delta: isize) {
        if self == Workspace::Plugins {
            let len = app.plugins_visible_rows().len();
            move_in_list(&mut app.plugins_state, len, delta);
            return;
        }
        let (selection, len) = self
            .list_parts_mut(app)
            .expect("non-Plugins workspaces own ListCursor");
        selection.move_by(len, delta);
    }

    /// Refresh this workspace for either a view-shown or explicit user refresh
    fn refresh(self, app: &mut App, cause: RefreshCause) {
        match self {
            Workspace::Plugins => {}
            Workspace::Conflicts if matches!(cause, RefreshCause::Explicit) => app.scan_conflicts(),
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
                let len = match &app.conflicts.status {
                    ConflictsStatus::Ready(v) => v.len(),
                    _ => 0,
                };
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

/// Keep a selection within `[0, len)`, clear it when the list is empty
fn clamp_selection(state: &mut ListState, len: usize) {
    if len == 0 {
        state.select(None);
    } else if let Some(i) = state.selected() {
        state.select(Some(i.min(len - 1)));
    }
}

/// Move a list selection by `delta` clamped to `[0, len)`
fn move_in_list(state: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        return;
    }
    let current = state.selected().unwrap_or(0) as isize;
    let next = (current + delta).clamp(0, len as isize - 1) as usize;
    state.select(Some(next));
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
