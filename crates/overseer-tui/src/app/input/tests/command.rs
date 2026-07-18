use super::*;
use crate::app::{DeployJob, OperationState, RefreshDownloadsJob};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn context(focus: Focus, workspace: Workspace) -> Context {
    Context { focus, workspace }
}

#[test]
fn key_table_maps_every_main_command() {
    let app = App::sample();
    let cases = vec![
        (KeyCode::Char('j'), Command::Move(1)),
        (KeyCode::Down, Command::Move(1)),
        (KeyCode::Char('k'), Command::Move(-1)),
        (KeyCode::Up, Command::Move(-1)),
        (KeyCode::Char('J'), Command::Reorder(1)),
        (KeyCode::Char('K'), Command::Reorder(-1)),
        (KeyCode::Tab, Command::ToggleFocus),
        (KeyCode::Char(' '), Command::ToggleSelected),
        (KeyCode::Enter, Command::ToggleSelected),
        (KeyCode::Char(']'), Command::CycleWorkspace(1)),
        (KeyCode::Char('['), Command::CycleWorkspace(-1)),
        (
            KeyCode::Char('1'),
            Command::SwitchWorkspace(Workspace::Plugins),
        ),
        (
            KeyCode::Char('2'),
            Command::SwitchWorkspace(Workspace::Conflicts),
        ),
        (
            KeyCode::Char('3'),
            Command::SwitchWorkspace(Workspace::Downloads),
        ),
        (
            KeyCode::Char('4'),
            Command::SwitchWorkspace(Workspace::Saves),
        ),
        (KeyCode::Char('l'), Command::OpenSelect(SelectKind::Launch)),
        (KeyCode::Char('p'), Command::OpenSelect(SelectKind::Profile)),
        (
            KeyCode::Char('s'),
            Command::OpenSelect(SelectKind::Instance),
        ),
        (KeyCode::Char('?'), Command::OpenHelp),
        (KeyCode::Char('d'), Command::OpenDoctor),
        (KeyCode::Char('R'), Command::OpenRenameMod),
        (KeyCode::Char('A'), Command::OpenNewSeparator),
        (KeyCode::Char('r'), Command::RefreshWorkspace),
        (KeyCode::Char('o'), Command::CycleSort),
        (KeyCode::Char('O'), Command::ToggleSortDir),
        (KeyCode::Char('X'), Command::DeleteSave),
        (KeyCode::Char('x'), Command::DeleteSeparator),
        (KeyCode::Delete, Command::DeleteSeparator),
        (KeyCode::Char('L'), Command::ToggleLocalSaves),
        (KeyCode::Char('f'), Command::FilterConflicts),
        (KeyCode::Char('g'), Command::JumpToProvider),
        (KeyCode::Char('m'), Command::RemoveMod),
        (KeyCode::Char('e'), Command::ReplaceMod),
        (KeyCode::Char('D'), Command::Deploy),
        (KeyCode::Char('P'), Command::Purge),
    ];

    for (code, expected) in cases {
        assert_eq!(app.command_for(key(code)), Some(expected), "{code:?}");
    }
}

#[test]
fn key_table_leaves_quit_and_unknown_keys_unmapped() {
    let app = App::sample();

    for code in [KeyCode::Char('q'), KeyCode::Esc, KeyCode::Char('z')] {
        assert_eq!(app.command_for(key(code)), None, "{code:?}");
    }
}

#[test]
fn busy_policy_covers_every_command() {
    let default_context = context(Focus::Mods, Workspace::Plugins);
    for command in [
        Command::Move(1),
        Command::Reorder(1),
        Command::ToggleFocus,
        Command::SwitchWorkspace(Workspace::Saves),
        Command::CycleWorkspace(1),
        Command::OpenHelp,
        Command::OpenDoctor,
        Command::OpenRenameMod,
        Command::OpenNewSeparator,
        Command::CycleSort,
        Command::ToggleSortDir,
        Command::DeleteSave,
        Command::DeleteSeparator,
        Command::ToggleLocalSaves,
        Command::FilterConflicts,
        Command::JumpToProvider,
        Command::RemoveMod,
        Command::ReplaceMod,
        Command::Deploy,
        Command::Purge,
    ] {
        let expected = match &command {
            Command::Move(_)
            | Command::ToggleFocus
            | Command::SwitchWorkspace(_)
            | Command::CycleWorkspace(_)
            | Command::OpenHelp
            | Command::CycleSort
            | Command::ToggleSortDir
            | Command::FilterConflicts
            | Command::JumpToProvider => BusyPolicy::Allowed,
            Command::OpenDoctor => BusyPolicy::Blocked(OperationKind::Doctor),
            Command::RemoveMod => BusyPolicy::Blocked(OperationKind::Remove),
            Command::ReplaceMod => BusyPolicy::Blocked(OperationKind::Replace),
            Command::Deploy => BusyPolicy::Blocked(OperationKind::Deploy),
            Command::Purge => BusyPolicy::Blocked(OperationKind::Purge),
            Command::Reorder(_)
            | Command::OpenRenameMod
            | Command::OpenNewSeparator
            | Command::DeleteSave
            | Command::DeleteSeparator
            | Command::ToggleLocalSaves => BusyPolicy::BlockedGeneric,
            Command::ToggleSelected | Command::OpenSelect(_) | Command::RefreshWorkspace => {
                unreachable!("context-sensitive commands have dedicated cases")
            }
        };
        assert_eq!(command.busy_policy(default_context), expected);
    }

    for command in [
        Command::OpenSelect(SelectKind::Launch),
        Command::OpenSelect(SelectKind::Profile),
        Command::OpenSelect(SelectKind::Instance),
    ] {
        assert_eq!(
            command.busy_policy(default_context),
            BusyPolicy::BlockedGeneric
        );
    }

    for (workspace, expected) in [
        (Workspace::Plugins, BusyPolicy::BlockedGeneric),
        (
            Workspace::Conflicts,
            BusyPolicy::Blocked(OperationKind::ScanConflicts),
        ),
        (
            Workspace::Downloads,
            BusyPolicy::Blocked(OperationKind::RefreshDownloads),
        ),
        (
            Workspace::Saves,
            BusyPolicy::Blocked(OperationKind::RefreshSaves),
        ),
    ] {
        assert_eq!(
            Command::RefreshWorkspace.busy_policy(context(Focus::Mods, workspace)),
            expected
        );
    }

    assert_eq!(
        Command::ToggleSelected.busy_policy(context(Focus::Workspace, Workspace::Downloads)),
        BusyPolicy::Blocked(OperationKind::Install)
    );
    for ctx in [
        context(Focus::Mods, Workspace::Downloads),
        context(Focus::Workspace, Workspace::Plugins),
        context(Focus::Workspace, Workspace::Conflicts),
        context(Focus::Workspace, Workspace::Saves),
    ] {
        assert_eq!(
            Command::ToggleSelected.busy_policy(ctx),
            BusyPolicy::BlockedGeneric
        );
    }
}

#[test]
fn enter_and_space_toggle_when_idle_and_are_gated_when_busy() {
    let mut app = App::sample();
    let original = app.session.profile.rows().to_vec();

    app.handle_key(key(KeyCode::Enter));
    assert_ne!(app.session.profile.rows(), original);
    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.session.profile.rows(), original);

    app.workspace = Workspace::Downloads;
    app.focus = Focus::Workspace;
    app.start_operation(RefreshDownloadsJob);
    for code in [KeyCode::Enter, KeyCode::Char(' ')] {
        app.handle_key(key(code));
        assert!(
            app.message
                .as_ref()
                .is_some_and(|notice| notice.text.contains("Downloads refresh is running"))
        );
    }
    app.finish_operation_after_terminal();
}

#[test]
fn enter_dismisses_a_completed_operation_before_dispatching() {
    let mut app = App::sample();
    app.start_operation(RefreshDownloadsJob);
    app.finish_operation_after_terminal();
    assert!(matches!(app.operation, OperationState::Completed(_)));
    app.note("keep me");

    app.handle_key(key(KeyCode::Enter));

    assert!(matches!(app.operation, OperationState::Idle));
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("keep me")
    );
}

#[test]
fn esc_preserves_its_idle_and_busy_priority_order() {
    let mut idle = App::sample();
    idle.note("idle");
    idle.handle_key(key(KeyCode::Esc));
    assert!(idle.should_quit);
    assert_eq!(
        idle.message.as_ref().map(|notice| notice.text.as_str()),
        Some("idle")
    );

    let mut idle_filtered = App::sample();
    idle_filtered.workspace = Workspace::Conflicts;
    idle_filtered.conflicts.filter = Some("mod".to_owned());
    idle_filtered.note("idle filter");
    idle_filtered.handle_key(key(KeyCode::Esc));
    assert!(!idle_filtered.should_quit);
    assert!(idle_filtered.conflicts.filter.is_none());
    assert_eq!(
        idle_filtered
            .message
            .as_ref()
            .map(|notice| notice.text.as_str()),
        Some("idle filter")
    );

    let mut read_only = App::sample();
    read_only.start_operation(RefreshDownloadsJob);
    read_only.handle_key(key(KeyCode::Esc));
    assert!(read_only.should_quit);
    read_only.finish_operation_after_terminal();

    let mut read_only_filtered = App::sample();
    read_only_filtered.workspace = Workspace::Conflicts;
    read_only_filtered.conflicts.filter = Some("mod".to_owned());
    read_only_filtered.start_operation(RefreshDownloadsJob);
    read_only_filtered.handle_key(key(KeyCode::Esc));
    assert!(!read_only_filtered.should_quit);
    assert!(read_only_filtered.conflicts.filter.is_none());
    read_only_filtered.finish_operation_after_terminal();

    let mut mutating = App::sample();
    mutating.start_operation(DeployJob);
    mutating.handle_key(key(KeyCode::Esc));
    assert!(!mutating.should_quit);
    assert!(
        mutating
            .message
            .as_ref()
            .is_some_and(|notice| notice.text.contains("running"))
    );
    mutating.finish_operation_after_terminal();

    let mut mutating_filtered = App::sample();
    mutating_filtered.workspace = Workspace::Conflicts;
    mutating_filtered.conflicts.filter = Some("mod".to_owned());
    mutating_filtered.start_operation(DeployJob);
    mutating_filtered.handle_key(key(KeyCode::Esc));
    assert!(!mutating_filtered.should_quit);
    assert_eq!(mutating_filtered.conflicts.filter.as_deref(), Some("mod"));
    mutating_filtered.finish_operation_after_terminal();
}

#[test]
fn filtering_conflicts_remains_available_during_read_only_work() {
    let mut app = App::sample();
    app.workspace = Workspace::Conflicts;
    app.start_operation(RefreshDownloadsJob);

    app.handle_key(key(KeyCode::Char('f')));

    assert_eq!(app.conflicts.filter.as_deref(), Some("OffMod"));
    app.finish_operation_after_terminal();
}

#[test]
fn idle_remove_and_replace_outside_mods_are_silent_no_ops() {
    for code in [KeyCode::Char('m'), KeyCode::Char('e')] {
        let mut app = App::sample();
        app.focus = Focus::Workspace;
        app.note("clear me");

        app.handle_key(key(code));

        assert!(app.modal.is_none(), "no modal opens outside the Mods pane");
        assert!(
            app.message.is_none(),
            "an accepted key still clears the notice"
        );
    }
}

#[test]
fn unknown_keys_clear_notices_but_early_returns_preserve_them() {
    let mut app = App::sample();
    app.note("unknown");
    app.handle_key(key(KeyCode::Char('z')));
    assert!(app.message.is_none());

    app.note("quit");
    app.handle_key(key(KeyCode::Char('q')));
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("quit")
    );

    app.start_operation(RefreshDownloadsJob);
    app.note("busy unknown");
    app.handle_key(key(KeyCode::Char('z')));
    assert!(app.message.is_none());
    app.finish_operation_after_terminal();
}
