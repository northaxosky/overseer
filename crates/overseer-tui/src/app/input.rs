//! Keyboard handling and the actions it drives on [`App`].

use anyhow::Result;
use overseer_core::deploy::NullSink;
use overseer_core::instance::{InstanceError, ModKind};
use overseer_core::plugins::discover_plugins;
use overseer_core::{apply, launch};
use overseer_diagnostics::diagnose;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use super::{
    App, Focus, HELP_ENTRIES, Modal, Popup, Prompt, PromptKind, Select, SelectKind, Session,
    initial_selection,
};

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        // A modal blocks everything beneath it: it gets keys before popup or main
        if self.modal.is_some() {
            self.handle_modal_key(key);
            return;
        }
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
            KeyCode::Char('l') => self.open_select(SelectKind::Launch),
            KeyCode::Char('p') => self.open_select(SelectKind::Profile),

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

    /// Route a key while a model is open
    fn handle_modal_key(&mut self, key: KeyEvent) {
        match self.modal {
            Some(Modal::Select(_)) => self.handle_select_key(key),
            Some(Modal::Prompt(_)) => self.handle_prompt_key(key),
            None => {}
        }
    }

    /// Keys for Select modal: navigate the list, submit, or cancel
    fn handle_select_key(&mut self, key: KeyEvent) {
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

    /// Keys for Prompt modal: edit the line, submit, or cancel
    fn handle_prompt_key(&mut self, key: KeyEvent) {
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

    /// Route a key while a popup is open: Tab cycles tabs, everything else goes to the active tab
    fn handle_overlay_key(&mut self, tab: Popup, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.focus_tab(tab.cycle(1)),
            KeyCode::BackTab => self.focus_tab(tab.cycle(-1)),
            _ => match tab {
                Popup::Help => self.handle_help_key(key),
                Popup::Settings => self.handle_settings_key(key),
                Popup::Doctor => self.handle_doctor_key(key),
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

    /// Move the active Select modal's selection by `delta`, clamped to its items
    fn move_in_select(&mut self, delta: isize) {
        if let Some(Modal::Select(select)) = self.modal.as_mut() {
            move_in_list(&mut select.state, select.items.len(), delta);
        }
    }

    /// Open a Select modal of `kind`, selecting its first item
    fn open_select(&mut self, kind: SelectKind) {
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

    fn open_new_profile(&mut self) {
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
            }
            Err(e) => self.fail(format!("Error: {e}")),
        }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    /// The selected index of an open Select modal, or `None`
    fn modal_selection(app: &App) -> Option<usize> {
        match &app.modal {
            Some(Modal::Select(s)) => s.state.selected(),
            Some(Modal::Prompt(_)) | None => None,
        }
    }

    /// A key event with no modifiers.
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Open the profile picker, then the new-profile prompt, then type `name`.
    fn open_prompt_and_type(app: &mut App, name: &str) {
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('n')));
        for c in name.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    /// The open Prompt's input + error, or `None` when no prompt is open.
    fn prompt_state(app: &App) -> Option<(&str, Option<&str>)> {
        match &app.modal {
            Some(Modal::Prompt(p)) => Some((p.input.as_str(), p.error.as_deref())),
            _ => None,
        }
    }

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

    // --- new-profile prompt (Modal::Prompt) ---

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
