//! The Prompt modal: the new-profile name entry.

use camino::Utf8Path;
use overseer_core::apply;
use overseer_core::instance::{Executable, InstanceError, ModKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Focus, Modal, Prompt, PromptKind, SelectKind, Session};

impl App {
    /// Keys for Prompt modal: edit the line, submit, or cancel
    pub(super) fn handle_prompt_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => match self.open_prompt_kind().and_then(|kind| kind.cancel_to()) {
                Some(kind) => self.open_select(kind),
                None => self.modal = None,
            },
            KeyCode::Enter => match self.open_prompt_kind() {
                Some(PromptKind::NewProfile) => self.submit_new_profile(),
                Some(PromptKind::RenameProfile { old }) => self.submit_rename_profile(old),
                Some(PromptKind::AddExe) => self.submit_add_exe(),
                Some(PromptKind::RenameMod { old }) => self.submit_rename_mod(old),
                None => {}
            },
            KeyCode::Backspace => {
                if let Some(Modal::Prompt(prompt)) = self.modal.as_mut() {
                    prompt.input.pop();
                    prompt.error = None;
                }
            }
            // Accept any ordinary printable char
            KeyCode::Char(c) if !c.is_control() => {
                if let Some(Modal::Prompt(prompt)) = self.modal.as_mut()
                    && prompt.input.len() < prompt.kind.max_len()
                {
                    prompt.input.push(c);
                    prompt.error = None;
                }
            }
            _ => {}
        }
    }

    fn open_prompt_kind(&self) -> Option<PromptKind> {
        match &self.modal {
            Some(Modal::Prompt(prompt)) => Some(prompt.kind.clone()),
            _ => None,
        }
    }

    pub(super) fn open_new_profile(&mut self) {
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::NewProfile,
            input: String::new(),
            error: None,
        }));
    }

    pub(super) fn open_rename_mod(&mut self) {
        if self.focus != Focus::Mods {
            self.note("Switch to the mods pane to rename a mod");
            return;
        }
        let Some(i) = self.mods_state.selected() else {
            return;
        };
        let entry = &self.session.profile.mods[i];
        if entry.kind != ModKind::Managed {
            self.note("Only managed mods can be renamed");
            return;
        }
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::RenameMod {
                old: entry.name.clone(),
            },
            input: String::new(),
            error: None,
        }));
    }

    pub(super) fn open_add_exe(&mut self) {
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::AddExe,
            input: String::new(),
            error: None,
        }));
    }

    /// Create the profile named in the open prompt; stay open on any error
    fn submit_new_profile(&mut self) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let name = prompt.input.trim().to_owned();

        // Compute the outcome first
        match self.create_named_profile(&name) {
            Ok(()) => {
                self.reopen_profiles_selecting(&name);
                self.ok(format!("Created profile: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Validate then create a profile, mapping any failure to a user-facing message.
    fn create_named_profile(&self, name: &str) -> Result<(), String> {
        validate_name(name)?;
        match self.session.instance.create_profile(name) {
            Ok(_) => Ok(()),
            Err(InstanceError::ProfileExists(_)) => {
                Err(format!("A profile named {name} already exists"))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Add the launch target at the path in the open prompt; stay open on any error
    fn submit_add_exe(&mut self) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let path = prompt.input.trim().to_owned();

        match self.add_named_exe(&path) {
            Ok(name) => {
                self.open_select(SelectKind::Launch);
                if let Some(Modal::Select(s)) = self.modal.as_mut()
                    && let Some(i) = s.items.iter().position(|p| p == &name)
                {
                    s.state.select(Some(i));
                }
                self.ok(format!("Added launch target: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Derive a name from the path's file stem, then add + persist the target
    fn add_named_exe(&mut self, path: &str) -> Result<String, String> {
        if path.is_empty() {
            return Err("Path cannot be empty".to_owned());
        }
        // Store an absolute path so the target doesn't depend on the process cwd (matches `exe add`).
        let path = overseer_frontend::absolutize(Utf8Path::new(path))
            .map_err(|e| format!("Invalid path: {e}"))?;
        let name = path
            .file_stem()
            .filter(|stem| !stem.is_empty())
            .ok_or_else(|| "Could not derive a name from that path".to_owned())?
            .to_owned();
        validate_name(&name)?;
        if self
            .session
            .instance
            .config
            .executables
            .iter()
            .any(|e| e.name == name)
        {
            return Err(format!("A launch target named {name} already exists"));
        }
        self.session.instance.config.executables.push(Executable {
            name: name.clone(),
            path,
            args: Vec::new(),
        });
        if let Err(e) = self.session.instance.save() {
            self.session.instance.config.executables.pop();
            return Err(format!("Could not save instance: {e}"));
        }
        Ok(name)
    }

    fn submit_rename_mod(&mut self, old: String) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let new = prompt.input.trim().to_owned();

        match self.rename_selected_mod(&old, &new) {
            Ok(()) => {
                self.modal = None;
                self.ok(format!("Renamed {old} \u{2192} {new}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Validate then rename a mod, mapping any failure to a user-facing message.
    fn rename_selected_mod(&mut self, old: &str, new: &str) -> Result<(), String> {
        validate_name(new)?;
        apply::rename_mod(&self.session.instance, old, new).map_err(rename_error_message)?;
        if let Some(entry) = self
            .session
            .profile
            .mods
            .iter_mut()
            .find(|entry| entry.name.eq_ignore_ascii_case(old))
        {
            entry.name = new.to_owned();
        }
        self.mark_conflicts_stale();
        Ok(())
    }

    /// Open a rename prompt for the profile highlighted in the open Profile picker
    pub(super) fn open_rename_profile(&mut self) {
        let Some(Modal::Select(select)) = &self.modal else {
            return;
        };
        let Some(old) = select
            .state
            .selected()
            .and_then(|i| select.items.get(i).cloned())
        else {
            self.note("No profile to rename");
            return;
        };
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::RenameProfile { old },
            input: String::new(),
            error: None,
        }));
    }

    fn submit_rename_profile(&mut self, old: String) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let new = prompt.input.trim().to_owned();
        if let Err(msg) = validate_name(&new) {
            self.set_prompt_error(msg);
            return;
        }

        // Distinguish "rename didnt happen" from "rename stuck"
        let warning = match apply::rename_profile(&mut self.session.instance, &old, &new) {
            Ok(()) => None,
            Err(apply::ApplyError::DefaultProfileNotUpdated(e)) => Some(format!(
                "Renamed, but couldn't update the default profile: {e}"
            )),
            Err(e) => {
                self.set_prompt_error(rename_profile_error_message(e));
                return;
            }
        };

        // The rename is committed on disk
        if self.session.profile.name.eq_ignore_ascii_case(&old) {
            let root = self.session.instance.root.clone();
            match Session::load(&root, &new) {
                Ok(session) => {
                    self.session = session;
                    self.after_session_changed();
                }
                Err(e) => {
                    self.reopen_profiles_selecting(&new);
                    self.fail(format!("Renamed profile, but reloading it failed: {e}"));
                    return;
                }
            }
        }
        self.reopen_profiles_selecting(&new);
        match warning {
            Some(w) => self.fail(w),
            None => self.ok(format!("Renamed profile {old} \u{2192} {new}")),
        }
    }

    /// Reopen the Profile picker with `name` highlighted
    fn reopen_profiles_selecting(&mut self, name: &str) {
        self.open_select(SelectKind::Profile);
        if let Some(Modal::Select(s)) = self.modal.as_mut()
            && let Some(i) = s.items.iter().position(|p| p == name)
        {
            s.state.select(Some(i));
        }
    }

    /// Show an inline error on the open prompt (no-op if no prompt is open).
    fn set_prompt_error(&mut self, msg: String) {
        if let Some(Modal::Prompt(prompt)) = self.modal.as_mut() {
            prompt.error = Some(msg);
        }
    }
}

fn rename_error_message(error: apply::ApplyError) -> String {
    match error {
        apply::ApplyError::DeployedCannotRename { .. } => "Purge before renaming mods".to_owned(),
        apply::ApplyError::Instance(InstanceError::ModAlreadyInstalled(name)) => {
            format!("A mod named {name} is already installed")
        }
        apply::ApplyError::Instance(InstanceError::ModAlreadyInList(_)) => {
            "A profile already lists both mod names".to_owned()
        }
        apply::ApplyError::Instance(InstanceError::ModNotInstalled(name)) => {
            format!("No installed mod named {name}")
        }
        apply::ApplyError::Instance(InstanceError::InvalidModName(msg)) => msg,
        other => other.to_string(),
    }
}

fn rename_profile_error_message(error: apply::ApplyError) -> String {
    match error {
        apply::ApplyError::DeployedCannotRename { .. } => {
            "Purge before renaming the deployed profile".to_owned()
        }
        apply::ApplyError::Instance(InstanceError::ProfileExists(name)) => {
            format!("A profile named {name} already exists")
        }
        apply::ApplyError::Instance(InstanceError::ProfileNotFound(name)) => {
            format!("No profile named {name}")
        }
        apply::ApplyError::Instance(InstanceError::InvalidProfileName(msg)) => msg,
        other => other.to_string(),
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    const BAD: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    // Windows device names are reserved as a whole component, case-insensitively.
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if name.is_empty() {
        Err("Name cannot be empty".to_owned())
    } else if name.chars().count() > 64 {
        Err("Name cannot be longer than 64 characters".to_owned())
    } else if name.contains("..") || name.contains(BAD) || name.contains(char::is_control) {
        Err("Name cannot contain .. or any of / \\ : * ? \" < > |".to_owned())
    } else if name.ends_with('.') || name.ends_with(' ') {
        // Windows trims these, so "Foo." would create "Foo" and desync silently.
        Err("Name cannot end with a space or '.'".to_owned())
    } else if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        Err("That name is reserved by Windows".to_owned())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Select;
    use crate::app::input::test_helpers::*;
    use overseer_core::test_support::{install_mod, save_profile};
    use ratatui::widgets::ListState;

    #[test]
    fn n_in_the_profile_picker_opens_the_new_profile_prompt() {
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('p')));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Profile,
                    ..
                }))
            ),
            "p opens the profile picker"
        );
        app.handle_key(key(KeyCode::Char('n')));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Prompt(Prompt {
                    kind: PromptKind::NewProfile,
                    ..
                }))
            ),
            "n opens the new-profile prompt"
        );
    }

    #[test]
    fn r_in_the_profile_picker_opens_a_rename_prompt_for_the_highlighted_profile() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        instance.create_profile("Default").expect("create Default");
        let mut app = App::sample();
        app.session.instance = instance;

        app.handle_key(key(KeyCode::Char('p'))); // profile picker
        app.handle_key(key(KeyCode::Char('r'))); // rename side-action

        match &app.modal {
            Some(Modal::Prompt(Prompt {
                kind: PromptKind::RenameProfile { old },
                input,
                error,
            })) => {
                assert_eq!(old, "Default");
                assert_eq!(input, "");
                assert!(error.is_none());
            }
            other => panic!("expected a rename-profile prompt, got {other:?}"),
        }
    }

    #[test]
    fn submitting_a_valid_profile_rename_moves_it_on_disk_and_reopens_the_picker() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        instance.create_profile("Default").expect("create Default");
        instance.create_profile("Extra").expect("create Extra");
        let mut app = App::sample();
        app.session.instance = instance;

        app.handle_key(key(KeyCode::Char('p'))); // picker: [Default, Extra]
        app.handle_key(key(KeyCode::Down)); // highlight the non-active "Extra"
        app.handle_key(key(KeyCode::Char('r')));
        for c in "Extra2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert!(app.session.instance.profile_dir("Extra2").is_dir());
        assert!(!app.session.instance.profile_dir("Extra").exists());
        match &app.modal {
            Some(Modal::Select(s)) => {
                assert!(
                    s.items.iter().any(|p| p == "Extra2"),
                    "picker reopened with the new name"
                );
                assert!(!s.items.iter().any(|p| p == "Extra"));
            }
            other => panic!("expected the reopened profile picker, got {other:?}"),
        }
        assert!(app.message.is_some(), "an ok notice is shown");
    }

    #[test]
    fn renaming_the_active_profile_reloads_the_session_under_the_new_name() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        instance.create_profile("Default").expect("create Default");
        let mut app = App::sample();
        app.session.instance = instance;

        app.handle_key(key(KeyCode::Char('p'))); // picker: [Default] (the active profile)
        app.handle_key(key(KeyCode::Char('r')));
        for c in "Main".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(
            app.session.profile.name, "Main",
            "the active session reloaded under the new name"
        );
        assert!(app.session.instance.profile_dir("Main").is_dir());
    }

    #[test]
    fn submitting_an_invalid_profile_rename_keeps_the_prompt_with_error() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        instance.create_profile("Default").expect("create Default");
        let mut app = App::sample();
        app.session.instance = instance;

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('r')));
        for c in "a/b".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert!(matches!(prompt_state(&app), Some(("a/b", Some(_)))));
    }

    #[test]
    fn submitting_a_duplicate_profile_rename_keeps_the_prompt_with_error() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        instance.create_profile("Default").expect("create Default");
        instance.create_profile("Extra").expect("create Extra");
        let mut app = App::sample();
        app.session.instance = instance;

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Down)); // Extra
        app.handle_key(key(KeyCode::Char('r')));
        for c in "Default".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert!(matches!(prompt_state(&app), Some(("Default", Some(_)))));
        assert!(
            app.session.instance.profile_dir("Extra").is_dir(),
            "nothing moved on collision"
        );
    }

    #[test]
    fn typing_and_backspace_edit_the_prompt_input() {
        let mut app = App::sample();
        open_prompt_and_type(&mut app, "Surv");
        assert_eq!(prompt_state(&app), Some(("Surv", None)));
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(
            prompt_state(&app),
            Some(("Sur", None)),
            "backspace pops a char"
        );
    }

    #[test]
    fn esc_from_the_prompt_returns_to_the_profile_picker() {
        let mut app = App::sample();
        open_prompt_and_type(&mut app, "Whatever");
        app.handle_key(key(KeyCode::Esc));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Profile,
                    ..
                }))
            ),
            "Esc cancels back to the picker it came from"
        );
    }

    #[test]
    fn submitting_an_empty_name_sets_an_inline_error_and_keeps_the_prompt() {
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Enter)); // input is empty
        match prompt_state(&app) {
            Some((input, error)) => {
                assert_eq!(input, "", "input is preserved");
                assert!(error.is_some(), "an inline error is shown");
            }
            None => panic!("the prompt must stay open on a validation error"),
        }
    }

    #[test]
    fn submitting_a_name_with_a_path_separator_is_rejected_inline() {
        let mut app = App::sample();
        open_prompt_and_type(&mut app, "a/b");
        app.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(prompt_state(&app), Some(("a/b", Some(_)))),
            "a path-dangerous name keeps the prompt open with an error"
        );
    }

    #[test]
    fn submitting_a_valid_name_creates_the_profile_and_returns_to_the_picker() {
        // create_profile writes to disk, so back the session with a temp instance.
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        let mut app = App::sample();
        app.session.instance = instance;

        open_prompt_and_type(&mut app, "Survival");
        app.handle_key(key(KeyCode::Enter));

        match &app.modal {
            Some(Modal::Select(s)) => {
                let i = s
                    .items
                    .iter()
                    .position(|p| p == "Survival")
                    .expect("new profile is listed");
                assert_eq!(s.state.selected(), Some(i), "the new profile is selected");
            }
            _ => panic!("a successful create returns to the profile picker"),
        }
        assert!(
            app.session.instance.profile_dir("Survival").is_dir(),
            "the profile exists on disk"
        );
        assert!(app.message.is_some(), "an ok notice is shown");
    }

    #[test]
    fn submitting_a_duplicate_name_keeps_the_prompt_with_an_error() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        instance.create_profile("Default").expect("seed a profile");
        let mut app = App::sample();
        app.session.instance = instance;

        open_prompt_and_type(&mut app, "Default");
        app.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(prompt_state(&app), Some(("Default", Some(_)))),
            "a duplicate keeps the prompt open with an inline error"
        );
    }

    #[test]
    fn validate_name_rejects_windows_unsafe_names() {
        // Windows strips a trailing dot/space, so these would create a different
        // directory than requested and silently desync from Profile.name.
        assert!(validate_name("Foo.").is_err(), "trailing dot");
        assert!(validate_name("Foo ").is_err(), "trailing space");
        // Reserved device names are rejected as a whole, case-insensitively.
        assert!(validate_name("nul").is_err(), "reserved, lowercase");
        assert!(validate_name("COM1").is_err(), "reserved, uppercase");
    }

    #[test]
    fn validate_name_allows_an_interior_space() {
        assert!(validate_name("Survival Build").is_ok());
    }

    #[test]
    fn r_on_a_managed_mod_opens_an_empty_rename_prompt() {
        let mut app = App::sample();

        app.handle_key(key(KeyCode::Char('R')));

        match &app.modal {
            Some(Modal::Prompt(Prompt {
                kind: PromptKind::RenameMod { old },
                input,
                error,
            })) => {
                assert_eq!(old, "CoolMod");
                assert_eq!(input, "");
                assert!(error.is_none());
            }
            other => panic!("expected rename prompt, got {other:?}"),
        }
    }

    #[test]
    fn r_on_unrenameable_rows_or_plugins_pane_is_a_note() {
        let mut app = App::sample();
        app.session
            .profile
            .mods
            .push(overseer_core::instance::ModListEntry {
                name: "DLC".to_owned(),
                enabled: true,
                kind: ModKind::Foreign,
            });
        app.mods_state.select(Some(2));

        app.handle_key(key(KeyCode::Char('R')));

        assert!(app.modal.is_none());
        assert!(app.message.is_some());

        app.session.profile.mods[2].kind = ModKind::Separator;
        app.message = None;
        app.handle_key(key(KeyCode::Char('R')));
        assert!(app.modal.is_none());
        assert!(app.message.is_some());

        app.focus = Focus::Workspace;
        app.message = None;
        app.handle_key(key(KeyCode::Char('R')));
        assert!(app.modal.is_none());
        assert!(app.message.is_some());
    }

    #[test]
    fn submitting_a_valid_mod_rename_updates_memory_and_keeps_selection() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
        save_profile(&instance, "Default", &[("CoolMod", true)]);
        let mut app = App::sample();
        app.session.instance = instance;
        app.mods_state.select(Some(0));

        app.handle_key(key(KeyCode::Char('R')));
        for c in "BetterMod".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert!(app.modal.is_none(), "successful rename closes prompt");
        assert_eq!(app.session.profile.mods[0].name, "BetterMod");
        assert_eq!(app.mods_state.selected(), Some(0));
        assert!(app.message.is_some(), "an ok notice is shown");
    }

    #[test]
    fn submitting_an_invalid_mod_rename_keeps_the_prompt_with_error() {
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('R')));
        for c in "a/b".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert!(
            matches!(prompt_state(&app), Some(("a/b", Some(_)))),
            "invalid name stays inline"
        );
    }

    #[test]
    fn submitting_a_duplicate_mod_rename_keeps_the_prompt_with_error() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
        install_mod(&instance, "Existing", &[("Textures/b.dds", "pixels")]);
        save_profile(&instance, "Default", &[("CoolMod", true)]);
        let mut app = App::sample();
        app.session.instance = instance;
        app.mods_state = ListState::default();
        app.mods_state.select(Some(0));

        app.handle_key(key(KeyCode::Char('R')));
        for c in "Existing".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert!(
            matches!(prompt_state(&app), Some(("Existing", Some(_)))),
            "duplicate name stays inline"
        );
        assert_eq!(app.session.profile.mods[0].name, "CoolMod");
    }

    #[test]
    fn submitting_a_path_adds_a_derived_launch_target() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        let mut app = App::sample();
        app.session.instance = instance;
        std::fs::create_dir_all(&app.session.instance.root).unwrap();

        app.handle_key(key(KeyCode::Char('l'))); // launch picker (empty)
        app.handle_key(key(KeyCode::Char('a'))); // add-exe prompt
        for c in "C:/Tools/FO4Edit.exe".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        // The name is derived from the file stem and selected in the reopened picker.
        match &app.modal {
            Some(Modal::Select(s)) => {
                let i = s
                    .items
                    .iter()
                    .position(|p| p == "FO4Edit")
                    .expect("the derived target is listed");
                assert_eq!(s.state.selected(), Some(i), "the new target is selected");
            }
            _ => panic!("a successful add returns to the launch picker"),
        }
        let reloaded =
            overseer_core::instance::Instance::load(app.session.instance.root.clone()).unwrap();
        let names: Vec<_> = reloaded
            .config
            .executables
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, vec!["FO4Edit"], "the derived target is persisted");
    }

    #[test]
    fn esc_from_the_add_exe_prompt_returns_to_the_launch_picker() {
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Esc));
        assert!(
            matches!(
                app.modal,
                Some(Modal::Select(Select {
                    kind: SelectKind::Launch,
                    ..
                }))
            ),
            "Esc cancels back to the launch picker"
        );
    }

    #[test]
    fn submitting_a_duplicate_derived_name_keeps_the_prompt_with_an_error() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        let mut app = App::sample();
        app.session.instance = instance;
        app.session.instance.config.executables = vec![Executable {
            name: "FO4Edit".to_owned(),
            path: camino::Utf8PathBuf::from("C:/old/FO4Edit.exe"),
            args: Vec::new(),
        }];

        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('a')));
        for c in "D:/new/FO4Edit.exe".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        match prompt_state(&app) {
            Some((input, error)) => {
                assert_eq!(input, "D:/new/FO4Edit.exe", "the input is preserved");
                assert!(error.is_some(), "a duplicate derived name errors inline");
            }
            None => panic!("the prompt stays open on a duplicate"),
        }
    }

    #[test]
    fn a_relative_path_is_absolutized_before_it_is_stored() {
        let (_tmp, instance) = overseer_core::test_support::temp_instance();
        let mut app = App::sample();
        app.session.instance = instance;
        std::fs::create_dir_all(&app.session.instance.root).unwrap();

        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('a')));
        for c in "tools/MyTool.exe".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        // A relative path is resolved against cwd, so config never stores a cwd-dependent path.
        let exe = app
            .session
            .instance
            .config
            .executables
            .iter()
            .find(|e| e.name == "MyTool")
            .expect("the target was added under its derived name");
        assert!(exe.path.is_absolute(), "the stored path is absolutized");
    }
}
