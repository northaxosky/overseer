//! Application state and update logic.
//!
//! Pure state + update: the [`App`] snapshot, loaded from an instance, and the
//! key handling that mutates it. No terminal I/O and no rendering (see
//! [`crate::ui`]); input is read by the run loop in `main` and dispatched here
//! via [`App::handle_key`].

use anyhow::Result;
use camino::Utf8Path;
use overseer_core::apply::{self, DeploymentStatus};
use overseer_core::instance::{Instance, ModListEntry, Profile};
use overseer_core::plugins::{PluginEntry, PluginLoadOrder, PluginMeta, discover_plugins};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

/// Which pane has keyboard focus.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Mods,
    Plugins,
}

/// The loaded snapshot the UI renders.
#[derive(Debug, Default)]
pub(crate) struct App {
    pub(crate) should_quit: bool,
    pub(crate) focus: Focus,
    pub(crate) profile_name: String,
    pub(crate) mods: Vec<ModListEntry>,
    pub(crate) plugins: Vec<PluginEntry>,
    pub(crate) discovered: Vec<PluginMeta>,
    pub(crate) status: Option<DeploymentStatus>,
    pub(crate) mods_state: ListState,
    pub(crate) plugins_state: ListState,
}

impl App {
    /// Load an instance snapshot. Reconciles in memory but never saves — this is
    /// a read-only viewer, so it must not mutate the instance on disk.
    pub(crate) fn load(instance_dir: &Utf8Path, profile_name: &str) -> Result<Self> {
        let instance = Instance::load(instance_dir.to_owned())?;

        let mut profile = Profile::load(&instance, profile_name)?;
        profile.reconcile(&instance)?;

        let discovered = discover_plugins(&instance, &profile)?;
        let mut order = PluginLoadOrder::load(&instance, profile_name)?;
        order.reconcile(&discovered);

        let status = apply::status(&instance)?;
        let mods = profile.mods;
        let plugins = order.plugins;

        Ok(Self {
            profile_name: profile_name.to_owned(),
            mods_state: initial_selection(mods.len()),
            plugins_state: initial_selection(plugins.len()),
            mods,
            plugins,
            discovered,
            status,
            ..Self::default()
        })
    }

    /// Handle one key press. Input is read by the run loop in `main`, which
    /// filters to key-press events before calling this.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        if is_quit(key) {
            self.should_quit = true;
            return;
        }
        match key.code {
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            _ => {}
        }
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Mods => Focus::Plugins,
            Focus::Plugins => Focus::Mods,
        };
    }

    /// Move the selection within the focused pane, clamped to its bounds.
    fn move_selection(&mut self, delta: isize) {
        let (state, len) = match self.focus {
            Focus::Mods => (&mut self.mods_state, self.mods.len()),
            Focus::Plugins => (&mut self.plugins_state, self.plugins.len()),
        };
        if len == 0 {
            return;
        }
        let current = state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        state.select(Some(next));
    }
}

/// Whether a key event should quit the app: `q`, `Esc`, or `Ctrl-C`.
fn is_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
}

/// A `ListState` selecting the first row when the list is non-empty.
fn initial_selection(len: usize) -> ListState {
    let mut state = ListState::default();
    if len > 0 {
        state.select(Some(0));
    }
    state
}

#[cfg(test)]
impl App {
    /// A small in-memory fixture for tests (no disk access). Shared with the
    /// `ui` render tests.
    pub(crate) fn sample() -> Self {
        App {
            profile_name: "Default".to_owned(),
            mods: vec![
                ModListEntry {
                    name: "CoolMod".to_owned(),
                    enabled: true,
                    foreign: false,
                },
                ModListEntry {
                    name: "OffMod".to_owned(),
                    enabled: false,
                    foreign: false,
                },
            ],
            plugins: vec![
                PluginEntry {
                    name: "Cool.esm".to_owned(),
                    active: true,
                },
                PluginEntry {
                    name: "Cool.esp".to_owned(),
                    active: false,
                },
            ],
            discovered: vec![PluginMeta {
                name: "Cool.esm".to_owned(),
                is_master: true,
                is_light: false,
                masters: Vec::new(),
            }],
            mods_state: initial_selection(2),
            plugins_state: initial_selection(2),
            ..App::default()
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
        app.move_selection(-1); // already at top → clamps
        assert_eq!(app.mods_state.selected(), Some(0));
        app.move_selection(1);
        assert_eq!(app.mods_state.selected(), Some(1));
        app.move_selection(1); // at bottom (len 2) → clamps
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
