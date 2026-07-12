//! Tests for operation result reduction

use super::*;

use camino::Utf8PathBuf;
use overseer_core::apply::{self, DeploymentStatus};
use overseer_core::deploy::{FileConflict, NullSink};
use overseer_core::instance::{Instance, ModKind, ModListEntry};
use overseer_core::settings::{DownloadsSort, DownloadsSortKey, SavesSort, SavesSortKey, SortDir};
use overseer_core::test_support::{install_mod, save_profile, temp_instance};
use overseer_diagnostics::{Finding, Report};

use super::super::protocol::{OperationFailure, OperationKind, OperationRecovery};
use crate::app::{Focus, Info, Modal, Session, Workspace};
use crate::test_support::{download_entry, save_info};

fn completion(app: &App, outcome: Result<OperationOutput, OperationFailure>) -> WorkerCompletion {
    WorkerCompletion {
        context: OperationContext::capture(&app.session),
        outcome,
    }
}

fn conflict(relative: &str, winner: &str) -> FileConflict {
    FileConflict {
        relative: Utf8PathBuf::from(relative),
        providers: vec!["Loser".to_owned(), winner.to_owned()],
    }
}

fn session_with_mod(name: &str) -> Session {
    let mut session = App::sample().session;
    session.profile.mods = vec![ModListEntry {
        name: name.to_owned(),
        enabled: false,
        kind: ModKind::Managed,
    }];
    session.order.plugins.clear();
    session.discovered.clear();
    session
}

fn live_status() -> (tempfile::TempDir, DeploymentStatus) {
    let (temp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone())
        .expect("initialize instance");
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    apply::deploy_profile(&instance, "Default", &NullSink).expect("deploy fixture");
    let status = apply::status(&instance)
        .expect("read status")
        .expect("live deployment");
    (temp, status)
}

fn seed_ephemeral_view_state(app: &mut App) {
    app.session.profile.mods.push(ModListEntry {
        name: "Group_separator".to_owned(),
        enabled: false,
        kind: ModKind::Separator,
    });
    app.mods.reset(&app.session.profile.mods);
    app.mods.toggle_separator(0);
    app.mods.select(Some(1));
    app.plugins.select(Some(1));
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Saves;
    app.conflicts.status = ConflictsStatus::Ready(vec![conflict("cached.dds", "Winner")]);
    app.downloads.entries = vec![download_entry("Cached.zip", 1, 1, false)];
    app.saves.entries = vec![save_info("Cached.fos", 1, None)];
}

fn assert_ephemeral_view_state(app: &App) {
    assert_eq!(app.mods.index(), Some(1));
    assert_eq!(app.plugins.index(), Some(1));
    assert_eq!(app.focus, Focus::Workspace);
    assert_eq!(app.workspace, Workspace::Saves);
    assert!(
        app.mods
            .project(&app.session.profile.mods)
            .iter()
            .any(|row| matches!(
                row,
                crate::app::ModPaneRow::Separator {
                    collapsed: true,
                    ..
                }
            ))
    );
    assert!(matches!(
        app.conflicts.status,
        ConflictsStatus::Ready(ref found) if found[0].relative == "cached.dds"
    ));
    assert_eq!(app.downloads.entries[0].name, "Cached.zip");
    assert_eq!(app.saves.entries[0].file_name, "Cached.fos");
}

#[test]
fn same_context_applies_and_profile_or_root_changes_discard() {
    let mut same = App::sample();
    let result = completion(
        &same,
        Ok(OperationOutput::RefreshDownloads(vec![download_entry(
            "Same.zip", 1, 1, false,
        )])),
    );
    same.apply_completion(OperationKind::RefreshDownloads, result);
    assert_eq!(same.downloads.entries[0].name, "Same.zip");

    let mut changed_profile = App::sample();
    let result = completion(
        &changed_profile,
        Ok(OperationOutput::RefreshDownloads(vec![download_entry(
            "Stale.zip",
            1,
            1,
            false,
        )])),
    );
    changed_profile.session.profile.name = "Other".to_owned();
    changed_profile.apply_completion(OperationKind::RefreshDownloads, result);
    assert!(changed_profile.downloads.entries.is_empty());
    assert!(matches!(
        changed_profile.operation,
        OperationState::Completed(CompletedOperation {
            ref message,
            ..
        }) if message.contains("active session changed")
    ));

    let mut changed_root = App::sample();
    let result = completion(
        &changed_root,
        Ok(OperationOutput::RefreshDownloads(vec![download_entry(
            "Stale.zip",
            1,
            1,
            false,
        )])),
    );
    changed_root.session.instance.root = Utf8PathBuf::from("different-instance");
    changed_root.apply_completion(OperationKind::RefreshDownloads, result);
    assert!(changed_root.downloads.entries.is_empty());
    assert!(matches!(
        changed_root.operation,
        OperationState::Completed(_)
    ));
}

#[test]
fn deploy_success_updates_only_status_and_persistent_completion() {
    let (_temp, status) = live_status();
    let mut app = App::sample();
    seed_ephemeral_view_state(&mut app);
    let root_before = app.session.instance.root.clone();
    let mods_before = app.session.profile.mods.clone();
    let plugins_before = app.session.order.plugins.clone();
    let discovered_before = app.session.discovered.clone();
    let separators_before = app.session.plugin_separators.items.clone();
    let result = completion(
        &app,
        Ok(OperationOutput::Deploy {
            status: Some(status),
            files: 1,
        }),
    );

    app.apply_completion(OperationKind::Deploy, result);

    assert!(app.session.status.is_some());
    assert_eq!(app.session.instance.root, root_before);
    assert_eq!(app.session.profile.mods, mods_before);
    assert_eq!(app.session.order.plugins, plugins_before);
    assert_eq!(app.session.discovered, discovered_before);
    assert_eq!(app.session.plugin_separators.items, separators_before);
    assert_ephemeral_view_state(&app);
    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            kind: OperationKind::Deploy,
            succeeded: true,
            ref message,
        }) if message == "Deployed 1 files"
    ));
}

#[test]
fn purge_success_clears_only_status_and_preserves_view_state() {
    let (_temp, status) = live_status();
    let mut app = App::sample();
    app.session.status = Some(status);
    seed_ephemeral_view_state(&mut app);
    let mods_before = app.session.profile.mods.clone();
    let plugins_before = app.session.order.plugins.clone();
    let result = completion(&app, Ok(OperationOutput::Purge(None)));

    app.apply_completion(OperationKind::Purge, result);

    assert!(app.session.status.is_none());
    assert_eq!(app.session.profile.mods, mods_before);
    assert_eq!(app.session.order.plugins, plugins_before);
    assert_ephemeral_view_state(&app);
    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            kind: OperationKind::Purge,
            succeeded: true,
            ref message,
        }) if message == "Purged the live deployment"
    ));
}

#[test]
fn mutation_failure_applies_typed_status_recovery() {
    let (_temp, status) = live_status();
    let mut app = App::sample();
    app.session.status = None;
    let result = completion(
        &app,
        Err(OperationFailure {
            message: "Deploy failed: primary".to_owned(),
            recovery: Some(OperationRecovery::DeploymentStatus(Some(Box::new(status)))),
            recovery_error: None,
        }),
    );

    app.apply_completion(OperationKind::Deploy, result);

    assert!(app.session.status.is_some());
    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            succeeded: false,
            ref message,
            ..
        }) if message == "Deploy failed: primary"
    ));
}

#[test]
fn mutation_failure_keeps_primary_and_secondary_recovery_errors() {
    let mut app = App::sample();
    let result = completion(
        &app,
        Err(OperationFailure {
            message: "Purge failed: primary".to_owned(),
            recovery: None,
            recovery_error: Some(
                "deployment status recovery failed: secondary refresh error".to_owned(),
            ),
        }),
    );

    app.apply_completion(OperationKind::Purge, result);

    let OperationState::Completed(completed) = &app.operation else {
        panic!("failure remains persistent")
    };
    assert!(completed.message.starts_with("Purge failed: primary"));
    assert!(completed.message.contains("secondary refresh error"));
}

#[test]
fn install_success_accepts_session_and_downloads_without_resetting_other_panes() {
    let mut app = App::sample();
    app.mods.select(Some(1));
    app.plugins.select(Some(1));
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Saves;
    app.conflicts.status = ConflictsStatus::Ready(vec![conflict("cached.dds", "Winner")]);
    *app.conflicts.list.state_mut().offset_mut() = 2;
    app.downloads.entries = vec![download_entry("Old.zip", 1, 1, false)];
    app.saves.entries = vec![save_info("Cached.fos", 1, None)];
    app.saves.list.select(Some(0));
    *app.saves.list.state_mut().offset_mut() = 4;
    let result = completion(
        &app,
        Ok(OperationOutput::Install {
            session: Box::new(session_with_mod("Installed")),
            name: "Installed".to_owned(),
            downloads: vec![download_entry("Installed.zip", 2, 2, true)],
        }),
    );

    app.apply_completion(OperationKind::Install, result);

    assert_eq!(app.session.profile.mods[0].name, "Installed");
    assert_eq!(app.mods.index(), Some(0));
    assert_eq!(app.plugins.index(), None);
    assert_eq!(app.downloads.entries[0].name, "Installed.zip");
    assert!(app.downloads.entries[0].installed);
    assert_eq!(app.saves.entries[0].file_name, "Cached.fos");
    assert_eq!(app.saves.list.state_mut().offset(), 4);
    assert_eq!(app.conflicts.list.state_mut().offset(), 2);
    assert!(matches!(app.conflicts.status, ConflictsStatus::Stale));
    assert_eq!(app.focus, Focus::Workspace);
    assert_eq!(app.workspace, Workspace::Saves);
    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            kind: OperationKind::Install,
            succeeded: true,
            ref message,
        }) if message == "Installed Installed"
    ));
}

#[test]
fn failed_install_accepts_session_recovery_and_keeps_primary_failure() {
    let mut app = App::sample();
    app.downloads.entries = vec![download_entry("Cached.zip", 1, 1, false)];
    app.conflicts.status = ConflictsStatus::Ready(Vec::new());
    let result = completion(
        &app,
        Err(OperationFailure {
            message: "Install failed: primary".to_owned(),
            recovery: Some(OperationRecovery::Session(Box::new(session_with_mod(
                "Recovered",
            )))),
            recovery_error: None,
        }),
    );

    app.apply_completion(OperationKind::Install, result);

    assert_eq!(app.session.profile.mods[0].name, "Recovered");
    assert_eq!(app.downloads.entries[0].name, "Cached.zip");
    assert!(matches!(app.conflicts.status, ConflictsStatus::Stale));
    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            kind: OperationKind::Install,
            succeeded: false,
            ref message,
        }) if message == "Install failed: primary"
    ));
}

#[test]
fn non_install_failure_ignores_session_recovery() {
    let mut app = App::sample();
    let original_mods = app.session.profile.mods.clone();
    let result = completion(
        &app,
        Err(OperationFailure {
            message: "Deploy failed: primary".to_owned(),
            recovery: Some(OperationRecovery::Session(Box::new(session_with_mod(
                "MustNotApply",
            )))),
            recovery_error: None,
        }),
    );

    app.apply_completion(OperationKind::Deploy, result);

    assert_eq!(app.session.profile.mods, original_mods);
}

#[test]
fn failed_install_keeps_primary_and_secondary_session_errors() {
    let mut app = App::sample();
    let result = completion(
        &app,
        Err(OperationFailure {
            message: "Installed Mod, but reloading failed: primary".to_owned(),
            recovery: None,
            recovery_error: Some("session recovery failed: secondary".to_owned()),
        }),
    );

    app.apply_completion(OperationKind::Install, result);

    let OperationState::Completed(completed) = &app.operation else {
        panic!("failure remains persistent")
    };
    assert_eq!(
        completed.message,
        "Installed Mod, but reloading failed: primary; session recovery failed: secondary"
    );
}

#[test]
fn successful_conflict_result_replaces_cache_and_selects_the_first_row() {
    let mut app = App::sample();
    app.conflicts.status = ConflictsStatus::Ready(vec![conflict("old.dds", "Old")]);
    let result = completion(
        &app,
        Ok(OperationOutput::ScanConflicts(vec![
            conflict("a.dds", "Alpha"),
            conflict("b.dds", "Beta"),
        ])),
    );

    app.apply_completion(OperationKind::ScanConflicts, result);

    let ConflictsStatus::Ready(found) = &app.conflicts.status else {
        panic!("conflicts are ready")
    };
    assert_eq!(
        found
            .iter()
            .map(|entry| entry.relative.as_str())
            .collect::<Vec<_>>(),
        ["a.dds", "b.dds"]
    );
    assert_eq!(app.conflicts.list.index(), Some(0));
    assert!(matches!(app.operation, OperationState::Idle));
}

#[test]
fn failed_conflict_scan_preserves_cached_results_and_selection() {
    let mut app = App::sample();
    app.conflicts.status =
        ConflictsStatus::Ready(vec![conflict("a.dds", "Alpha"), conflict("b.dds", "Beta")]);
    app.conflicts.list.select(Some(1));
    let result = completion(&app, Err(OperationFailure::new("scan stopped")));

    app.apply_completion(OperationKind::ScanConflicts, result);

    let ConflictsStatus::Ready(found) = &app.conflicts.status else {
        panic!("cached conflicts remain ready")
    };
    assert_eq!(found[1].relative.as_str(), "b.dds");
    assert_eq!(app.conflicts.list.index(), Some(1));
    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            kind: OperationKind::ScanConflicts,
            succeeded: false,
            ref message,
        }) if message == "scan stopped"
    ));
}

#[test]
fn profile_global_conflicts_accept_after_selection_and_workspace_changes() {
    let mut app = App::sample();
    let result = completion(
        &app,
        Ok(OperationOutput::ScanConflicts(vec![conflict(
            "shared.dds",
            "Winner",
        )])),
    );
    app.mods.select(Some(1));
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Saves;

    app.apply_completion(OperationKind::ScanConflicts, result);

    assert!(matches!(
        app.conflicts.status,
        ConflictsStatus::Ready(ref found) if found[0].relative.as_str() == "shared.dds"
    ));
    assert!(matches!(app.operation, OperationState::Idle));
}

#[test]
fn doctor_success_opens_a_selected_report_and_returns_idle() {
    let mut app = App::sample();
    let result = completion(
        &app,
        Ok(OperationOutput::Doctor(Report::new(vec![Finding::info(
            "Healthy",
        )]))),
    );

    app.apply_completion(OperationKind::Doctor, result);

    let Some(Modal::Doctor(doctor)) = &app.modal else {
        panic!("Doctor modal opens")
    };
    assert_eq!(doctor.report.findings[0].title, "Healthy");
    assert_eq!(doctor.list.index(), Some(0));
    assert!(matches!(app.operation, OperationState::Idle));
}

#[test]
fn empty_doctor_success_still_opens_the_modal() {
    let mut app = App::sample();
    let result = completion(&app, Ok(OperationOutput::Doctor(Report::new(Vec::new()))));

    app.apply_completion(OperationKind::Doctor, result);

    let Some(Modal::Doctor(doctor)) = &app.modal else {
        panic!("empty Doctor modal opens")
    };
    assert!(doctor.report.findings.is_empty());
    assert_eq!(doctor.list.index(), None);
    assert!(matches!(app.operation, OperationState::Idle));
}

#[test]
fn doctor_success_replaces_help_opened_while_it_was_running() {
    let mut app = App::sample();
    let result = completion(
        &app,
        Ok(OperationOutput::Doctor(Report::new(vec![Finding::info(
            "Result",
        )]))),
    );
    app.modal = Some(Modal::Info(Info {
        title: "Help".to_owned(),
        entries: vec![("?".to_owned(), "help".to_owned())],
        state: ListCursor::first(1),
    }));

    app.apply_completion(OperationKind::Doctor, result);

    assert!(matches!(app.modal, Some(Modal::Doctor(_))));
    assert!(matches!(app.operation, OperationState::Idle));
}

#[test]
fn doctor_failure_opens_no_modal_and_remains_persistent() {
    let mut app = App::sample();
    let result = completion(&app, Err(OperationFailure::new("diagnostics stopped")));

    app.apply_completion(OperationKind::Doctor, result);

    assert!(app.modal.is_none());
    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            kind: OperationKind::Doctor,
            succeeded: false,
            ref message,
        }) if message == "diagnostics stopped"
    ));
}

#[test]
fn changed_profile_or_root_discards_doctor_reports() {
    for change_root in [false, true] {
        let mut app = App::sample();
        let result = completion(
            &app,
            Ok(OperationOutput::Doctor(Report::new(vec![Finding::info(
                "Stale",
            )]))),
        );
        if change_root {
            app.session.instance.root = Utf8PathBuf::from("different-instance");
        } else {
            app.session.profile.name = "Other".to_owned();
        }

        app.apply_completion(OperationKind::Doctor, result);

        assert!(app.modal.is_none());
        assert!(matches!(
            app.operation,
            OperationState::Completed(CompletedOperation {
                kind: OperationKind::Doctor,
                ref message,
                ..
            }) if message.contains("active session changed")
        ));
    }
}

#[test]
fn focus_workspace_and_selection_changes_do_not_discard() {
    let mut app = App::sample();
    let result = completion(
        &app,
        Ok(OperationOutput::RefreshDownloads(vec![download_entry(
            "Kept.zip", 1, 1, false,
        )])),
    );
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Saves;
    app.mods.select(Some(1));

    app.apply_completion(OperationKind::RefreshDownloads, result);

    assert_eq!(app.downloads.entries[0].name, "Kept.zip");
    assert!(matches!(app.operation, OperationState::Idle));
}

#[test]
fn successful_saves_result_applies_silently() {
    let mut app = App::sample();
    let result = completion(
        &app,
        Ok(OperationOutput::RefreshSaves(vec![save_info(
            "Save1.fos",
            1,
            None,
        )])),
    );

    app.apply_completion(OperationKind::RefreshSaves, result);

    assert_eq!(app.saves.entries[0].file_name, "Save1.fos");
    assert!(matches!(app.operation, OperationState::Idle));
    assert!(app.message.is_none(), "read-only success adds no notice");
}

#[test]
fn latest_save_sort_wins_at_acceptance() {
    let mut app = App::sample();
    let result = completion(
        &app,
        Ok(OperationOutput::RefreshSaves(vec![
            save_info("Beta.fos", 2, None),
            save_info("Alpha.fos", 1, None),
        ])),
    );
    app.settings.saves_sort = SavesSort {
        key: SavesSortKey::Name,
        dir: SortDir::Asc,
    };

    app.apply_completion(OperationKind::RefreshSaves, result);

    assert_eq!(
        app.saves
            .entries
            .iter()
            .map(|entry| entry.file_name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha.fos", "Beta.fos"]
    );
}

#[test]
fn selected_save_path_survives_result_reordering() {
    let mut app = App::sample();
    app.saves.entries = vec![
        save_info("Alpha.fos", 1, None),
        save_info("Beta.fos", 2, None),
    ];
    app.saves.list.select(Some(0));
    let selected_path = app.saves.entries[0].path.clone();
    let result = completion(
        &app,
        Ok(OperationOutput::RefreshSaves(vec![
            save_info("Alpha.fos", 1, None),
            save_info("Beta.fos", 2, None),
        ])),
    );

    app.apply_completion(OperationKind::RefreshSaves, result);

    assert_eq!(app.saves.list.index(), Some(1));
    assert_eq!(
        app.saves.entries[app.saves.list.index().expect("selection")].path,
        selected_path
    );
}

#[test]
fn missing_selected_save_path_clamps_the_prior_numeric_index() {
    let mut app = App::sample();
    app.saves.entries = vec![
        save_info("A.fos", 3, None),
        save_info("B.fos", 2, None),
        save_info("Gone.fos", 1, None),
    ];
    app.saves.list.select(Some(2));
    let result = completion(
        &app,
        Ok(OperationOutput::RefreshSaves(vec![
            save_info("A.fos", 3, None),
            save_info("B.fos", 2, None),
        ])),
    );

    app.apply_completion(OperationKind::RefreshSaves, result);

    assert_eq!(app.saves.list.index(), Some(1));
}

#[test]
fn failed_saves_refresh_preserves_cache_and_selection() {
    let mut app = App::sample();
    app.saves.entries = vec![
        save_info("A.fos", 2, None),
        save_info("Cached.fos", 1, None),
    ];
    app.saves.list.select(Some(1));
    let selected_path = app.saves.entries[1].path.clone();
    let result = completion(&app, Err(OperationFailure::new("test worker stopped")));

    app.apply_completion(OperationKind::RefreshSaves, result);

    assert_eq!(app.saves.entries.len(), 2);
    assert_eq!(app.saves.list.index(), Some(1));
    assert_eq!(app.saves.entries[1].path, selected_path);
    assert!(matches!(app.operation, OperationState::Completed(_)));
}

#[test]
fn latest_download_sort_and_selected_path_win_at_acceptance() {
    let mut app = App::sample();
    app.downloads.entries = vec![
        download_entry("Alpha.zip", 1, 1, false),
        download_entry("Beta.zip", 3, 2, false),
    ];
    app.downloads.list.select(Some(1));
    let result = completion(
        &app,
        Ok(OperationOutput::RefreshDownloads(vec![
            download_entry("Beta.zip", 3, 2, false),
            download_entry("Alpha.zip", 1, 1, false),
        ])),
    );
    app.settings.downloads_sort = DownloadsSort {
        key: DownloadsSortKey::Size,
        dir: SortDir::Asc,
    };

    app.apply_completion(OperationKind::RefreshDownloads, result);

    assert_eq!(
        app.downloads
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha.zip", "Beta.zip"]
    );
    assert_eq!(app.downloads.list.index(), Some(1), "Beta remains selected");
}

#[test]
fn missing_selected_path_clamps_the_prior_numeric_index() {
    let mut app = App::sample();
    app.downloads.entries = vec![
        download_entry("A.zip", 1, 1, false),
        download_entry("B.zip", 1, 1, false),
        download_entry("Gone.zip", 1, 1, false),
    ];
    app.downloads.list.select(Some(2));
    let result = completion(
        &app,
        Ok(OperationOutput::RefreshDownloads(vec![
            download_entry("A.zip", 1, 1, false),
            download_entry("B.zip", 1, 1, false),
        ])),
    );

    app.apply_completion(OperationKind::RefreshDownloads, result);

    assert_eq!(app.downloads.list.index(), Some(1));
}

#[test]
fn failed_refresh_preserves_cached_downloads() {
    let mut app = App::sample();
    app.downloads.entries = vec![download_entry("Cached.zip", 1, 1, false)];
    let result = completion(&app, Err(OperationFailure::new("test worker stopped")));

    app.apply_completion(OperationKind::RefreshDownloads, result);

    assert_eq!(app.downloads.entries[0].name, "Cached.zip");
    assert!(matches!(app.operation, OperationState::Completed(_)));
}
