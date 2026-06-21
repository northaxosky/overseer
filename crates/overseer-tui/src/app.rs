//! Application state and update logic.

use anyhow::Result;
use camino::Utf8Path;
use overseer_core::apply::{self, DeploymentStatus};
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

#[cfg(test)]
use overseer_core::instance::ModListEntry;
#[cfg(test)]
use overseer_core::plugins::PluginEntry;

/// Which pane has keyboard focus.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Mods,
    Plugins,
}

/// A popup floating over the main view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Popup {
    Help,
    // Settings, ModActions, etc... later
}

/// The loaded snapshot the UI renders.
#[derive(Debug)]
pub(crate) struct App {
    pub(crate) should_quit: bool,
    pub(crate) popup: Option<Popup>,
    pub(crate) focus: Focus,
    pub(crate) instance: Instance,
    pub(crate) profile: Profile,
    pub(crate) order: PluginLoadOrder,
    pub(crate) discovered: Vec<PluginMeta>,
    pub(crate) status: Option<DeploymentStatus>,
    pub(crate) message: Option<String>,
    pub(crate) mods_state: ListState,
    pub(crate) plugins_state: ListState,
}

impl App {
    /// Load an instance snapshot
    pub(crate) fn load(instance_dir: &Utf8Path, profile_name: &str) -> Result<Self> {
        let instance = Instance::load(instance_dir.to_owned())?;

        let mut profile = Profile::load(&instance, profile_name)?;
        profile.reconcile(&instance)?;

        let discovered = discover_plugins(&instance, &profile)?;
        let mut order = PluginLoadOrder::load(&instance, profile_name)?;
        order.reconcile(&discovered);

        let status = apply::status(&instance)?;

        Ok(Self {
            should_quit: false,
            popup: None,
            focus: Focus::Mods,
            mods_state: initial_selection(profile.mods.len()),
            plugins_state: initial_selection(order.plugins.len()),
            instance,
            profile,
            order,
            discovered,
            status,
            message: None,
        })
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        match self.popup {
            None => self.handle_main_key(key),
            // Help is currently non-interactive
            Some(Popup::Help) => self.popup = None,
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
            KeyCode::Char('?') => self.popup = Some(Popup::Help),
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected(),
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Char('J') => self.reorder_selected(1),
            KeyCode::Char('K') => self.reorder_selected(-1),
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
            Focus::Mods => (&mut self.mods_state, self.profile.mods.len()),
            Focus::Plugins => (&mut self.plugins_state, self.order.plugins.len()),
        };
        if len == 0 {
            return;
        }
        let current = state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        state.select(Some(next));
    }

    /// Toggle the selected item in the focused pane & report the outcome
    fn toggle_selected(&mut self) {
        if !self.flip_selected() {
            return;
        }
        self.message = Some(match self.persist() {
            Ok(()) => "Saved".to_owned(),
            Err(e) => format!("Error: {e}"),
        });
    }

    /// Move the selected mod up or down in priority
    fn reorder_selected(&mut self, delta: isize) {
        if !self.shift_selected_mod(delta) {
            return;
        }
        self.message = Some(match self.profile.save(&self.instance) {
            Ok(()) => "Saved".to_owned(),
            Err(e) => format!("Error: {e}"),
        });
    }

    /// Move the selected mod one step in priority
    fn shift_selected_mod(&mut self, delta: isize) -> bool {
        if self.focus != Focus::Mods {
            return false;
        }
        let Some(i) = self.mods_state.selected() else {
            return false;
        };
        let target = i as isize + delta;
        if target < 0 || target >= self.profile.mods.len() as isize {
            return false;
        }
        let name = self.profile.mods[i].name.clone();
        let moved = if delta < 0 {
            self.profile.move_up(&name).is_ok()
        } else {
            self.profile.move_down(&name).is_ok()
        };
        if moved {
            self.mods_state.select(Some(target as usize));
        }
        moved
    }

    /// Flip the mod's `enabled` / plugin's `active`
    fn flip_selected(&mut self) -> bool {
        match self.focus {
            Focus::Mods => {
                if let Some(i) = self.mods_state.selected() {
                    let m = &mut self.profile.mods[i];
                    m.enabled = !m.enabled;
                    return true;
                }
            }
            Focus::Plugins => {
                if let Some(i) = self.plugins_state.selected() {
                    let p = &mut self.order.plugins[i];
                    p.active = !p.active;
                    return true;
                }
            }
        }
        false
    }

    /// Save the profile and load order, re-deriving plugins
    fn persist(&mut self) -> Result<()> {
        self.profile.save(&self.instance)?;
        self.discovered = discover_plugins(&self.instance, &self.profile)?;
        self.order.reconcile(&self.discovered);
        self.order.save(&self.instance)?;
        clamp_selection(&mut self.plugins_state, self.order.plugins.len());
        Ok(())
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

/// Keep a selection within `[0, len)`, clear it when the list is empty
fn clamp_selection(state: &mut ListState, len: usize) {
    if len == 0 {
        state.select(None);
    } else if let Some(i) = state.selected() {
        state.select(Some(i.min(len - 1)));
    }
}

#[cfg(test)]
impl App {
    /// A small in-memory fixture for tests (no disk access).
    pub(crate) fn sample() -> Self {
        App {
            should_quit: false,
            popup: None,
            focus: Focus::Mods,
            instance: Instance::new("test-instance", "test-game"),
            message: None,
            profile: Profile {
                name: "Default".to_owned(),
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
            },
            order: PluginLoadOrder {
                profile: "Default".to_owned(),
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
            },
            discovered: vec![PluginMeta {
                name: "Cool.esm".to_owned(),
                is_master: true,
                is_light: false,
                masters: Vec::new(),
            }],
            mods_state: initial_selection(2),
            plugins_state: initial_selection(2),
            status: None,
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

    #[test]
    fn flip_toggles_the_selected_mod() {
        let mut app = App::sample();
        assert!(app.profile.mods[0].enabled);
        assert!(app.flip_selected());
        assert!(!app.profile.mods[0].enabled);
    }

    #[test]
    fn flip_toggles_the_selected_plugin() {
        let mut app = App::sample();
        app.focus = Focus::Plugins;
        assert!(app.order.plugins[0].active);
        assert!(app.flip_selected());
        assert!(!app.order.plugins[0].active);
    }

    #[test]
    fn shift_moves_the_selected_mod_and_keeps_selection() {
        let mut app = App::sample();
        assert!(app.shift_selected_mod(1));
        assert_eq!(app.profile.mods[1].name, "CoolMod");
        assert_eq!(app.mods_state.selected(), Some(1));
        assert!(app.shift_selected_mod(-1));
        assert_eq!(app.profile.mods[0].name, "CoolMod");
        assert_eq!(app.mods_state.selected(), Some(0));
    }

    #[test]
    fn shift_is_a_noop_at_edges_and_in_the_plugins_pane() {
        let mut app = App::sample();
        assert!(!app.shift_selected_mod(-1)); // at the top
        assert_eq!(app.mods_state.selected(), Some(0));
        app.focus = Focus::Plugins;
        assert!(!app.shift_selected_mod(1)); // unsupported pane
    }

    #[test]
    fn question_mark_opens_help_and_any_key_closes_it() {
        let mut app = App::sample();
        assert_eq!(app.popup, None);
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert_eq!(app.popup, Some(Popup::Help));
        let before = app.mods_state.selected();
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.popup, None, "any key closes help");
        assert_eq!(
            app.mods_state.selected(),
            before,
            "key swallowed by the popup"
        );
    }
}
