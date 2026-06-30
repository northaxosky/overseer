//! Keyboard handling and the actions it drives on [`App`].

mod actions;
mod overlay;
mod prompt;
mod select;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use super::{App, Focus, Modal, Popup, SelectKind};

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
            Focus::Mods => Focus::Plugins,
            Focus::Plugins => Focus::Mods,
        };
    }

    /// Move the selection within the focused pane, clamped to its bounds.
    fn move_main_selection(&mut self, delta: isize) {
        let (state, len) = match self.focus {
            Focus::Mods => (&mut self.mods_state, self.session.profile.mods.len()),
            Focus::Plugins => (&mut self.plugins_state, self.session.order.plugins.len()),
        };
        move_in_list(state, len, delta);
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
        assert_eq!(app.focus, Focus::Plugins);
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
}
