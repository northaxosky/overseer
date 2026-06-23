//! Application state and update logic.

use anyhow::Result;
use camino::Utf8Path;
use overseer_core::apply::{self, DeploymentStatus};
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins};
use overseer_core::settings::Settings;
use overseer_diagnostics::{Report, diagnose};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

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
    Settings,
    Doctor,
    // ModActions, etc... later
}

/// Key bindings shown (and selectable) in the help popup: (keys, description).
pub(crate) const HELP_ENTRIES: &[(&str, &str)] = &[
    ("j / k   ↓ / ↑", "move selection"),
    ("Tab", "switch pane"),
    ("Space / Enter", "toggle enabled / active"),
    ("J / K", "reorder mod (priority)"),
    ("s", "open settings"),
    ("d", "run diagnostics"),
    ("?", "toggle this help"),
    ("q / Esc", "quit"),
];

/// The loaded domain data for one instance — replaced wholesale on a switch.
#[derive(Debug)]
pub(crate) struct Session {
    pub(crate) instance: Instance,
    pub(crate) profile: Profile,
    pub(crate) order: PluginLoadOrder,
    pub(crate) discovered: Vec<PluginMeta>,
    pub(crate) status: Option<DeploymentStatus>,
}

impl Session {
    /// Load an instance's domain data. Reconciles in memory but never saves.
    pub(crate) fn load(instance_dir: &Utf8Path, profile_name: &str) -> Result<Self> {
        let instance = Instance::load(instance_dir.to_owned())?;

        let mut profile = Profile::load(&instance, profile_name)?;
        profile.reconcile(&instance)?;

        let discovered = discover_plugins(&instance, &profile)?;
        let mut order = PluginLoadOrder::load(&instance, profile_name)?;
        order.reconcile(&discovered);

        let status = apply::status(&instance)?;

        Ok(Self {
            instance,
            profile,
            order,
            discovered,
            status,
        })
    }
}

/// Snapshot the UI renders: persistent UI state plus the current instance's [`Session`].
#[derive(Debug)]
pub(crate) struct App {
    pub(crate) should_quit: bool,
    pub(crate) popup: Option<Popup>,
    pub(crate) focus: Focus,
    pub(crate) message: Option<String>,
    pub(crate) settings: Settings,
    pub(crate) session: Session,
    pub(crate) mods_state: ListState,
    pub(crate) plugins_state: ListState,
    pub(crate) settings_state: ListState,
    pub(crate) help_state: ListState,
    pub(crate) report: Option<Report>,
    pub(crate) doctor_state: ListState,
}

impl App {
    /// Load an instance and remember it in settings.
    pub(crate) fn load(
        instance_dir: &Utf8Path,
        profile_name: &str,
        mut settings: Settings,
    ) -> Result<Self> {
        let session = Session::load(instance_dir, profile_name)?;

        // Only a successful load is worth remembering.
        settings.record_opened(instance_dir);
        if let Err(e) = settings.save() {
            tracing::warn!(error = %e, "could not save settings");
        }

        Ok(Self {
            should_quit: false,
            popup: None,
            focus: Focus::Mods,
            message: None,
            mods_state: initial_selection(session.profile.mods.len()),
            plugins_state: initial_selection(session.order.plugins.len()),
            settings_state: ListState::default(),
            help_state: ListState::default(),
            report: None,
            doctor_state: ListState::default(),
            settings,
            session,
        })
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        match self.popup {
            None => self.handle_main_key(key),
            Some(Popup::Settings) => self.handle_settings_key(key),
            Some(Popup::Help) => self.handle_help_key(key),
            Some(Popup::Doctor) => self.handle_doctor_key(key),
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
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('s') => self.open_settings(),
            KeyCode::Char('d') => self.open_doctor(),

            // Main view related controls
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected(),
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Down | KeyCode::Char('j') => self.move_main_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_main_selection(-1),
            KeyCode::Char('J') => self.reorder_selected(1),
            KeyCode::Char('K') => self.reorder_selected(-1),
            _ => {}
        }
    }

    /// Handle key press in the settings pop up
    fn handle_settings_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.popup = None,
            KeyCode::Down | KeyCode::Char('j') => move_in_list(
                &mut self.settings_state,
                self.settings.recent_instances.len(),
                1,
            ),
            KeyCode::Up | KeyCode::Char('k') => move_in_list(
                &mut self.settings_state,
                self.settings.recent_instances.len(),
                -1,
            ),
            KeyCode::Enter => self.switch_to_selected_instance(),
            _ => {}
        }
    }

    /// Handle a key press in the help popup
    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => self.popup = None,
            KeyCode::Down | KeyCode::Char('j') => {
                move_in_list(&mut self.help_state, HELP_ENTRIES.len(), 1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                move_in_list(&mut self.help_state, HELP_ENTRIES.len(), -1);
            }
            _ => {}
        }
    }

    /// Handle a key press in the diagnostics popup
    fn handle_doctor_key(&mut self, key: KeyEvent) {
        let len = self.report.as_ref().map_or(0, |r| r.findings.len());
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') => self.popup = None,
            KeyCode::Down | KeyCode::Char('j') => move_in_list(&mut self.doctor_state, len, 1),
            KeyCode::Up | KeyCode::Char('k') => move_in_list(&mut self.doctor_state, len, -1),
            _ => {}
        }
    }

    /// Open the settings popup, selecting the current instance
    fn open_settings(&mut self) {
        let selected = (!self.settings.recent_instances.is_empty()).then_some(0);
        self.settings_state.select(selected);
        self.popup = Some(Popup::Settings);
    }

    /// Open the help popup at the top
    fn open_help(&mut self) {
        self.help_state.select(Some(0));
        self.popup = Some(Popup::Help);
    }

    /// Run diagnostics for the current profile and open the popup
    fn open_doctor(&mut self) {
        match diagnose(&self.session.instance, &self.session.profile.name) {
            Ok(report) => {
                let selected = (!report.findings.is_empty()).then_some(0);
                self.doctor_state.select(selected);
                self.report = Some(report);
                self.popup = Some(Popup::Doctor);
            }
            Err(e) => self.message = Some(format!("Error: {e}")),
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

    /// Switch to the instance selected in the settings popup
    fn switch_to_selected_instance(&mut self) {
        let Some(i) = self.settings_state.selected() else {
            return;
        };
        let Some(dir) = self.settings.recent_instances.get(i).cloned() else {
            return;
        };
        let profile_name = self.session.profile.name.clone();
        match Session::load(&dir, &profile_name) {
            Ok(session) => {
                self.session = session;
                self.mods_state = initial_selection(self.session.profile.mods.len());
                self.plugins_state = initial_selection(self.session.order.plugins.len());
                self.focus = Focus::Mods;
                self.settings.record_opened(&dir);
                if let Err(e) = self.settings.save() {
                    tracing::warn!(error = %e, "could not save settings");
                }
                self.message = Some("Switched instance".to_owned());
            }
            Err(e) => self.message = Some(format!("Error: {e}")),
        }
        self.popup = None;
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
        self.message = Some(match self.session.profile.save(&self.session.instance) {
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
        if target < 0 || target >= self.session.profile.mods.len() as isize {
            return false;
        }
        let name = self.session.profile.mods[i].name.clone();
        let moved = if delta < 0 {
            self.session.profile.move_up(&name).is_ok()
        } else {
            self.session.profile.move_down(&name).is_ok()
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
                    let m = &mut self.session.profile.mods[i];
                    m.enabled = !m.enabled;
                    return true;
                }
            }
            Focus::Plugins => {
                if let Some(i) = self.plugins_state.selected() {
                    let p = &mut self.session.order.plugins[i];
                    p.active = !p.active;
                    return true;
                }
            }
        }
        false
    }

    /// Save the profile and load order, re-deriving plugins
    fn persist(&mut self) -> Result<()> {
        self.session.profile.save(&self.session.instance)?;
        self.session.discovered = discover_plugins(&self.session.instance, &self.session.profile)?;
        self.session.order.reconcile(&self.session.discovered);
        self.session.order.save(&self.session.instance)?;
        clamp_selection(&mut self.plugins_state, self.session.order.plugins.len());
        Ok(())
    }
}

/// Whether a key event should quit the app: `q`, `Esc`, or `Ctrl-C`.
fn is_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
}

/// A `ListState` selecting the first row when the list is non-empty.
pub(crate) fn initial_selection(len: usize) -> ListState {
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

/// Move a list selection by `delta` clamped to `[0, len)`
fn move_in_list(state: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        return;
    }
    let current = state.selected().unwrap_or(0) as isize;
    let next = (current + delta).clamp(0, len as isize - 1) as usize;
    state.select(Some(next));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
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

    #[test]
    fn flip_toggles_the_selected_mod() {
        let mut app = App::sample();
        assert!(app.session.profile.mods[0].enabled);
        assert!(app.flip_selected());
        assert!(!app.session.profile.mods[0].enabled);
    }

    #[test]
    fn flip_toggles_the_selected_plugin() {
        let mut app = App::sample();
        app.focus = Focus::Plugins;
        assert!(app.session.order.plugins[0].active);
        assert!(app.flip_selected());
        assert!(!app.session.order.plugins[0].active);
    }

    #[test]
    fn shift_moves_the_selected_mod_and_keeps_selection() {
        let mut app = App::sample();
        assert!(app.shift_selected_mod(1));
        assert_eq!(app.session.profile.mods[1].name, "CoolMod");
        assert_eq!(app.mods_state.selected(), Some(1));
        assert!(app.shift_selected_mod(-1));
        assert_eq!(app.session.profile.mods[0].name, "CoolMod");
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
    fn help_popup_opens_navigates_and_closes() {
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert_eq!(app.popup, Some(Popup::Help));
        assert_eq!(app.help_state.selected(), Some(0));
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(
            app.help_state.selected(),
            Some(1),
            "j navigates within help"
        );
        assert_eq!(
            app.popup,
            Some(Popup::Help),
            "navigation does not close help"
        );
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.popup, None, "Esc closes help");
    }

    #[test]
    fn s_opens_settings_and_navigation_clamps() {
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.popup, Some(Popup::Settings));
        assert_eq!(app.settings_state.selected(), Some(0));
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.settings_state.selected(), Some(1));
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)); // clamp
        assert_eq!(app.settings_state.selected(), Some(1));
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.popup, None);
    }

    #[test]
    fn doctor_popup_navigates_and_closes() {
        use overseer_diagnostics::{Finding, Report, Severity};
        let mut app = App::sample();
        app.report = Some(Report::new(vec![
            Finding {
                check: "a",
                severity: Severity::Warning,
                title: "first".to_owned(),
                detail: None,
            },
            Finding {
                check: "b",
                severity: Severity::Error,
                title: "second".to_owned(),
                detail: Some("why".to_owned()),
            },
        ]));
        app.doctor_state.select(Some(0));
        app.popup = Some(Popup::Doctor);

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.doctor_state.selected(), Some(1), "j navigates findings");
        assert_eq!(
            app.popup,
            Some(Popup::Doctor),
            "navigation does not close the popup"
        );
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)); // clamp at the end
        assert_eq!(app.doctor_state.selected(), Some(1));
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.popup, None, "Esc closes the popup");
    }
}
