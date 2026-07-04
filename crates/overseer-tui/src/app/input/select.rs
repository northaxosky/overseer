//! The Select modal: launcher, profile, and instance pickers.

use anyhow::Result;
use camino::Utf8PathBuf;
use overseer_core::launch;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{
    App, Confirm, ConfirmAction, Focus, Modal, Select, SelectKind, Session, initial_selection,
};

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
            KeyCode::Char('r') if select.kind == SelectKind::Profile => self.open_rename_profile(),
            KeyCode::Char('a') if select.kind == SelectKind::Launch => self.open_add_exe(),
            KeyCode::Char('x') if select.kind == SelectKind::Launch => self.confirm_remove_exe(),
            KeyCode::Char(c) if c == toggle => self.modal = None,
            KeyCode::Down | KeyCode::Char('j') => self.move_in_modal_list(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_in_modal_list(-1),
            KeyCode::Enter => self.submit_modal(),
            _ => {}
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
            // The recent instances, minus the one already open (record_opened puts it first)
            SelectKind::Instance => self
                .settings
                .recent_instances
                .iter()
                .filter(|p| {
                    !p.as_str()
                        .eq_ignore_ascii_case(self.session.instance.root.as_str())
                })
                .map(camino::Utf8PathBuf::to_string)
                .collect(),
        })
    }

    /// Act on the active modal's submission, then close it
    fn submit_modal(&mut self) {
        let select = match self.modal.take() {
            Some(Modal::Select(select)) => select,
            // A Prompt submits via its own handler; a Confirm via `handle_confirm_key`
            Some(Modal::Prompt(_))
            | Some(Modal::Confirm(_))
            | Some(Modal::Info(_))
            | Some(Modal::Doctor(_))
            | None => {
                return;
            }
        };
        let chosen = select
            .state
            .selected()
            .and_then(|i| select.items.get(i).cloned());
        match select.kind {
            SelectKind::Launch => self.launch(chosen),
            SelectKind::Profile => self.switch_profile(chosen),
            SelectKind::Instance => self.switch_instance(chosen),
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

    /// Ask to remove the highlighted launch target
    fn confirm_remove_exe(&mut self) {
        let Some(Modal::Select(select)) = &self.modal else {
            return;
        };
        let Some(name) = select
            .state
            .selected()
            .and_then(|i| select.items.get(i).cloned())
        else {
            self.note("No launch target to remove");
            return;
        };
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Remove launch target {name}?"),
            action: ConfirmAction::RemoveExe(name),
        }));
    }

    /// Remove the launch target named `name`, persist, then reopen the picker
    pub(super) fn remove_exe(&mut self, name: &str) {
        if !self
            .session
            .instance
            .config
            .executables
            .iter()
            .any(|e| e.name == name)
        {
            self.fail(format!("No launch target named {name}"));
            return;
        }
        let snapshot = self.session.instance.config.executables.clone();
        self.session
            .instance
            .config
            .executables
            .retain(|e| e.name != name);
        if let Err(e) = self.session.instance.save() {
            self.session.instance.config.executables = snapshot; // keep memory == disk
            self.fail(format!("Could not save instance: {e}"));
            return;
        }
        self.open_select(SelectKind::Launch);
        self.ok(format!("Removed launch target {name}"));
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
                self.after_session_changed();
                self.focus = Focus::Mods;
                self.ok(format!("Switched to {name}"));
            }
            Err(e) => self.fail(format!("Error: {e}")),
        }
    }

    /// Switch to `chosen` with the current profile, re-opening the picker with a fail notice if load fails
    fn switch_instance(&mut self, chosen: Option<String>) {
        let Some(path) = chosen else {
            self.note("No other instances to switch to");
            return;
        };
        let dir = Utf8PathBuf::from(path);
        let profile_name = self.session.profile.name.clone();
        match Session::load(&dir, &profile_name) {
            Ok(session) => {
                self.session = session;
                self.after_session_changed();
                self.focus = Focus::Mods;
                self.settings.record_opened(&dir);
                if let Err(e) = self.settings.save() {
                    tracing::warn!(error = %e, "could not save settings");
                }
                self.ok("Switched instance");
            }
            Err(e) => {
                // Leave the picker visible so the failed switch is recoverable
                self.open_select(SelectKind::Instance);
                self.fail(format!("Error: {e}"));
            }
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
        // `n` is a profile-picker side-action only; in the launcher it's inert
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

    #[test]
    fn a_in_the_launch_picker_opens_the_add_exe_prompt() {
        use crate::app::{Prompt, PromptKind};
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('a')));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Prompt(Prompt {
                    kind: PromptKind::AddExe,
                    ..
                }))
            ),
            "a opens the add-exe prompt"
        );
    }

    #[test]
    fn x_in_the_launch_picker_confirms_removal_of_the_highlighted_target() {
        let mut app = App::sample();
        app.session.instance.config.executables = vec![overseer_core::instance::Executable {
            name: "FO4Edit".to_owned(),
            path: Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
            args: Vec::new(),
        }];
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('x')));
        match &app.modal {
            Some(Modal::Confirm(c)) => {
                assert!(
                    c.message.contains("FO4Edit"),
                    "the confirm names the target"
                );
                assert!(
                    matches!(&c.action, ConfirmAction::RemoveExe(n) if n == "FO4Edit"),
                    "x stages a RemoveExe confirm"
                );
            }
            _ => panic!("x opens a remove confirm"),
        }
    }

    #[test]
    fn x_on_an_empty_launch_picker_notes_and_stays_open() {
        let mut app = App::sample(); // the sample instance configures no exes
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('x')));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Launch,
                    ..
                }))
            ),
            "the picker stays open"
        );
        assert!(
            app.message.is_some(),
            "the user is told there is nothing to remove"
        );
    }

    #[test]
    fn confirming_removal_deletes_the_target_and_reopens_the_picker() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        let mut app = App::sample();
        app.session.instance = instance;
        std::fs::create_dir_all(&app.session.instance.root).unwrap();
        app.session.instance.config.executables = vec![
            overseer_core::instance::Executable {
                name: "game".to_owned(),
                path: Utf8PathBuf::from("game.exe"),
                args: Vec::new(),
            },
            overseer_core::instance::Executable {
                name: "FO4Edit".to_owned(),
                path: Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
                args: Vec::new(),
            },
        ];
        app.session.instance.save().unwrap();

        app.handle_key(key(KeyCode::Char('l'))); // picker opens on "game"
        app.handle_key(key(KeyCode::Char('x'))); // confirm remove "game"
        app.handle_key(key(KeyCode::Char('y'))); // accept

        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Launch,
                    ..
                }))
            ),
            "removal reopens the launch picker"
        );
        let names: Vec<_> = app
            .session
            .instance
            .config
            .executables
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, vec!["FO4Edit"], "the target is gone from memory");

        let reloaded =
            overseer_core::instance::Instance::load(app.session.instance.root.clone()).unwrap();
        assert_eq!(
            reloaded.config.executables.len(),
            1,
            "the removal is persisted to disk"
        );
    }

    #[test]
    fn a_failed_save_on_removal_rolls_the_target_back_in_memory() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        let mut app = App::sample();
        app.session.instance = instance;
        std::fs::create_dir_all(&app.session.instance.root).unwrap();
        app.session.instance.config.executables = vec![overseer_core::instance::Executable {
            name: "game".to_owned(),
            path: Utf8PathBuf::from("game.exe"),
            args: Vec::new(),
        }];
        app.session.instance.save().unwrap();
        // Delete the instance dir so the next save() fails mid-removal
        std::fs::remove_dir_all(&app.session.instance.root).unwrap();

        app.handle_key(key(KeyCode::Char('l'))); // picker opens on "game"
        app.handle_key(key(KeyCode::Char('x'))); // confirm remove "game"
        app.handle_key(key(KeyCode::Char('y'))); // accept → save fails

        assert_eq!(
            app.session
                .instance
                .config
                .executables
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            vec!["game"],
            "a failed save leaves the target in memory so it still matches disk"
        );
        assert!(
            app.message
                .as_ref()
                .is_some_and(|n| n.text.contains("Could not save")),
            "the failure is reported"
        );
    }

    #[test]
    fn s_opens_the_instance_picker_and_navigation_clamps() {
        let mut app = App::sample();
        let current = app.session.instance.root.to_string();
        app.handle_key(key(KeyCode::Char('s')));
        match &app.modal {
            Some(Modal::Select(s)) => {
                assert_eq!(s.kind, SelectKind::Instance, "s opens the instance picker");
                assert!(
                    !s.items.contains(&current),
                    "the current instance is excluded"
                );
            }
            _ => panic!("s opens a Select modal"),
        }
        assert_eq!(
            modal_selection(&app),
            Some(0),
            "opens on the first instance"
        );
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(modal_selection(&app), Some(1), "j moves down");
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(modal_selection(&app), Some(1), "clamps at the end");
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(modal_selection(&app), Some(0), "k moves up");
    }

    #[test]
    fn s_again_closes_the_instance_picker() {
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('s')));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Instance,
                    ..
                }))
            ),
            "s opens the instance select modal"
        );
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.modal.is_none(), "s again closes it");
    }

    #[test]
    fn switching_to_a_missing_instance_keeps_the_picker_open() {
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('s')));
        // The sample's recents point at directories with no instance, so the load fails
        app.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Instance,
                    ..
                }))
            ),
            "a failed switch leaves the instance picker open"
        );
        assert!(app.message.is_some(), "the failure is reported");
    }
}
