//! The Select modal: launcher and profile pickers.

use anyhow::Result;
use overseer_core::launch;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::move_in_list;
use crate::app::{App, Focus, Modal, Select, SelectKind, Session, initial_selection};

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
            KeyCode::Char(c) if c == toggle => self.modal = None,
            KeyCode::Down | KeyCode::Char('j') => self.move_in_select(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_in_select(-1),
            KeyCode::Enter => self.submit_modal(),
            _ => {}
        }
    }

    /// Move the active Select modal's selection by `delta`, clamped to its items
    fn move_in_select(&mut self, delta: isize) {
        if let Some(Modal::Select(select)) = self.modal.as_mut() {
            move_in_list(&mut select.state, select.items.len(), delta);
        }
    }

    /// Open a Select modal of `kind`, selecting its first item
    pub(super) fn open_select(&mut self, kind: SelectKind) {
        match self.load_select_items(kind) {
            Ok(items) => {
                let state = initial_selection(items.len());
                self.modal = Some(Modal::Select(Select { kind, items, state }));
            }
            Err(e) => self.fail(format!("Error: {e}")),
        }
    }

    /// Load a kind's items; fallible so a real listing error surfaces
    fn load_select_items(&self, kind: SelectKind) -> Result<Vec<String>> {
        Ok(match kind {
            SelectKind::Launch => launch::targets(&self.session.instance),
            SelectKind::Profile => self.session.instance.profiles()?,
        })
    }

    /// Act on the active modal's submission, then close it
    fn submit_modal(&mut self) {
        let select = match self.modal.take() {
            Some(Modal::Select(select)) => select,
            // A Prompt submits via its own handler, never here.
            Some(Modal::Prompt(_)) | None => return,
        };
        let chosen = select
            .state
            .selected()
            .and_then(|i| select.items.get(i).cloned());
        match select.kind {
            SelectKind::Launch => self.launch(chosen),
            SelectKind::Profile => self.switch_profile(chosen),
        }
    }

    /// Launch the target at `selected` or note when there is none
    fn launch(&mut self, selected: Option<String>) {
        match selected {
            Some(name) => match launch::launch(&self.session.instance, &name) {
                Ok(()) => self.ok(format!("Launched {name}")),
                Err(e) => self.fail(format!("Launch failed: {e}")),
            },
            None => self.note("No launch targets — add one with `overseer exe add`"),
        }
    }

    /// Switch the active profile to the one at `selected`, reloading the session
    fn switch_profile(&mut self, selected: Option<String>) {
        let Some(name) = selected else {
            self.note("No profiles to switch to");
            return;
        };
        let dir = self.session.instance.root.clone();
        match Session::load(&dir, &name) {
            Ok(session) => {
                self.session = session;
                self.mods_state = initial_selection(self.session.profile.mods.len());
                self.plugins_state = initial_selection(self.session.order.plugins.len());
                self.focus = Focus::Mods;
                self.ok(format!("Switched to {name}"));
                self.mark_conflicts_stale();
            }
            Err(e) => self.fail(format!("Error: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::input::test_helpers::*;
    use ratatui::crossterm::event::KeyModifiers;

    #[test]
    fn l_opens_the_launcher_and_l_again_closes_it() {
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Launch,
                    ..
                }))
            ),
            "l opens the launch select modal"
        );
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(app.modal.is_none(), "l again closes it");
    }

    #[test]
    fn launching_with_no_targets_notes_and_closes() {
        let mut app = App::sample(); // sample instance configures no exes
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.modal.is_none(), "picker closes");
        assert!(app.message.is_some(), "user is told there are none");
    }

    #[test]
    fn esc_closes_the_launch_modal() {
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(app.modal.is_some());
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.modal.is_none(), "Esc cancels the modal");
    }

    #[test]
    fn launch_modal_navigates_and_clamps() {
        use camino::Utf8PathBuf;
        use overseer_core::instance::Executable;
        let mut app = App::sample();
        app.session.instance.config.executables = vec![
            Executable {
                name: "game".to_owned(),
                path: Utf8PathBuf::from("game.exe"),
                args: Vec::new(),
            },
            Executable {
                name: "script-extender".to_owned(),
                path: Utf8PathBuf::from("f4se.exe"),
                args: Vec::new(),
            },
        ];
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert_eq!(modal_selection(&app), Some(0), "opens on the first target");
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(modal_selection(&app), Some(1), "j moves down");
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(modal_selection(&app), Some(1), "clamps at the end");
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(modal_selection(&app), Some(0), "k moves up");
    }

    #[test]
    fn n_does_nothing_in_the_launch_picker() {
        // `n` is a profile-picker side-action only; in the launcher it's inert.
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('n')));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Launch,
                    ..
                }))
            ),
            "the launch picker stays open and unchanged"
        );
    }
}
