//! The Prompt modal: single-line text entry for new-profile, new-separator, rename-mod, rename-profile, and add-exe.

use camino::Utf8Path;
use overseer_core::apply;
use overseer_core::instance::{Executable, InstanceError, ModKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{
    App, Focus, ModPaneRow, Modal, Prompt, PromptKind, SelectKind, Session, separator_display,
};

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
                Some(PromptKind::EditExeArgs { index }) => self.submit_edit_exe_args(index),
                Some(PromptKind::EditExeName { index }) => self.submit_edit_exe_name(index),
                Some(PromptKind::RenameMod { old }) => self.submit_rename_mod(old),
                Some(PromptKind::RenameSeparator { index, .. }) => {
                    self.submit_rename_separator(index)
                }
                Some(PromptKind::NewSeparator) => self.submit_new_separator(),
                Some(PromptKind::NewPluginSeparator) => self.submit_new_plugin_separator(),
                Some(PromptKind::RenamePluginSeparator { index, .. }) => {
                    self.submit_rename_plugin_separator(index);
                }
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

    pub(super) fn open_new_separator(&mut self) {
        if self.focus == Focus::Mods {
            self.modal = Some(Modal::Prompt(Prompt {
                kind: PromptKind::NewSeparator,
                input: String::new(),
                error: None,
            }));
        } else if self.on_plugins_pane() {
            self.open_new_plugin_separator();
        } else {
            self.note("Switch to the mods or plugins pane to add a separator");
        }
    }

    pub(super) fn open_rename_mod(&mut self) {
        if self.on_plugins_pane() {
            self.open_rename_plugin_separator();
            return;
        }
        if self.focus != Focus::Mods {
            self.note("Switch to the mods or plugins pane to rename");
            return;
        }
        let rows = self.mods.project(&self.session.profile.mods);
        let Some(row) = self.mods.index().and_then(|index| rows.get(index)).copied() else {
            return;
        };
        let model_index = row.model_index();
        let entry = &self.session.profile.mods[model_index];
        match row {
            ModPaneRow::Separator { .. } => {
                self.modal = Some(Modal::Prompt(Prompt {
                    kind: PromptKind::RenameSeparator {
                        index: model_index,
                        name: separator_display(&entry.name).to_owned(),
                    },
                    input: String::new(),
                    error: None,
                }));
                return;
            }
            ModPaneRow::Mod { .. } if entry.kind != ModKind::Managed => {
                self.note("Only managed mods can be renamed");
                return;
            }
            ModPaneRow::Mod { .. } => {}
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

    /// Open the name step of editing the target at `index`, prefilled with curr name
    pub(super) fn open_edit_exe_name(&mut self, index: usize) {
        let Some(exe) = self.session.instance.config.executables.get(index) else {
            self.note("That launch target is gone");
            return;
        };
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::EditExeName { index },
            input: exe.name.clone(),
            error: None,
        }));
    }

    /// Apply the new name, then advance to the args step; stay open on error
    fn submit_edit_exe_name(&mut self, index: usize) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let name = prompt.input.trim().to_owned();
        match self.rename_exe(index, &name) {
            Ok(()) => self.open_edit_exe_args(index),
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Rename the target at `index`, validating uniqueness and persisting with rollback
    fn rename_exe(&mut self, index: usize, name: &str) -> Result<(), String> {
        validate_name(name)?;
        let exes = &self.session.instance.config.executables;
        if index >= exes.len() {
            return Err("That launch target is gone".to_owned());
        }
        if exes
            .iter()
            .enumerate()
            .any(|(i, e)| i != index && e.name == name)
        {
            return Err(format!("A launch target named {name} already exists"));
        }
        let prev = self.session.instance.config.executables[index].name.clone();
        self.session.instance.config.executables[index].name = name.to_owned();
        if let Err(e) = self.session.instance.save() {
            self.session.instance.config.executables[index].name = prev;
            return Err(format!("Could not save instance: {e}"));
        }
        Ok(())
    }

    /// Open the args step of editing the target at `index`, prefilled with curr args
    fn open_edit_exe_args(&mut self, index: usize) {
        let Some(exe) = self.session.instance.config.executables.get(index) else {
            self.note("That launch target is gone");
            return;
        };
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::EditExeArgs { index },
            input: exe.args.join(" "),
            error: None,
        }));
    }

    /// Apply the edited args, persist, and reopen picker
    fn submit_edit_exe_args(&mut self, index: usize) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let args: Vec<String> = prompt.input.split_whitespace().map(str::to_owned).collect();
        match self.set_exe_args(index, args) {
            Ok(name) => {
                self.reopen_select(SelectKind::Launch, &name);
                self.ok(format!("Updated launch target: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Set the args on the target at `index`, persist on rollback, return name
    fn set_exe_args(&mut self, index: usize, args: Vec<String>) -> Result<String, String> {
        if index >= self.session.instance.config.executables.len() {
            return Err("That launch target is gone".to_owned());
        }
        let prev = self.session.instance.config.executables[index].args.clone();
        self.session.instance.config.executables[index].args = args;
        let name = self.session.instance.config.executables[index].name.clone();
        if let Err(e) = self.session.instance.save() {
            self.session.instance.config.executables[index].args = prev;
            return Err(format!("Could not save instance: {e}"));
        }
        Ok(name)
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

    /// Insert the separator named in the open prompt above the selection; stay open on any error
    fn submit_new_separator(&mut self) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let name = prompt.input.trim().to_owned();
        match self.insert_selected_separator(&name) {
            Ok(()) => {
                self.modal = None;
                self.ok(format!("Added separator: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Insert a separator above the selection and persist; revert the in-memory insert if it fails
    fn insert_selected_separator(&mut self, name: &str) -> Result<(), String> {
        let rows = self.mods.project(&self.session.profile.mods);
        let anchor = self
            .mods
            .index()
            .and_then(|index| rows.get(index))
            .map_or(self.session.profile.mods.len(), |row| row.model_index() + 1);
        let separator_index = self.session.profile.mods[..anchor]
            .iter()
            .filter(|entry| entry.kind == ModKind::Separator)
            .count();
        self.session
            .profile
            .insert_separator(anchor, name)
            .map_err(|e| e.to_string())?;
        if let Err(e) = self.session.profile.save_modlist(&self.session.instance) {
            self.session.profile.mods.remove(anchor);
            return Err(format!("Could not save: {e}"));
        }
        self.mods.insert_separator(separator_index);
        let display = self
            .mods
            .project(&self.session.profile.mods)
            .iter()
            .position(|row| row.model_index() == anchor);
        self.mods.select(display);
        Ok(())
    }

    /// Validate then create a profile, mapping any failure to a user-facing message
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
                self.reopen_select(SelectKind::Launch, &name);
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
        // Store an absolute path so the target doesn't depend on the process cwd (matches `exe add`)
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

    /// Rename the selected mod to the name in the open prompt; stay open on any error
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

    /// Validate then rename a mod, mapping any failure to a user-facing message
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

    /// Rename the separator named in the open prompt; stay open on any error
    fn submit_rename_separator(&mut self, index: usize) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let name = prompt.input.trim().to_owned();
        match self.rename_selected_separator(index, &name) {
            Ok(()) => {
                self.modal = None;
                self.ok(format!("Renamed separator: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Rename the separator at `index` and persist; revert the in memory rename if the save fails
    fn rename_selected_separator(&mut self, index: usize, name: &str) -> Result<(), String> {
        let prev = self
            .session
            .profile
            .mods
            .get(index)
            .map(|m| m.name.clone())
            .ok_or_else(|| "That separator is gone".to_owned())?;
        self.session
            .profile
            .rename_separator(index, name)
            .map_err(|e| e.to_string())?;
        if let Err(e) = self.session.profile.save_modlist(&self.session.instance) {
            self.session.profile.mods[index].name = prev;
            return Err(format!("Could not save: {e}"));
        }
        let display = self
            .mods
            .project(&self.session.profile.mods)
            .iter()
            .position(|row| row.model_index() == index);
        self.mods.select(display);
        Ok(())
    }

    /// Open a rename prompt for the profile highlighted in the open Profile picker
    pub(super) fn open_rename_profile(&mut self) {
        let Some(Modal::Select(select)) = &self.modal else {
            return;
        };
        let Some(old) = select
            .state
            .index()
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

    /// Rename the profile to the name in the open prompt; stay open on any error
    fn submit_rename_profile(&mut self, old: String) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let new = prompt.input.trim().to_owned();
        if let Err(msg) = validate_name(&new) {
            self.set_prompt_error(msg);
            return;
        }

        // Distinguish "rename didn't happen" from "rename stuck"
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
            match Session::load(&root, Some(&new)) {
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

    /// Reopen a Select picker of `kind` with the entry named `name` highlighted
    fn reopen_select(&mut self, kind: SelectKind, name: &str) {
        self.open_select(kind);
        if let Some(Modal::Select(s)) = self.modal.as_mut()
            && let Some(i) = s.items.iter().position(|p| p == name)
        {
            s.state.select(Some(i));
        }
    }

    /// Reopen the Profile picker with `name` highlighted
    fn reopen_profiles_selecting(&mut self, name: &str) {
        self.reopen_select(SelectKind::Profile, name);
    }

    /// Show an inline error on the open prompt (no-op if no prompt is open)
    pub(super) fn set_prompt_error(&mut self, msg: String) {
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
    // Windows device names are reserved as a whole component, case-insensitively
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
        // Windows trims these, so "Foo." would create "Foo" and desync silently
        Err("Name cannot end with a space or '.'".to_owned())
    } else if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        Err("That name is reserved by Windows".to_owned())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/prompt.rs"]
mod tests;
