//! The saves workspace's actions: listing the profile's `.fos` saves and deleting one

use crate::app::sort::sort_saves;
use crate::app::{App, Confirm, ConfirmAction, Focus, Modal, Workspace, select_first};
use camino::Utf8Path;
use overseer_core::saves::{self, SaveInfo};

impl App {
    /// List the current profile's saves in the saved sort order, selecting the first row
    pub(super) fn refresh_saves(&mut self) {
        let listed = self
            .session
            .instance
            .saves_dir(&self.session.profile.name)
            .map_err(|e| format!("Could not locate saves: {e}"))
            .and_then(|dir| {
                saves::list_saves(&dir).map_err(|e| format!("Could not list saves: {e}"))
            });
        match listed {
            Ok(mut entries) => {
                sort_saves(&mut entries, self.settings.saves_sort);
                select_first(&mut self.saves.list, entries.len());
                self.saves.entries = entries;
            }
            Err(msg) => {
                self.saves.entries.clear();
                self.saves.list.select(None);
                self.fail(msg);
            }
        }
    }

    /// The currently selected save entry, if any
    fn selected_save(&self) -> Option<&SaveInfo> {
        let i = self.saves.list.selected()?;
        self.saves.entries.get(i)
    }

    /// Confirm deleting the selected save; inert unless the Saves pane is focused
    pub(super) fn begin_delete_selected_save(&mut self) {
        // `x` is a main-view key, so guard it to the one pane it acts on
        if !self.on_saves_pane() {
            return;
        }
        let Some(save) = self.selected_save() else {
            return;
        };

        // Copy out what the confirm needs so we stop borrowing `self.saves`
        let file_name = save.file_name.clone();
        let path = save.path.clone();
        self.modal = Some(Modal::Confirm(Confirm {
            message: format!("Delete {file_name}? This cannot be undone."),
            action: ConfirmAction::DeleteSave(path),
        }));
    }

    /// Delete the save at `path`, re-list, and keep the selection near where it was
    pub(super) fn delete_selected_save(&mut self, path: &Utf8Path) {
        let name = path.file_name().unwrap_or(path.as_str()).to_owned();
        let prev = self.saves.list.selected().unwrap_or(0);
        match saves::delete_save(path) {
            Ok(()) => {
                self.refresh_saves();
                // The deleted row is gone; clamp the selection to the new bounds
                let len = self.saves.entries.len();
                self.saves.list.select((len > 0).then(|| prev.min(len - 1)));
                self.ok(format!("Deleted {name}"));
            }
            Err(e) => self.fail(format!("Delete failed: {e}")),
        }
    }

    /// Toggle the current profile's LocalSaves flag; inert unless the Saves pane is focused
    pub(super) fn toggle_local_saves(&mut self) {
        if !self.on_saves_pane() {
            return;
        }
        self.session.profile.local_saves = !self.session.profile.local_saves;
        match self.session.profile.save(&self.session.instance) {
            Ok(()) => {
                let state = if self.session.profile.local_saves {
                    "on"
                } else {
                    "off"
                };
                self.ok(format!("Local saves {state}"));
            }
            Err(e) => {
                self.session.profile.local_saves = !self.session.profile.local_saves;
                self.fail(format!("Could not save profile: {e}"));
            }
        }
    }

    /// True when the Saves workspace pane is focused
    fn on_saves_pane(&self) -> bool {
        self.focus == Focus::Workspace && self.workspace == Workspace::Saves
    }
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use crate::app::input::test_helpers::key;
    use crate::app::{App, Focus, Modal, Session, Workspace};
    use overseer_core::instance::Instance;
    use overseer_core::settings::{SavesSort, SavesSortKey, SortDir};
    use overseer_core::test_support::{self, temp_instance};
    use ratatui::crossterm::event::KeyCode;

    /// A temp instance plus its `App`, with `count` saves seeded for `profile`
    fn app_with_saves(profile: &str, count: u32) -> (tempfile::TempDir, App) {
        let (tmp, instance) = temp_instance();
        let dir = instance.saves_dir(profile).expect("saves dir");
        for n in 1..=count {
            test_support::write_fos(
                &dir.join(format!("Save{n}.fos")),
                n,
                "Nora",
                10 + n,
                "Sanctuary",
                "Day 1",
            );
        }
        let mut app = App::sample();
        app.session.instance = instance;
        (tmp, app)
    }

    #[test]
    fn pressing_4_switches_to_saves_and_lists_them() {
        let (_tmp, mut app) = app_with_saves("Default", 1);

        app.handle_key(key(KeyCode::Char('4')));

        assert_eq!(app.workspace, Workspace::Saves, "4 switches workspace");
        assert_eq!(app.focus, Focus::Mods, "switching never moves focus");
        assert_eq!(app.saves.entries.len(), 1, "the profile's save is listed");
        assert_eq!(app.saves.list.selected(), Some(0), "first row selected");
    }

    #[test]
    fn capital_x_on_a_save_opens_a_confirm_without_deleting() {
        let (_tmp, mut app) = app_with_saves("Default", 1);
        let save = app
            .session
            .instance
            .saves_dir("Default")
            .unwrap()
            .join("Save1.fos");

        app.handle_key(key(KeyCode::Char('4')));
        app.focus = Focus::Workspace;
        app.handle_key(key(KeyCode::Char('X')));

        match &app.modal {
            Some(Modal::Confirm(c)) => assert!(
                c.message.contains("Delete Save1.fos"),
                "the confirm names the save"
            ),
            other => panic!("expected a confirm modal, got {other:?}"),
        }
        assert!(
            save.exists(),
            "nothing is deleted until the confirm is accepted"
        );
    }

    #[test]
    fn confirming_deletes_the_save_relists_and_clamps_selection() {
        let (_tmp, mut app) = app_with_saves("Default", 2);
        app.handle_key(key(KeyCode::Char('4')));
        app.focus = Focus::Workspace;
        app.saves.list.select(Some(1)); // the second, newest-ordered row

        let doomed = app.saves.entries[1].path.clone();
        app.handle_key(key(KeyCode::Char('X')));
        app.handle_key(key(KeyCode::Char('y')));

        assert!(app.modal.is_none(), "the confirm closes after accepting");
        assert!(!doomed.exists(), "the save file is removed");
        assert_eq!(app.saves.entries.len(), 1, "the list is refreshed");
        assert_eq!(
            app.saves.list.selected(),
            Some(0),
            "the selection clamps into the shorter list"
        );
        assert!(
            app.message
                .as_ref()
                .is_some_and(|n| n.text.contains("Deleted")),
            "a success notice is shown"
        );
    }

    #[test]
    fn deleting_removes_the_script_extender_co_save() {
        let (_tmp, mut app) = app_with_saves("Default", 1);
        let dir = app.session.instance.saves_dir("Default").unwrap();
        let co_save = dir.join("Save1.f4se");
        test_support::write(&co_save, "co-save");

        app.handle_key(key(KeyCode::Char('4')));
        app.focus = Focus::Workspace;
        app.handle_key(key(KeyCode::Char('X')));
        app.handle_key(key(KeyCode::Char('y')));

        assert!(
            !co_save.exists(),
            "the co-save is removed alongside the .fos"
        );
    }

    #[test]
    fn toggling_local_saves_flips_and_persists() {
        let (_tmp, mut app) = app_with_saves("Default", 1);
        app.handle_key(key(KeyCode::Char('4')));
        app.focus = Focus::Workspace;

        let name = app.session.profile.name.clone();
        let before = app.session.profile.local_saves;
        app.handle_key(key(KeyCode::Char('L')));

        assert_eq!(app.session.profile.local_saves, !before, "L flips the flag");
        assert!(
            app.message
                .as_ref()
                .is_some_and(|n| n.text.contains("Local saves")),
            "a status notice is shown"
        );
        // Persisted: reloading from disk reflects the new value
        let reloaded =
            overseer_core::instance::Profile::load(&app.session.instance, &name).unwrap();
        assert_eq!(
            reloaded.local_saves, !before,
            "the toggle is written to disk"
        );
    }

    #[test]
    fn local_saves_toggle_is_inert_off_the_saves_pane() {
        let (_tmp, mut app) = app_with_saves("Default", 1);
        // Still focused on Mods, not the Saves workspace
        let before = app.session.profile.local_saves;
        app.handle_key(key(KeyCode::Char('L')));
        assert_eq!(
            app.session.profile.local_saves, before,
            "inert unless the Saves pane is focused"
        );
    }

    #[test]
    fn switching_profile_while_on_saves_relists_for_the_new_profile() {
        // A real on-disk instance so the profile switch (Session::load) works
        let (_tmp, scaffold) = temp_instance();
        let instance =
            Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
        instance.create_profile("Default").expect("default profile");
        instance.create_profile("Other").expect("other profile");
        // Only the Other profile has saves on disk
        test_support::write_fos(
            &instance.saves_dir("Other").unwrap().join("Low.fos"),
            1,
            "Nate",
            5,
            "Vault 111",
            "Day 0",
        );
        test_support::write_fos(
            &instance.saves_dir("Other").unwrap().join("High.fos"),
            2,
            "Nate",
            10,
            "Concord",
            "Day 2",
        );

        let mut app = App::sample();
        app.session = Session::load(&instance.root, "Default").expect("session");
        app.settings.saves_sort = SavesSort {
            key: SavesSortKey::Level,
            dir: SortDir::Desc,
        };

        app.handle_key(key(KeyCode::Char('4')));
        assert!(app.saves.entries.is_empty(), "Default has no saves yet");

        // Open the profile picker and switch to Other (sorted second)
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.session.profile.name, "Other", "the profile switched");
        assert_eq!(app.workspace, Workspace::Saves, "still on the Saves pane");
        let names: Vec<&str> = app
            .saves
            .entries
            .iter()
            .map(|e| e.file_name.as_str())
            .collect();
        assert_eq!(
            names,
            ["High.fos", "Low.fos"],
            "the list refreshed and re-applied the saved sort"
        );
    }
}
