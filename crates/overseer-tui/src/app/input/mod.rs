//! Keyboard handling and the actions it drives on [`App`].

mod actions;
mod overlay;
mod prompt;
mod select;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use overseer_core::deploy::detect_conflicts;

use super::{App, ConflictsStatus, Focus, Modal, Popup, SelectKind, Workspace};

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        // A modal blocks everything beneath it: it gets keys before popup or main
        if self.modal.is_some() {
            self.handle_modal_key(key);
            return;
        }
        match self.popup {
            None => self.handle_main_key(key),
            Some(tab) => self.handle_overlay_key(tab, key),
        }
    }

    /// Handle one key press. Input is read by the run loop in `main`
    pub(crate) fn handle_main_key(&mut self, key: KeyEvent) {
        if is_quit(key) {
            self.should_quit = true;
            return;
        }
        // Any key stroke clears the last message, toggle sets a fresh one
        self.message = None;
        match key.code {
            // Popup keys
            KeyCode::Char('?') => self.focus_tab(Popup::Help),
            KeyCode::Char('s') => self.focus_tab(Popup::Settings),
            KeyCode::Char('d') => self.focus_tab(Popup::Doctor),
            KeyCode::Char('l') => self.open_select(SelectKind::Launch),
            KeyCode::Char('p') => self.open_select(SelectKind::Profile),

            // Main view related controls
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected(),
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Char(']') => self.workspace = self.workspace.cycle(1),
            KeyCode::Char('[') => self.workspace = self.workspace.cycle(-1),
            KeyCode::Char('1') => self.workspace = Workspace::Plugins,
            KeyCode::Char('2') => self.workspace = Workspace::Conflicts,
            KeyCode::Char('r') if self.workspace == Workspace::Conflicts => self.scan_conflicts(),
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
            None => {}
        }
    }

    /// Route a key while a popup is open: Tab cycles tabs, everything else goes to the active tab
    fn handle_overlay_key(&mut self, tab: Popup, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.focus_tab(tab.cycle(1)),
            KeyCode::BackTab => self.focus_tab(tab.cycle(-1)),
            _ => match tab {
                Popup::Help => self.handle_help_key(key),
                Popup::Settings => self.handle_settings_key(key),
                Popup::Doctor => self.handle_doctor_key(key),
            },
        }
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Mods => Focus::Workspace,
            Focus::Workspace => Focus::Mods,
        };
    }

    /// Move the selection within the focused pane, clamped to its bounds.
    fn move_main_selection(&mut self, delta: isize) {
        let (state, len) = match self.focus {
            Focus::Mods => (&mut self.mods_state, self.session.profile.mods.len()),
            Focus::Workspace => match self.workspace {
                Workspace::Plugins => (&mut self.plugins_state, self.session.order.plugins.len()),
                Workspace::Conflicts => {
                    let len = match &self.conflicts.status {
                        ConflictsStatus::Ready(v) => v.len(),
                        _ => 0,
                    };
                    (&mut self.conflicts.list, len)
                }
            },
        };
        move_in_list(state, len, delta);
    }

    /// Walk the enabled mods' staging dirs and record any file they both provide.
    fn scan_conflicts(&mut self) {
        let sources = self.session.profile.deploy_sources(&self.session.instance);
        match detect_conflicts(&sources) {
            Ok(found) => {
                self.conflicts.list.select((!found.is_empty()).then_some(0));
                self.conflicts.status = ConflictsStatus::Ready(found);
            }
            Err(e) => self.conflicts.status = ConflictsStatus::Error(e.to_string()),
        }
    }

    /// Invalidate the last conflicts scan after the enabled mod set changes.
    pub(super) fn mark_conflicts_stale(&mut self) {
        self.conflicts.status = ConflictsStatus::Stale;
    }
}

/// Whether a key event should quit the app: `q`, `Esc`, or `Ctrl-C`.
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

/// Shared fixtures for the input submodule tests.
#[cfg(test)]
pub(crate) mod test_helpers {
    use crate::app::{App, Modal};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// The selected index of an open Select modal, or `None`
    pub(crate) fn modal_selection(app: &App) -> Option<usize> {
        match &app.modal {
            Some(Modal::Select(s)) => s.state.selected(),
            Some(Modal::Prompt(_)) | None => None,
        }
    }

    /// A key event with no modifiers.
    pub(crate) fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Open the profile picker, then the new-profile prompt, then type `name`.
    pub(crate) fn open_prompt_and_type(app: &mut App, name: &str) {
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('n')));
        for c in name.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    /// The open Prompt's input + error, or `None` when no prompt is open.
    pub(crate) fn prompt_state(app: &App) -> Option<(&str, Option<&str>)> {
        match &app.modal {
            Some(Modal::Prompt(p)) => Some((p.input.as_str(), p.error.as_deref())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_toggles_focus() {
        let mut app = App::sample();
        assert_eq!(app.focus, Focus::Mods);
        app.toggle_focus();
        assert_eq!(app.focus, Focus::Workspace);
        app.toggle_focus();
        assert_eq!(app.focus, Focus::Mods);
    }

    #[test]
    fn selection_moves_and_clamps_within_the_focused_pane() {
        let mut app = App::sample();
        assert_eq!(app.mods_state.selected(), Some(0));
        app.move_main_selection(-1); // already at top → clamps
        assert_eq!(app.mods_state.selected(), Some(0));
        app.move_main_selection(1);
        assert_eq!(app.mods_state.selected(), Some(1));
        app.move_main_selection(1); // at bottom (len 2) → clamps
        assert_eq!(app.mods_state.selected(), Some(1));
        // The plugins pane is independent and untouched while Mods is focused.
        assert_eq!(app.plugins_state.selected(), Some(0));
    }

    #[test]
    fn quit_keys_are_recognised() {
        assert!(is_quit(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE
        )));
        assert!(is_quit(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(is_quit(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_quit(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        )));
    }

    #[test]
    fn keys_1_and_2_switch_workspace_without_moving_focus() {
        let mut app = App::sample();
        assert_eq!(app.workspace, Workspace::Plugins);
        assert_eq!(app.focus, Focus::Mods);

        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(app.workspace, Workspace::Conflicts);
        assert_eq!(app.focus, Focus::Mods, "switching never moves focus");

        // Even with the right pane focused, switching back leaves focus put.
        app.focus = Focus::Workspace;
        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert_eq!(app.workspace, Workspace::Plugins);
        assert_eq!(app.focus, Focus::Workspace, "switching never moves focus");
    }

    #[test]
    fn brackets_cycle_through_the_workspaces() {
        let mut app = App::sample();
        assert_eq!(app.workspace, Workspace::Plugins);
        app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        assert_eq!(app.workspace, Workspace::Conflicts, "] goes to the next");
        app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        assert_eq!(app.workspace, Workspace::Plugins, "] wraps around");
        app.handle_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        assert_eq!(app.workspace, Workspace::Conflicts, "[ wraps backward");
    }

    #[test]
    fn jk_route_to_the_active_workspace_list() {
        use overseer_core::deploy::FileConflict;
        let conflict = |name: &str| FileConflict {
            relative: camino::Utf8PathBuf::from(name),
            providers: vec!["Low".to_owned(), "High".to_owned()],
        };

        let mut app = App::sample();
        app.focus = Focus::Workspace;

        // Plugins workspace (default): j/k move the plugins list.
        assert_eq!(app.plugins_state.selected(), Some(0));
        app.move_main_selection(1);
        assert_eq!(app.plugins_state.selected(), Some(1));

        // Conflicts workspace: j/k move the conflicts list, leaving plugins put.
        app.workspace = Workspace::Conflicts;
        app.conflicts.status = ConflictsStatus::Ready(vec![conflict("a.dds"), conflict("b.dds")]);
        app.conflicts.list.select(Some(0));
        app.move_main_selection(1);
        assert_eq!(
            app.conflicts.list.selected(),
            Some(1),
            "conflicts list moves"
        );
        assert_eq!(
            app.plugins_state.selected(),
            Some(1),
            "plugins list untouched"
        );
    }

    #[test]
    fn scanning_a_temp_instance_reports_a_shared_file() {
        use overseer_core::instance::{ModKind, ModListEntry};
        use overseer_core::test_support::{install_mod, temp_instance};

        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "A", &[("Textures/shared.dds", "from-a")]);
        install_mod(&instance, "B", &[("Textures/shared.dds", "from-b")]);

        let mut app = App::sample();
        app.session.instance = instance;
        app.session.profile.mods = vec![
            ModListEntry {
                name: "A".to_owned(),
                enabled: true,
                kind: ModKind::Managed,
            },
            ModListEntry {
                name: "B".to_owned(),
                enabled: true,
                kind: ModKind::Managed,
            },
        ];

        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

        match &app.conflicts.status {
            ConflictsStatus::Ready(found) => {
                assert_eq!(found.len(), 1, "the shared file is the only conflict");
                // deploy_sources feeds detect_conflicts lowest priority first, so the
                // higher-priority mod (top of the list) lands last as the winner.
                assert_eq!(found[0].providers, ["B", "A"]);
            }
            other => panic!("expected a completed scan, got {other:?}"),
        }
        assert_eq!(
            app.conflicts.list.selected(),
            Some(0),
            "selection lands first"
        );
    }

    #[test]
    fn r_outside_the_conflicts_workspace_is_inert() {
        let mut app = App::sample();
        assert_eq!(app.workspace, Workspace::Plugins);
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(
            matches!(app.conflicts.status, ConflictsStatus::Stale),
            "r only scans in the Conflicts workspace"
        );
    }
}
