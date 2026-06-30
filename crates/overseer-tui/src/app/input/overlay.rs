//! The tabbed overlay popups: help, settings, and diagnostics.

use overseer_diagnostics::diagnose;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::move_in_list;
use crate::app::{App, Focus, HELP_ENTRIES, Popup, Session, initial_selection};

impl App {
    /// Handle key press in the settings pop up
    pub(super) fn handle_settings_key(&mut self, key: KeyEvent) {
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
    pub(super) fn handle_help_key(&mut self, key: KeyEvent) {
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
    pub(super) fn handle_doctor_key(&mut self, key: KeyEvent) {
        let len = self.report.as_ref().map_or(0, |r| r.findings.len());
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') => self.popup = None,
            KeyCode::Down | KeyCode::Char('j') => move_in_list(&mut self.doctor_state, len, 1),
            KeyCode::Up | KeyCode::Char('k') => move_in_list(&mut self.doctor_state, len, -1),
            _ => {}
        }
    }

    /// Show `tab`, preparing its selection (for doctor: its fresh report)
    pub(super) fn focus_tab(&mut self, tab: Popup) {
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
        }
        self.popup = Some(tab);
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
                self.mark_conflicts_stale();
            }
            Err(e) => self.fail(format!("Error: {e}")),
        }
        self.popup = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;

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
