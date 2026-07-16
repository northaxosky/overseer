//! Tests for keyboard handling and the actions it drives on App

use super::test_helpers::key;
use super::*;
use crate::app::{ModPaneRow, Select};
use crate::test_support::{download_entry, save_info};
use overseer_core::settings::{
    DownloadsSort, DownloadsSortKey, SavesSort, SavesSortKey, Settings, SortDir,
};
use std::ffi::OsString;
use std::sync::Mutex;
use strum::IntoEnumIterator;

static SETTINGS_ENV_LOCK: Mutex<()> = Mutex::new(());

struct ConfigEnvGuard {
    previous: Option<OsString>,
}

impl Drop for ConfigEnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => unsafe { std::env::set_var("OVERSEER_CONFIG_DIR", value) },
            None => unsafe { std::env::remove_var("OVERSEER_CONFIG_DIR") },
        }
    }
}

fn with_config_dir<R>(f: impl FnOnce(camino::Utf8PathBuf) -> R) -> R {
    let _lock = SETTINGS_ENV_LOCK.lock().expect("settings env lock");
    let dir = tempfile::TempDir::new().expect("temp settings dir");
    let path = camino::Utf8PathBuf::from_path_buf(dir.path().to_owned()).expect("utf8 path");
    let previous = std::env::var_os("OVERSEER_CONFIG_DIR");
    unsafe { std::env::set_var("OVERSEER_CONFIG_DIR", path.as_str()) };
    let _guard = ConfigEnvGuard { previous };
    f(path.join("config.toml"))
}

#[test]
fn tab_toggles_focus() {
    let mut app = App::sample();
    assert_eq!(app.focus, Focus::Mods);
    app.toggle_focus();
    assert_eq!(app.focus, Focus::Workspace);
    app.toggle_focus();
    assert_eq!(app.focus, Focus::Mods);
}

#[test]
fn selection_moves_and_clamps_within_the_focused_pane() {
    let mut app = App::sample();
    assert_eq!(app.mods.index(), Some(0));
    app.move_main_selection(-1); // already at top → clamps
    assert_eq!(app.mods.index(), Some(0));
    app.move_main_selection(1);
    assert_eq!(app.mods.index(), Some(1));
    app.move_main_selection(1); // at bottom (len 2) → clamps
    assert_eq!(app.mods.index(), Some(1));
    // The plugins pane is independent and untouched while Mods is focused
    assert_eq!(app.plugins.index(), Some(0));
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
fn keys_1_and_2_switch_workspace_without_moving_focus() {
    let mut app = App::sample();
    assert_eq!(app.workspace, Workspace::Plugins);
    assert_eq!(app.focus, Focus::Mods);

    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    assert_eq!(app.workspace, Workspace::Conflicts);
    assert_eq!(app.focus, Focus::Mods, "switching never moves focus");

    // Even with the right pane focused, switching back leaves focus put
    app.focus = Focus::Workspace;
    app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    assert_eq!(app.workspace, Workspace::Plugins);
    assert_eq!(app.focus, Focus::Workspace, "switching never moves focus");
}

#[test]
fn brackets_cycle_through_the_workspaces() {
    let mut app = App::sample();
    assert_eq!(app.workspace, Workspace::Plugins);
    app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
    assert_eq!(app.workspace, Workspace::Conflicts, "] goes to the next");
    app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
    assert_eq!(app.workspace, Workspace::Downloads, "] keeps going");
    app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
    assert_eq!(app.workspace, Workspace::Saves, "] reaches the last");
    app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
    assert_eq!(app.workspace, Workspace::Plugins, "] wraps around");
    app.handle_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
    assert_eq!(app.workspace, Workspace::Saves, "[ wraps backward");
}

#[test]
fn jk_route_to_the_active_workspace_list() {
    use overseer_core::deploy::{ConflictSnapshot, DestinationEntry, Provider, ProviderOrigin};
    let conflict = |name: &str| DestinationEntry {
        destination: camino::Utf8PathBuf::from(name),
        providers: ["Low", "High"]
            .into_iter()
            .map(|provider| Provider {
                origin: ProviderOrigin::Mod {
                    name: provider.to_owned(),
                },
                source: camino::Utf8PathBuf::from(format!("mods/{provider}")),
            })
            .collect(),
    };

    let mut app = App::sample();
    app.focus = Focus::Workspace;

    // Plugins workspace (default): j/k move the plugins list
    assert_eq!(app.plugins.index(), Some(0));
    app.move_main_selection(1);
    assert_eq!(app.plugins.index(), Some(1));

    // Conflicts workspace: j/k move the conflicts list, leaving plugins put
    app.workspace = Workspace::Conflicts;
    app.conflicts.status = ConflictsStatus::Ready(ConflictSnapshot::from_entries(vec![
        conflict("a.dds"),
        conflict("b.dds"),
    ]));
    app.conflicts.list.select(Some(0));
    app.move_main_selection(1);
    assert_eq!(app.conflicts.list.index(), Some(1), "conflicts list moves");
    assert_eq!(app.plugins.index(), Some(1), "plugins list untouched");
}

#[test]
fn r_starts_a_conflict_worker_without_scanning_immediately() {
    let mut app = App::sample();
    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::ScanConflicts)
    );
    assert!(matches!(app.conflicts.status, ConflictsStatus::Stale));
    app.finish_operation_after_terminal();
}

#[test]
fn r_outside_the_conflicts_workspace_is_inert() {
    let mut app = App::sample();
    assert_eq!(app.workspace, Workspace::Plugins);
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert!(
        matches!(app.conflicts.status, ConflictsStatus::Stale),
        "r only scans in the Conflicts workspace"
    );
}

// --- Characterization tests: pin today's workspace-dispatch behavior so the upcoming enum-method refactor can't drift. ---

#[test]
fn workspace_iter_is_in_switch_order() {
    let order: Vec<Workspace> = Workspace::iter().collect();
    assert_eq!(
        order,
        vec![
            Workspace::Plugins,
            Workspace::Conflicts,
            Workspace::Downloads,
            Workspace::Saves,
        ],
    );
}

#[test]
fn number_keys_select_each_workspace() {
    let mut app = App::sample();
    for (c, ws) in [
        ('1', Workspace::Plugins),
        ('2', Workspace::Conflicts),
        ('3', Workspace::Downloads),
        ('4', Workspace::Saves),
    ] {
        app.handle_key(key(KeyCode::Char(c)));
        assert_eq!(app.workspace, ws, "{c} selects its workspace");
    }
}

#[test]
fn o_cycles_saves_sort_key_and_persists() {
    with_config_dir(|config| {
        let mut app = App::sample();
        app.workspace = Workspace::Saves;
        app.saves.entries = vec![save_info("B.fos", 20, None), save_info("A.fos", 10, None)];
        app.saves.list.select(Some(1));

        app.handle_key(key(KeyCode::Char('o')));

        assert_eq!(
            app.settings.saves_sort,
            SavesSort {
                key: SavesSortKey::Name,
                dir: SortDir::Asc,
            }
        );
        // Name/Asc reorders A before B, and the cursor resets to the top row
        assert_eq!(app.saves.entries[0].file_name, "A.fos");
        assert_eq!(app.saves.list.index(), Some(0));
        let saved = Settings::load_from(&config).expect("load saved settings");
        assert_eq!(saved.saves_sort, app.settings.saves_sort);
    });
}

#[test]
fn shift_o_toggles_download_sort_direction_and_persists() {
    with_config_dir(|config| {
        let mut app = App::sample();
        app.workspace = Workspace::Downloads;
        app.settings.downloads_sort = DownloadsSort {
            key: DownloadsSortKey::Size,
            dir: SortDir::Desc,
        };

        app.handle_key(key(KeyCode::Char('O')));

        assert_eq!(app.settings.downloads_sort.dir, SortDir::Asc);
        let saved = Settings::load_from(&config).expect("load saved settings");
        assert_eq!(saved.downloads_sort, app.settings.downloads_sort);
    });
}

#[test]
fn o_cycles_downloads_sort_key_and_resets_to_top() {
    with_config_dir(|_config| {
        let mut app = App::sample();
        app.workspace = Workspace::Downloads;
        app.downloads.entries = vec![
            download_entry("B.zip", 1, 10),
            download_entry("A.zip", 1, 20),
        ];
        app.downloads.list.select(Some(1));

        app.handle_key(key(KeyCode::Char('o')));

        assert_eq!(
            app.settings.downloads_sort,
            DownloadsSort {
                key: DownloadsSortKey::Date,
                dir: SortDir::Desc,
            }
        );
        // Date/Desc puts the newer A.zip first; the cursor resets to the top row
        assert_eq!(app.downloads.entries[0].name, "A.zip");
        assert_eq!(app.downloads.list.index(), Some(0));
    });
}

#[test]
fn download_sort_remains_available_and_persists_while_busy() {
    use crate::app::RefreshDownloadsJob;

    with_config_dir(|config| {
        let mut app = App::sample();
        app.workspace = Workspace::Downloads;
        let before = app.settings.downloads_sort;
        app.start_operation(RefreshDownloadsJob);

        app.handle_key(key(KeyCode::Char('o')));

        assert_ne!(app.settings.downloads_sort, before);
        let saved = Settings::load_from(&config).expect("load saved settings");
        assert_eq!(saved.downloads_sort, app.settings.downloads_sort);
        app.finish_operation_after_terminal();
    });
}

#[test]
fn sort_keys_are_inert_outside_saves_and_downloads() {
    let mut app = App::sample();
    app.workspace = Workspace::Plugins;

    app.handle_key(key(KeyCode::Char('o')));

    assert_eq!(app.settings.saves_sort, SavesSort::default());
    assert_eq!(app.settings.downloads_sort, DownloadsSort::default());
    assert!(
        app.message
            .as_ref()
            .is_some_and(|n| n.text.contains("Saves and Downloads")),
        "the no-op is explained"
    );
}

#[test]
fn cycle_wraps_in_both_directions() {
    assert_eq!(Workspace::Plugins.cycle(1), Workspace::Conflicts);
    assert_eq!(
        Workspace::Saves.cycle(1),
        Workspace::Plugins,
        "forward wraps to the front"
    );
    assert_eq!(
        Workspace::Plugins.cycle(-1),
        Workspace::Saves,
        "backward wraps to the back"
    );
    assert_eq!(Workspace::Conflicts.cycle(-1), Workspace::Plugins);
}

#[test]
fn switching_to_conflicts_does_not_scan() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('2')));
    assert_eq!(app.workspace, Workspace::Conflicts);
    assert!(
        matches!(app.conflicts.status, ConflictsStatus::Stale),
        "entering Conflicts must not scan (scanning is r-only)"
    );
}

#[test]
fn conflicts_selection_length_tracks_the_scan_status() {
    use overseer_core::deploy::{ConflictSnapshot, DestinationEntry, Provider, ProviderOrigin};
    let conflict = |name: &str| DestinationEntry {
        destination: camino::Utf8PathBuf::from(name),
        providers: ["A", "B"]
            .into_iter()
            .map(|provider| Provider {
                origin: ProviderOrigin::Mod {
                    name: provider.to_owned(),
                },
                source: camino::Utf8PathBuf::from(format!("mods/{provider}")),
            })
            .collect(),
    };
    let mut app = App::sample();
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Conflicts;

    // Stale ⇒ zero rows: movement can select nothing
    app.conflicts.list.select(None);
    app.move_main_selection(1);
    assert_eq!(app.conflicts.list.index(), None, "a stale scan has no rows");

    // Ready(n) ⇒ n rows: movement walks them
    app.conflicts.status = ConflictsStatus::Ready(ConflictSnapshot::from_entries(vec![
        conflict("a.dds"),
        conflict("b.dds"),
    ]));
    app.conflicts.list.select(Some(0));
    app.move_main_selection(1);
    assert_eq!(
        app.conflicts.list.index(),
        Some(1),
        "a ready scan has n rows"
    );
}

#[test]
fn f_filters_conflicts_to_the_selected_mod_and_esc_clears() {
    use overseer_core::deploy::{ConflictSnapshot, DestinationEntry, Provider, ProviderOrigin};
    let entry = |dest: &str, providers: &[&str]| DestinationEntry {
        destination: camino::Utf8PathBuf::from(dest),
        providers: providers
            .iter()
            .map(|n| Provider {
                origin: ProviderOrigin::Mod {
                    name: (*n).to_owned(),
                },
                source: camino::Utf8PathBuf::from(format!("mods/{n}")),
            })
            .collect(),
    };
    let mut app = app_with_groups();
    app.workspace = Workspace::Conflicts;
    app.focus = Focus::Workspace;
    app.conflicts.status = ConflictsStatus::Ready(ConflictSnapshot::from_entries(vec![
        entry("Data/a.dds", &["PatchB", "PatchA"]),
        entry("Data/b.dds", &["TextureX", "PatchB"]),
    ]));
    app.conflicts.list.select(Some(0));

    // PatchA is display row 4 (model 0); filter conflicts to it
    app.mods.select(Some(4));
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
    assert_eq!(app.conflicts.filter.as_deref(), Some("PatchA"));
    assert_eq!(
        app.conflicts.visible_indices(),
        vec![0],
        "only the conflict involving PatchA shows"
    );

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.conflicts.filter, None, "Esc clears the filter");
    assert_eq!(app.conflicts.visible_indices(), vec![0, 1]);
}

#[test]
fn after_session_changed_resets_selection_and_marks_conflicts_stale() {
    let mut app = App::sample();
    app.mods.select(Some(1));
    app.plugins.select(Some(1));
    *app.conflicts.list.state_mut().offset_mut() = 2;
    *app.downloads.list.state_mut().offset_mut() = 3;
    *app.saves.list.state_mut().offset_mut() = 4;
    app.conflicts.status = ConflictsStatus::Ready(
        overseer_core::deploy::ConflictSnapshot::from_entries(Vec::new()),
    );
    app.workspace = Workspace::Plugins;

    app.after_session_changed();

    assert_eq!(
        app.mods.index(),
        Some(0),
        "mods selection resets to the top"
    );
    assert_eq!(
        app.plugins.index(),
        Some(0),
        "plugins selection resets to the top"
    );
    assert!(
        matches!(app.conflicts.status, ConflictsStatus::Stale),
        "a session change invalidates the conflicts scan"
    );
    assert_eq!(app.conflicts.list.state_mut().offset(), 0);
    assert_eq!(app.downloads.list.state_mut().offset(), 0);
    assert_eq!(app.saves.list.state_mut().offset(), 0);
}

#[test]
fn after_session_changed_refreshes_only_the_active_lazy_pane() {
    use overseer_core::test_support::{self, temp_instance};

    // On Downloads, a session change re-lists the archives
    let (_tmp_a, scaffold_a) = temp_instance();
    let instance_a =
        overseer_core::instance::Instance::init(scaffold_a.root.clone(), scaffold_a.config.clone())
            .expect("init");
    test_support::write(&instance_a.downloads_dir().join("Small.zip"), "x");
    test_support::write(&instance_a.downloads_dir().join("Large.zip"), "larger");
    let mut on_downloads = App::sample();
    on_downloads.session.instance = instance_a;
    on_downloads.workspace = Workspace::Downloads;
    on_downloads.settings.downloads_sort = DownloadsSort {
        key: DownloadsSortKey::Size,
        dir: SortDir::Desc,
    };
    on_downloads.downloads.entries.clear();
    on_downloads.after_session_changed();
    on_downloads.finish_operation_after_terminal();
    let names: Vec<&str> = on_downloads
        .downloads
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["Large.zip", "Small.zip"]);

    // On Plugins, the same change leaves the inactive Downloads pane empty
    let (_tmp_b, instance_b) = temp_instance();
    test_support::write(&instance_b.downloads_dir().join("Mod.zip"), "fake");
    let mut on_plugins = App::sample();
    on_plugins.session.instance = instance_b;
    on_plugins.workspace = Workspace::Plugins;
    on_plugins.downloads.entries.clear();
    on_plugins.after_session_changed();
    assert!(
        on_plugins.downloads.entries.is_empty(),
        "an inactive pane is not eagerly listed"
    );
}

#[test]
fn workspace_keys_are_unique() {
    let mut keys: Vec<char> = Workspace::iter().map(Workspace::key).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(
        keys.len(),
        Workspace::iter().count(),
        "every workspace has a distinct switch key"
    );
}

#[test]
fn from_key_round_trips_every_workspace() {
    for w in Workspace::iter() {
        assert_eq!(
            Workspace::from_key(w.key()),
            Some(w),
            "{w:?} round-trips through its key"
        );
    }
}

fn managed_row(name: &str) -> overseer_core::instance::ModListEntry {
    overseer_core::instance::ModListEntry {
        name: name.to_owned(),
        enabled: true,
        kind: overseer_core::instance::ModKind::Managed,
    }
}

fn separator_row(name: &str) -> overseer_core::instance::ModListEntry {
    overseer_core::instance::ModListEntry {
        name: name.to_owned(),
        enabled: false,
        kind: overseer_core::instance::ModKind::Separator,
    }
}

/// A fixture whose file order is PatchA, PatchB, [Gameplay], TextureX, [Visual]
fn app_with_groups() -> App {
    let mut app = App::sample();
    app.session.profile.mods = vec![
        managed_row("PatchA"),
        managed_row("PatchB"),
        separator_row("Gameplay_separator"),
        managed_row("TextureX"),
        separator_row("Visual_separator"),
    ];
    app.mods.reset(&app.session.profile.mods);
    app
}

/// Collect projected model indices in display order
fn projected_model_indices(app: &App) -> Vec<usize> {
    app.mods
        .project(&app.session.profile.mods)
        .iter()
        .map(|row| row.model_index())
        .collect()
}

/// Resolve the selected display row to its model index
fn selected_model_index(app: &App) -> Option<usize> {
    let rows = app.mods.project(&app.session.profile.mods);
    app.mods
        .index()
        .and_then(|index| rows.get(index))
        .map(|row| row.model_index())
}

#[test]
fn visible_rows_reverses_file_order_into_mo2_order() {
    let app = app_with_groups();
    assert_eq!(projected_model_indices(&app), vec![4, 3, 2, 1, 0]);
}

#[test]
fn selected_mod_translates_display_to_model() {
    let mut app = app_with_groups();
    app.mods.select(Some(0));
    assert_eq!(selected_model_index(&app), Some(4)); // top of the UI = the Visual separator
    app.mods.select(Some(4));
    assert_eq!(selected_model_index(&app), Some(0)); // bottom = PatchA, the highest priority
}

#[test]
fn collapsing_a_separator_hides_its_group_and_keeps_the_cursor() {
    let mut app = app_with_groups();
    app.mods.select(Some(2)); // the Gameplay separator
    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(
        projected_model_indices(&app),
        vec![4, 3, 2],
        "PatchB and PatchA are hidden"
    );
    assert!(matches!(
        app.mods.project(&app.session.profile.mods)[2],
        ModPaneRow::Separator {
            model_index: 2,
            collapsed: true,
            member_count: 2,
            ..
        }
    ));
    assert_eq!(
        selected_model_index(&app),
        Some(2),
        "the cursor stays on the separator"
    );
    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(projected_model_indices(&app), vec![4, 3, 2, 1, 0]);
}

#[test]
fn navigation_skips_a_collapsed_groups_members() {
    let mut app = app_with_groups();
    app.mods.select(Some(2));
    app.handle_key(key(KeyCode::Char(' '))); // collapse Gameplay -> visible [4, 3, 2]
    app.mods.select(Some(0));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down)); // clamps: only three rows are visible
    assert_eq!(app.mods.index(), Some(2));
}

#[test]
fn clamp_mod_selection_pulls_a_stale_index_into_view() {
    let mut app = app_with_groups();
    app.mods.select(Some(2));
    app.handle_key(key(KeyCode::Char(' '))); // visible shrinks to [4, 3, 2]
    app.mods.select(Some(4)); // stale: past the new end
    let len = app.mods.project(&app.session.profile.mods).len();
    app.mods.clamp(len);
    assert_eq!(app.mods.index(), Some(2));
}

#[test]
fn changing_session_clears_collapse_state() {
    let mut app = app_with_groups();
    app.mods.select(Some(2));
    app.handle_key(key(KeyCode::Char(' ')));
    app.after_session_changed();
    assert!(matches!(
        app.mods.project(&app.session.profile.mods)[2],
        ModPaneRow::Separator {
            collapsed: false,
            ..
        }
    ));
}

#[test]
fn an_open_modal_swallows_main_view_keys() {
    use crate::app::{Confirm, ConfirmAction};
    let mut app = App::sample();
    app.modal = Some(Modal::Confirm(Confirm {
        message: "Delete Save1.fos?".to_owned(),
        action: ConfirmAction::DeleteSave(camino::Utf8PathBuf::from("Save1.fos")),
    }));

    // Main-view keys must not leak past an open modal: no quit, no switch, no deploy
    for c in ['q', '2', 'D'] {
        app.handle_key(key(KeyCode::Char(c)));
    }

    assert!(!app.should_quit, "q must not quit while a modal is open");
    assert_eq!(
        app.workspace,
        Workspace::Plugins,
        "2 must not switch workspace"
    );
    assert!(
        matches!(app.modal, Some(Modal::Confirm(_))),
        "the confirm stays open, unaffected by main-view keys"
    );
}

fn jump_entry(
    destination: &str,
    origins: Vec<overseer_core::deploy::ProviderOrigin>,
) -> overseer_core::deploy::DestinationEntry {
    use overseer_core::deploy::{DestinationEntry, Provider};

    DestinationEntry {
        destination: camino::Utf8PathBuf::from(destination),
        providers: origins
            .into_iter()
            .enumerate()
            .map(|(index, origin)| Provider {
                origin,
                source: camino::Utf8PathBuf::from(format!("provider-{index}")),
            })
            .collect(),
    }
}

fn mod_origin(name: &str) -> overseer_core::deploy::ProviderOrigin {
    overseer_core::deploy::ProviderOrigin::Mod {
        name: name.to_owned(),
    }
}

fn foreign_row(name: &str) -> overseer_core::instance::ModListEntry {
    overseer_core::instance::ModListEntry {
        name: name.to_owned(),
        enabled: true,
        kind: overseer_core::instance::ModKind::Foreign,
    }
}

fn jump_app(
    mods: Vec<overseer_core::instance::ModListEntry>,
    conflict: overseer_core::deploy::DestinationEntry,
) -> App {
    let mut app = App::sample();
    app.session.profile.mods = mods;
    app.mods.reset(&app.session.profile.mods);
    app.workspace = Workspace::Conflicts;
    app.focus = Focus::Workspace;
    app.conflicts.status =
        ConflictsStatus::Ready(overseer_core::deploy::ConflictSnapshot::from_entries(vec![
            conflict,
        ]));
    app.conflicts.list.select(Some(0));
    app
}

fn selected_mod_name(app: &App) -> Option<&str> {
    let rows = app.mods.project(&app.session.profile.mods);
    app.mods
        .index()
        .and_then(|index| rows.get(index))
        .map(|row| app.session.profile.mods[row.model_index()].name.as_str())
}

#[test]
fn g_opens_provider_picker_in_winner_first_order() {
    let mut app = jump_app(
        vec![
            managed_row("Low"),
            managed_row("Middle"),
            managed_row("Winner"),
        ],
        jump_entry(
            "Data/shared.dds",
            vec![
                mod_origin("Low"),
                mod_origin("Middle"),
                mod_origin("Winner"),
            ],
        ),
    );

    app.handle_key(key(KeyCode::Char('g')));

    assert!(matches!(
        &app.modal,
        Some(Modal::Select(Select {
            kind: SelectKind::JumpProvider { providers },
            items,
            ..
        })) if providers == &["Winner", "Middle", "Low"]
            && items == &["Winner", "Middle", "Low"]
    ));
}

#[test]
fn submitting_provider_picker_reveals_the_chosen_mod() {
    let mut app = jump_app(
        vec![managed_row("Low"), managed_row("Winner")],
        jump_entry(
            "Data/shared.dds",
            vec![mod_origin("Low"), mod_origin("Winner")],
        ),
    );

    app.handle_key(key(KeyCode::Char('g')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Enter));

    assert!(app.modal.is_none());
    assert_eq!(app.focus, Focus::Mods);
    assert_eq!(selected_mod_name(&app), Some("Low"));
}

#[test]
fn overwrite_and_one_managed_provider_jump_without_a_picker() {
    let mut app = jump_app(
        vec![managed_row("Winner")],
        jump_entry(
            "Data/shared.dds",
            vec![
                overseer_core::deploy::ProviderOrigin::Overwrite,
                mod_origin("Winner"),
            ],
        ),
    );

    app.handle_key(key(KeyCode::Char('g')));

    assert!(app.modal.is_none());
    assert_eq!(app.focus, Focus::Mods);
    assert_eq!(selected_mod_name(&app), Some("Winner"));
}

#[test]
fn overwrite_only_conflict_notes_without_jumping() {
    let mut app = jump_app(
        vec![managed_row("Unrelated")],
        jump_entry(
            "Data/shared.dds",
            vec![
                overseer_core::deploy::ProviderOrigin::Overwrite,
                overseer_core::deploy::ProviderOrigin::Overwrite,
            ],
        ),
    );

    app.handle_key(key(KeyCode::Char('g')));

    assert!(app.modal.is_none());
    assert_eq!(app.focus, Focus::Workspace);
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("No mod provider to jump to")
    );
}

#[test]
fn jumping_expands_only_the_owning_separator_group() {
    let mut app = jump_app(
        vec![
            managed_row("Target"),
            separator_row("Target_separator"),
            managed_row("Other"),
            separator_row("Other_separator"),
        ],
        jump_entry("Data/shared.dds", vec![mod_origin("Target")]),
    );
    app.mods.toggle_separator(0);
    app.mods.toggle_separator(1);

    app.handle_key(key(KeyCode::Char('g')));

    let rows = app.mods.project(&app.session.profile.mods);
    assert!(rows.iter().any(|row| matches!(
        row,
        ModPaneRow::Separator {
            separator_index: 0,
            collapsed: false,
            ..
        }
    )));
    assert!(rows.iter().any(|row| matches!(
        row,
        ModPaneRow::Separator {
            separator_index: 1,
            collapsed: true,
            ..
        }
    )));
    assert_eq!(selected_mod_name(&app), Some("Target"));
}

#[test]
fn jumping_to_ungrouped_mod_preserves_unrelated_collapse_state() {
    let mut app = jump_app(
        vec![
            managed_row("Grouped"),
            separator_row("Group_separator"),
            managed_row("Target"),
        ],
        jump_entry("Data/shared.dds", vec![mod_origin("Target")]),
    );
    app.mods.toggle_separator(0);

    app.handle_key(key(KeyCode::Char('g')));

    assert!(
        app.mods
            .project(&app.session.profile.mods)
            .iter()
            .any(|row| matches!(
                row,
                ModPaneRow::Separator {
                    separator_index: 0,
                    collapsed: true,
                    ..
                }
            ))
    );
    assert_eq!(selected_mod_name(&app), Some("Target"));
}

#[test]
fn provider_name_matching_is_case_insensitive() {
    let mut app = jump_app(
        vec![managed_row("Target")],
        jump_entry("Data/shared.dds", vec![mod_origin("tArGeT")]),
    );

    app.handle_key(key(KeyCode::Char('g')));

    assert_eq!(app.focus, Focus::Mods);
    assert_eq!(selected_mod_name(&app), Some("Target"));
}

#[test]
fn missing_or_non_managed_provider_names_do_not_jump() {
    for mods in [Vec::new(), vec![foreign_row("Target")]] {
        let mut app = jump_app(
            mods,
            jump_entry("Data/shared.dds", vec![mod_origin("Target")]),
        );

        app.handle_key(key(KeyCode::Char('g')));

        assert!(app.modal.is_none());
        assert_eq!(app.focus, Focus::Workspace);
        assert_eq!(
            app.message.as_ref().map(|notice| notice.text.as_str()),
            Some("Target is not in the mod list")
        );
    }
}

#[test]
fn duplicate_managed_provider_names_are_rejected_as_ambiguous() {
    let mut app = jump_app(
        vec![managed_row("Target"), managed_row("target")],
        jump_entry("Data/shared.dds", vec![mod_origin("TARGET")]),
    );

    app.handle_key(key(KeyCode::Char('g')));

    assert_eq!(app.focus, Focus::Workspace);
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("TARGET matches multiple mods")
    );
}

#[test]
fn unavailable_conflict_selections_are_safe_no_ops() {
    let mut stale = App::sample();
    stale.workspace = Workspace::Conflicts;
    stale.focus = Focus::Workspace;
    stale.conflicts.list.select(None);

    let mut empty = App::sample();
    empty.workspace = Workspace::Conflicts;
    empty.focus = Focus::Workspace;
    empty.conflicts.status = ConflictsStatus::Ready(
        overseer_core::deploy::ConflictSnapshot::from_entries(Vec::new()),
    );
    empty.conflicts.list.select(None);

    let mut filtered_empty = jump_app(
        vec![managed_row("Target")],
        jump_entry("Data/shared.dds", vec![mod_origin("Target")]),
    );
    filtered_empty.conflicts.filter = Some("Absent".to_owned());
    filtered_empty.conflicts.list.select(None);

    for app in [&mut stale, &mut empty, &mut filtered_empty] {
        app.handle_key(key(KeyCode::Char('g')));
        assert!(app.modal.is_none());
        assert!(app.message.is_none());
        assert_eq!(app.focus, Focus::Workspace);
    }
}

#[test]
fn jumping_preserves_active_conflict_filter_and_workspace() {
    let mut app = jump_app(
        vec![managed_row("FilterMod"), managed_row("Target")],
        jump_entry(
            "Data/shared.dds",
            vec![mod_origin("FilterMod"), mod_origin("Target")],
        ),
    );
    app.conflicts.filter = Some("FilterMod".to_owned());

    app.handle_key(key(KeyCode::Char('g')));
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.focus, Focus::Mods);
    assert_eq!(selected_mod_name(&app), Some("Target"));
    assert_eq!(app.conflicts.filter.as_deref(), Some("FilterMod"));
    assert_eq!(app.workspace, Workspace::Conflicts);
}

fn plugin_jump_app(
    mods: Vec<overseer_core::instance::ModListEntry>,
    plugins: &[&str],
) -> (tempfile::TempDir, App) {
    let (temp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    app.session.profile.mods = mods;
    app.session.order.plugins = plugins
        .iter()
        .map(|name| overseer_core::plugins::PluginEntry {
            name: (*name).to_owned(),
            active: true,
        })
        .collect();
    app.session.plugin_separators = overseer_core::plugins::PluginSeparators::default();
    app.mods.reset(&app.session.profile.mods);
    app.plugins
        .reset(&app.session.order.plugins, &app.session.plugin_separators);
    app.workspace = Workspace::Plugins;
    app.focus = Focus::Workspace;
    (temp, app)
}

#[test]
fn g_in_plugins_routes_to_the_lazy_managed_provider() {
    let (_temp, mut app) = plugin_jump_app(vec![managed_row("Target")], &["Target.esp"]);
    overseer_core::test_support::install_mod(
        &app.session.instance,
        "Target",
        &[("Target.esp", "plugin")],
    );
    app.conflicts.status =
        ConflictsStatus::Ready(overseer_core::deploy::ConflictSnapshot::from_entries(vec![
            jump_entry(
                "Data/shared.dds",
                vec![overseer_core::deploy::ProviderOrigin::Overwrite],
            ),
        ]));
    app.conflicts.list.select(Some(0));

    app.handle_key(key(KeyCode::Char('g')));

    assert_eq!(app.focus, Focus::Mods);
    assert_eq!(selected_mod_name(&app), Some("Target"));
    assert!(app.message.is_none());
}

#[test]
fn overwrite_plugin_provider_notes_without_jumping() {
    let (_temp, mut app) = plugin_jump_app(vec![managed_row("Managed")], &["Shared.esp"]);
    overseer_core::test_support::install_mod(
        &app.session.instance,
        "Managed",
        &[("Shared.esp", "managed")],
    );
    std::fs::create_dir_all(app.session.instance.overwrite_dir()).expect("create overwrite");
    std::fs::write(
        app.session.instance.overwrite_dir().join("Shared.esp"),
        b"overwrite",
    )
    .expect("write overwrite plugin");

    app.handle_key(key(KeyCode::Char('g')));

    assert_eq!(app.focus, Focus::Workspace);
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Shared.esp is deployed from the Overwrite bucket")
    );
}

#[test]
fn unmanaged_plugin_provider_notes_without_jumping() {
    let (_temp, mut app) = plugin_jump_app(vec![managed_row("Other")], &["Base.esm"]);

    app.handle_key(key(KeyCode::Char('g')));

    assert_eq!(app.focus, Focus::Workspace);
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Base.esm is not from a managed mod")
    );
}

#[test]
fn plugin_provider_jump_is_inert_on_a_separator_row() {
    let (_temp, mut app) = plugin_jump_app(vec![managed_row("Target")], &["Target.esp"]);
    app.session
        .plugin_separators
        .items
        .push(overseer_core::plugins::Separator {
            name: "Group".to_owned(),
            anchor: Some("Target.esp".to_owned()),
        });
    app.plugins
        .reset(&app.session.order.plugins, &app.session.plugin_separators);
    app.plugins.select(Some(0));

    app.handle_key(key(KeyCode::Char('g')));

    assert_eq!(app.focus, Focus::Workspace);
    assert!(app.message.is_none());
}

#[test]
fn plugin_provider_jump_is_safe_without_a_plugin_selection() {
    let (_temp, mut unselected) = plugin_jump_app(vec![managed_row("Target")], &["Target.esp"]);
    unselected.plugins.select(None);

    let (_empty_temp, mut empty) = plugin_jump_app(Vec::new(), &[]);

    for app in [&mut unselected, &mut empty] {
        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.focus, Focus::Workspace);
        assert!(app.message.is_none());
    }
}

#[test]
fn plugin_provider_jump_reveals_a_mod_in_a_collapsed_group() {
    let (_temp, mut app) = plugin_jump_app(
        vec![managed_row("Target"), separator_row("Group")],
        &["Target.esp"],
    );
    overseer_core::test_support::install_mod(
        &app.session.instance,
        "Target",
        &[("Target.esp", "plugin")],
    );
    app.mods.toggle_separator(0);
    assert_eq!(selected_mod_name(&app), Some("Group"));

    app.handle_key(key(KeyCode::Char('g')));

    assert_eq!(app.focus, Focus::Mods);
    assert_eq!(selected_mod_name(&app), Some("Target"));
    assert!(
        app.mods
            .project(&app.session.profile.mods)
            .iter()
            .any(|row| matches!(
                row,
                ModPaneRow::Separator {
                    collapsed: false,
                    ..
                }
            ))
    );
}

#[test]
fn plugin_provider_jump_waits_for_mod_directory_mutation() {
    use crate::app::InstallJob;

    let (_temp, mut app) = plugin_jump_app(vec![managed_row("Target")], &["Target.esp"]);
    overseer_core::test_support::install_mod(
        &app.session.instance,
        "Target",
        &[("Target.esp", "plugin")],
    );
    app.start_operation(InstallJob::new(
        "Missing.zip".to_owned(),
        "Installing".to_owned(),
    ));

    app.handle_key(key(KeyCode::Char('g')));

    assert_eq!(app.focus, Focus::Workspace);
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Install is running; wait to resolve plugin providers")
    );
    app.finish_operation_after_terminal();
}

#[test]
fn g_outside_provider_workspaces_is_a_no_op() {
    let mut app = App::sample();
    app.workspace = Workspace::Downloads;
    let initial_selection = app.mods.index();

    app.handle_key(key(KeyCode::Char('g')));

    assert!(app.modal.is_none());
    assert_eq!(app.focus, Focus::Mods);
    assert_eq!(app.mods.index(), initial_selection);
}
