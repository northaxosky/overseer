//! The Prompt modal: the new-profile name entry.

use overseer_core::instance::InstanceError;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Modal, Prompt, PromptKind, SelectKind};

impl App {
    /// Keys for Prompt modal: edit the line, submit, or cancel
    pub(super) fn handle_prompt_key(&mut self, key: KeyEvent) {
        match key.code {
            // cancel returns to the picker the prompt was opened from
            KeyCode::Esc => self.open_select(SelectKind::Profile),
            KeyCode::Enter => self.submit_new_profile(),
            KeyCode::Backspace => {
                if let Some(Modal::Prompt(prompt)) = self.modal.as_mut() {
                    prompt.input.pop();
                    prompt.error = None;
                }
            }
            // Accept any ordinary printable char
            KeyCode::Char(c) if !c.is_control() => {
                if let Some(Modal::Prompt(prompt)) = self.modal.as_mut()
                    && prompt.input.len() < 64
                {
                    prompt.input.push(c);
                    prompt.error = None;
                }
            }
            _ => {}
        }
    }

    pub(super) fn open_new_profile(&mut self) {
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::NewProfile,
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

        // Compute the outcome first, then touch the modal once below — so we never
        // hold a borrow of `self.modal` across a `&mut self` call.
        match self.create_named_profile(&name) {
            Ok(()) => {
                self.open_select(SelectKind::Profile);
                if let Some(Modal::Select(s)) = self.modal.as_mut()
                    && let Some(i) = s.items.iter().position(|p| p == &name)
                {
                    s.state.select(Some(i));
                }
                self.ok(format!("Created profile: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Validate then create a profile, mapping any failure to a user-facing message.
    fn create_named_profile(&self, name: &str) -> Result<(), String> {
        validate_profile_name(name)?;
        match self.session.instance.create_profile(name) {
            Ok(_) => Ok(()),
            Err(InstanceError::ProfileExists(_)) => {
                Err(format!("A profile named {name} already exists"))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Show an inline error on the open prompt (no-op if no prompt is open).
    fn set_prompt_error(&mut self, msg: String) {
        if let Some(Modal::Prompt(prompt)) = self.modal.as_mut() {
            prompt.error = Some(msg);
        }
    }
}

fn validate_profile_name(name: &str) -> Result<(), String> {
    const BAD: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    // Windows device names are reserved as a whole component, case-insensitively.
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if name.is_empty() {
        Err("Name cannot be empty".to_owned())
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
    fn validate_profile_name_rejects_windows_unsafe_names() {
        // Windows strips a trailing dot/space, so these would create a different
        // directory than requested and silently desync from Profile.name.
        assert!(validate_profile_name("Foo.").is_err(), "trailing dot");
        assert!(validate_profile_name("Foo ").is_err(), "trailing space");
        // Reserved device names are rejected as a whole, case-insensitively.
        assert!(validate_profile_name("nul").is_err(), "reserved, lowercase");
        assert!(
            validate_profile_name("COM1").is_err(),
            "reserved, uppercase"
        );
    }

    #[test]
    fn validate_profile_name_allows_an_interior_space() {
        assert!(validate_profile_name("Survival Build").is_ok());
    }
}
