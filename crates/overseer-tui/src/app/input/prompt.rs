//! The Prompt modal: single-line text entry for new-profile, new-separator, rename-mod, rename-profile, and add-exe.

use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::apply;
use overseer_core::instance::{InstanceError, ModKind, ModRow};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{
    App, Focus, InstallJob, ModPaneRow, Modal, Prompt, PromptKind, SelectKind, Session,
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
                Some(PromptKind::EditExeArgs { key }) => self.submit_edit_exe_args(key),
                Some(PromptKind::EditExeName { key }) => self.submit_edit_exe_name(key),
                Some(PromptKind::RenameMod { old }) => self.submit_rename_mod(old),
                Some(PromptKind::RenameSeparator { index, .. }) => {
                    self.submit_rename_separator(index)
                }
                Some(PromptKind::NewSeparator) => self.submit_new_separator(),
                Some(PromptKind::NewPluginSeparator) => self.submit_new_plugin_separator(),
                Some(PromptKind::RenamePluginSeparator { index, .. }) => {
                    self.submit_rename_plugin_separator(index);
                }
                Some(PromptKind::InstallName { archive }) => self.submit_install(archive),
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
        let rows = self.mods.project(self.session.profile.rows());
        let Some(row) = self.mods.index().and_then(|index| rows.get(index)).copied() else {
            return;
        };
        let model_index = row.model_index();
        match row {
            ModPaneRow::Separator { name, .. } => {
                self.modal = Some(Modal::Prompt(Prompt {
                    kind: PromptKind::RenameSeparator {
                        index: model_index,
                        name: name.to_owned(),
                    },
                    input: String::new(),
                    error: None,
                }));
                return;
            }
            ModPaneRow::Mod { .. }
                if self
                    .session
                    .profile
                    .item_at_row(model_index)
                    .is_none_or(|entry| entry.kind != ModKind::Managed) =>
            {
                self.note("Only managed mods can be renamed");
                return;
            }
            ModPaneRow::Mod { .. } => {}
        }
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::RenameMod {
                old: self
                    .session
                    .profile
                    .item_at_row(model_index)
                    .expect("mod pane row maps to an item")
                    .name
                    .clone(),
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

    /// Open the name step of editing a user target
    pub(super) fn open_edit_exe_name(&mut self, key: String) {
        let Some(exe) = self
            .session
            .instance
            .config
            .tools
            .iter()
            .find(|tool| tool.id.as_str() == key)
        else {
            self.note("That launch target is gone");
            return;
        };
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::EditExeName { key },
            input: exe.name.clone(),
            error: None,
        }));
    }

    /// Apply the new name, then advance to the args step; stay open on error
    fn submit_edit_exe_name(&mut self, key: String) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let name = prompt.input.trim().to_owned();
        match self.rename_exe(&key, &name) {
            Ok(()) => self.open_edit_exe_args(key),
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Install the selected archive under the name typed into the prompt
    fn submit_install(&mut self, archive: Utf8PathBuf) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let name = prompt.input.trim().to_owned();
        if let Err(msg) = validate_name(&name) {
            self.set_prompt_error(msg);
            return;
        };
        let Some(basename) = archive.file_name().map(str::to_owned) else {
            self.set_prompt_error("Could not identify the archive basename".to_owned());
            return;
        };
        self.modal = None;
        self.start_operation(InstallJob::new(basename, name));
    }

    /// Rename a user target, validating uniqueness and persisting with rollback
    fn rename_exe(&mut self, key: &str, name: &str) -> Result<(), String> {
        validate_name(name)?;
        let snapshot = self.session.instance.config.tools.clone();
        self.session
            .instance
            .config
            .rename_tool(key, name.to_owned())
            .map_err(|error| error.to_string())?;
        if let Err(e) = self.session.instance.save() {
            self.session.instance.config.tools = snapshot;
            return Err(format!("Could not save instance: {e}"));
        }
        Ok(())
    }

    /// Open the args step of editing a user target
    fn open_edit_exe_args(&mut self, key: String) {
        let Some(exe) = self
            .session
            .instance
            .config
            .tools
            .iter()
            .find(|tool| tool.id.as_str() == key)
        else {
            self.note("That launch target is gone");
            return;
        };
        self.modal = Some(Modal::Prompt(Prompt {
            kind: PromptKind::EditExeArgs { key },
            input: exe.args.join(" "),
            error: None,
        }));
    }

    /// Apply the edited args, persist, and reopen picker
    fn submit_edit_exe_args(&mut self, key: String) {
        let Some(Modal::Prompt(prompt)) = self.modal.as_ref() else {
            return;
        };
        let args: Vec<String> = prompt.input.split_whitespace().map(str::to_owned).collect();
        match self.set_exe_args(&key, args) {
            Ok(name) => {
                self.reopen_select(SelectKind::Launch, &key);
                self.ok(format!("Updated launch target: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Set a user target's args, persist on rollback, return its name
    fn set_exe_args(&mut self, key: &str, args: Vec<String>) -> Result<String, String> {
        let snapshot = self.session.instance.config.tools.clone();
        self.session
            .instance
            .config
            .set_tool_args(key, args)
            .map_err(|error| error.to_string())?;
        let name = self
            .session
            .instance
            .config
            .tools
            .iter()
            .find(|tool| tool.id.as_str() == key)
            .expect("updated tool exists")
            .name
            .clone();
        if let Err(e) = self.session.instance.save() {
            self.session.instance.config.tools = snapshot;
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
        let rows = self.mods.project(self.session.profile.rows());
        let anchor = self
            .mods
            .index()
            .and_then(|index| rows.get(index))
            .map_or(self.session.profile.rows().len(), |row| {
                row.model_index() + 1
            });
        let separator_index = self.session.profile.rows()[..anchor]
            .iter()
            .filter(|row| matches!(row, ModRow::Separator(_)))
            .count();
        let mut profile = self.session.profile.clone();
        profile
            .insert_separator(anchor, name)
            .map_err(|e| e.to_string())?;
        if let Err(e) = profile.save_modlist(&self.session.instance) {
            return Err(format!("Could not save: {e}"));
        }
        self.session.profile = profile;
        self.mods.insert_separator(separator_index);
        let display = self
            .mods
            .project(self.session.profile.rows())
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
            Ok((name, key)) => {
                self.reopen_select(SelectKind::Launch, &key);
                self.ok(format!("Added launch target: {name}"));
            }
            Err(msg) => self.set_prompt_error(msg),
        }
    }

    /// Derive a name from the path's file stem, then add + persist the target
    fn add_named_exe(&mut self, path: &str) -> Result<(String, String), String> {
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
        let id = self
            .session
            .instance
            .config
            .add_tool(name.clone(), path, Vec::new())
            .map_err(|error| error.to_string())?;
        if let Err(e) = self.session.instance.save() {
            self.session.instance.config.tools.pop();
            return Err(format!("Could not save instance: {e}"));
        }
        Ok((name, id.to_string()))
    }

    /// Rename the selected mod to the name in the open prompt; stay open on any error
    fn submit_rename_mod(&mut self, old: String) {
        if self.block_while_playing("rename mods") {
            return;
        }
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
            .items_mut()
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
        let mut profile = self.session.profile.clone();
        profile
            .rename_separator(index, name)
            .map_err(|e| e.to_string())?;
        if let Err(e) = profile.save_modlist(&self.session.instance) {
            return Err(format!("Could not save: {e}"));
        }
        self.session.profile = profile;
        let display = self
            .mods
            .project(self.session.profile.rows())
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
        if self.block_while_playing("rename profiles") {
            return;
        }
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
            && let Some(i) = if s.kind == SelectKind::Launch {
                s.launch_rows.iter().position(|row| row.key == name)
            } else {
                s.items.iter().position(|item| item == name)
            }
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
