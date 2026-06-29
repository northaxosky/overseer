//! Keyboard handling and the actions it drives on [`App`].

use anyhow::Result;
use overseer_core::deploy::NullSink;
use overseer_core::instance::ModKind;
use overseer_core::plugins::discover_plugins;
use overseer_core::{apply, launch};
use overseer_diagnostics::diagnose;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use super::{App, Focus, HELP_ENTRIES, Popup, Session, initial_selection};

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
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
            KeyCode::Char('l') => self.focus_tab(Popup::Launcher),

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

    /// Route a key while a popup is open: Tab cycles tabs, everything else goes to the active tab
    fn handle_overlay_key(&mut self, tab: Popup, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.focus_tab(tab.cycle(1)),
            KeyCode::BackTab => self.focus_tab(tab.cycle(-1)),
            _ => match tab {
                Popup::Help => self.handle_help_key(key),
                Popup::Settings => self.handle_settings_key(key),
                Popup::Doctor => self.handle_doctor_key(key),
                Popup::Launcher => self.handle_launcher_key(key),
            },
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

    /// Handle a key press in the launch popup
    fn handle_launcher_key(&mut self, key: KeyEvent) {
        let n = launch::targets(&self.session.instance).len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('l') => self.popup = None,
            KeyCode::Down | KeyCode::Char('j') => move_in_list(&mut self.launch_state, n, 1),
            KeyCode::Up | KeyCode::Char('k') => move_in_list(&mut self.launch_state, n, -1),
            KeyCode::Enter => self.launch_selected(),
            _ => {}
        }
    }

    /// Show `tab`, preparing its selection (for doctor: its fresh report)
    fn focus_tab(&mut self, tab: Popup) {
        match tab {
            Popup::Help => self.help_state.select(Some(0)),
            Popup::Settings => {
                let selected = (!self.settings.recent_instances.is_empty()).then_some(0);
                self.settings_state.select(selected);
            }
            Popup::Doctor => match diagnose(&self.session.instance, &self.session.profile.name) {
                Ok(report) => {
                    let selected = (!report.findings.is_empty()).then_some(0);
                    self.doctor_state.select(selected);
                    self.report = Some(report);
                }
                Err(e) => {
                    self.fail(format!("Error: {e}"));
                    return;
                }
            },
            Popup::Launcher => {
                let n = launch::targets(&self.session.instance).len();
                self.launch_state.select((n > 0).then_some(0));
            }
        }
        self.popup = Some(tab);
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
                self.ok("Switched instance");
            }
            Err(e) => self.fail(format!("Error: {e}")),
        }
        self.popup = None;
    }

    fn launch_selected(&mut self) {
        let targets = launch::targets(&self.session.instance);
        match self.launch_state.selected().and_then(|i| targets.get(i)) {
            Some(name) => match launch::launch(&self.session.instance, name) {
                Ok(()) => self.ok(format!("Launched {name}")),
                Err(e) => self.fail(format!("Launch failed: {e}")),
            },
            None => self.note("No launch targets — add one with `overseer exe add`"),
        }
        self.popup = None;
    }

    fn deploy(&mut self) {
        match apply::deploy_profile(
            &self.session.instance,
            &self.session.profile.name,
            &NullSink,
        ) {
            Ok(d) => self.ok(format!("Deployed {} files", d.record.entries.len())),
            Err(e) => self.fail(format!("Deploy failed: {e}")),
        }
        self.session.status = apply::status(&self.session.instance).unwrap_or(None);
    }

    fn purge(&mut self) {
        match apply::purge(&self.session.instance, &NullSink) {
            Ok(()) => self.ok("Purged the live deployment"),
            Err(e) => self.fail(format!("Purge failed: {e}")),
        }
        self.session.status = apply::status(&self.session.instance).unwrap_or(None);
    }

    /// Toggle the selected item in the focused pane & report the outcome
    fn toggle_selected(&mut self) {
        if !self.flip_selected() {
            return;
        }
        match self.persist() {
            Ok(()) => self.ok("Saved"),
            Err(e) => self.fail(format!("Error: {e}")),
        }
    }

    /// Move the selected mod up or down in priority
    fn reorder_selected(&mut self, delta: isize) {
        if !self.shift_selected_mod(delta) {
            return;
        }
        match self.session.profile.save(&self.session.instance) {
            Ok(()) => self.ok("Saved"),
            Err(e) => self.fail(format!("Error: {e}")),
        }
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
                    // Only Managed mods serialize an enabled flag; flipping a DLC/CC
                    // (Foreign) or Separator would be a silent no-op on save.
                    if m.kind != ModKind::Managed {
                        self.note("Only managed mods can be toggled");
                        return false;
                    }
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
    fn toggling_a_non_managed_mod_is_refused() {
        use overseer_core::instance::ModKind;
        let mut app = App::sample();
        app.session
            .profile
            .mods
            .push(overseer_core::instance::ModListEntry {
                name: "DLCRobot".to_owned(),
                enabled: true,
                kind: ModKind::Foreign,
            });
        let foreign = app.session.profile.mods.len() - 1;
        app.mods_state.select(Some(foreign));
        assert!(!app.flip_selected(), "foreign entries can't be flipped");
        assert!(app.session.profile.mods[foreign].enabled, "left unchanged");
        assert!(app.message.is_some(), "user is told why");
    }

    #[test]
    fn l_opens_the_launcher_and_l_again_closes_it() {
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert_eq!(app.popup, Some(Popup::Launcher));
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert_eq!(app.popup, None);
    }

    #[test]
    fn launching_with_no_targets_notes_and_closes() {
        let mut app = App::sample(); // sample instance configures no exes
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.popup, None, "picker closes");
        assert!(app.message.is_some(), "user is told there are none");
    }

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

    #[test]
    fn tab_cycles_between_overlay_tabs() {
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)); // open Settings
        assert_eq!(app.popup, Some(Popup::Settings));
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // Settings -> Help
        assert_eq!(app.popup, Some(Popup::Help));
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)); // Help -> Settings
        assert_eq!(app.popup, Some(Popup::Settings));
    }
}
