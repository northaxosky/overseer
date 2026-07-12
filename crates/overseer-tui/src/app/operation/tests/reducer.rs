//! Tests for operation result reduction

use super::*;

use camino::Utf8PathBuf;
use overseer_core::settings::{DownloadsSort, DownloadsSortKey, SavesSort, SavesSortKey, SortDir};

use super::super::protocol::{OperationFailure, OperationKind};
use crate::app::{Focus, Workspace};
use crate::test_support::{download_entry, save_info};

fn completion(app: &App, outcome: Result<OperationOutput, OperationFailure>) -> WorkerCompletion {
    WorkerCompletion {
        context: OperationContext::capture(&app.session),
        outcome,
    }
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
