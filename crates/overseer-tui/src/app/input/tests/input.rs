//! Tests for keyboard handling and the actions it drives on App

use super::test_helpers::key;
use super::*;
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
    assert_eq!(app.mods_state.selected(), Some(0));
    app.move_main_selection(-1); // already at top → clamps
    assert_eq!(app.mods_state.selected(), Some(0));
    app.move_main_selection(1);
    assert_eq!(app.mods_state.selected(), Some(1));
    app.move_main_selection(1); // at bottom (len 2) → clamps
    assert_eq!(app.mods_state.selected(), Some(1));
    // The plugins pane is independent and untouched while Mods is focused
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
    use overseer_core::deploy::FileConflict;
    let conflict = |name: &str| FileConflict {
        relative: camino::Utf8PathBuf::from(name),
        providers: vec!["Low".to_owned(), "High".to_owned()],
    };

    let mut app = App::sample();
    app.focus = Focus::Workspace;

    // Plugins workspace (default): j/k move the plugins list
    assert_eq!(app.plugins_state.selected(), Some(0));
    app.move_main_selection(1);
    assert_eq!(app.plugins_state.selected(), Some(1));

    // Conflicts workspace: j/k move the conflicts list, leaving plugins put
    app.workspace = Workspace::Conflicts;
    app.conflicts.status = ConflictsStatus::Ready(vec![conflict("a.dds"), conflict("b.dds")]);
    app.conflicts.list.select(Some(0));
    app.move_main_selection(1);
    assert_eq!(
        app.conflicts.list.selected(),
        Some(1),
        "conflicts list moves"
    );
    assert_eq!(
        app.plugins_state.selected(),
        Some(1),
        "plugins list untouched"
    );
}

#[test]
fn scanning_a_temp_instance_reports_a_shared_file() {
    use overseer_core::instance::{ModKind, ModListEntry};
    use overseer_core::test_support::{install_mod, temp_instance};

    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "A", &[("Textures/shared.dds", "from-a")]);
    install_mod(&instance, "B", &[("Textures/shared.dds", "from-b")]);

    let mut app = App::sample();
    app.session.instance = instance;
    app.session.profile.mods = vec![
        ModListEntry {
            name: "A".to_owned(),
            enabled: true,
            kind: ModKind::Managed,
        },
        ModListEntry {
            name: "B".to_owned(),
            enabled: true,
            kind: ModKind::Managed,
        },
    ];

    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

    match &app.conflicts.status {
        ConflictsStatus::Ready(found) => {
            assert_eq!(found.len(), 1, "the shared file is the only conflict");
            // deploy_sources feeds detect_conflicts lowest priority first, so the higher-priority mod (top of the list) lands last as the winner
            assert_eq!(found[0].providers, ["B", "A"]);
        }
        other => panic!("expected a completed scan, got {other:?}"),
    }
    assert_eq!(
        app.conflicts.list.selected(),
        Some(0),
        "selection lands first"
    );
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
        assert_eq!(app.saves.list.selected(), Some(0));
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
            download_entry("B.zip", 1, 10, false),
            download_entry("A.zip", 1, 20, false),
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
        assert_eq!(app.downloads.list.selected(), Some(0));
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
    use overseer_core::deploy::FileConflict;
    let conflict = |name: &str| FileConflict {
        relative: camino::Utf8PathBuf::from(name),
        providers: vec!["A".to_owned(), "B".to_owned()],
    };
    let mut app = App::sample();
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Conflicts;

    // Stale ⇒ zero rows: movement can select nothing
    app.conflicts.list.select(None);
    app.move_main_selection(1);
    assert_eq!(
        app.conflicts.list.selected(),
        None,
        "a stale scan has no rows"
    );

    // Ready(n) ⇒ n rows: movement walks them
    app.conflicts.status = ConflictsStatus::Ready(vec![conflict("a.dds"), conflict("b.dds")]);
    app.conflicts.list.select(Some(0));
    app.move_main_selection(1);
    assert_eq!(
        app.conflicts.list.selected(),
        Some(1),
        "a ready scan has n rows"
    );
}

#[test]
fn after_session_changed_resets_selection_and_marks_conflicts_stale() {
    let mut app = App::sample();
    app.mods_state.select(Some(1));
    app.plugins_state.select(Some(1));
    app.conflicts.status = ConflictsStatus::Ready(Vec::new());
    app.workspace = Workspace::Plugins;

    app.after_session_changed();

    assert_eq!(
        app.mods_state.selected(),
        Some(0),
        "mods selection resets to the top"
    );
    assert_eq!(
        app.plugins_state.selected(),
        Some(0),
        "plugins selection resets to the top"
    );
    assert!(
        matches!(app.conflicts.status, ConflictsStatus::Stale),
        "a session change invalidates the conflicts scan"
    );
}

#[test]
fn after_session_changed_refreshes_only_the_active_lazy_pane() {
    use overseer_core::test_support::{self, temp_instance};

    // On Downloads, a session change re-lists the archives
    let (_tmp_a, instance_a) = temp_instance();
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
    app.mods_state.select(Some(0));
    app
}

#[test]
fn visible_rows_reverses_file_order_into_mo2_order() {
    let app = app_with_groups();
    assert_eq!(app.visible_rows(), vec![4, 3, 2, 1, 0]);
}

#[test]
fn selected_mod_translates_display_to_model() {
    let mut app = app_with_groups();
    app.mods_state.select(Some(0));
    assert_eq!(app.selected_mod(), Some(4)); // top of the UI = the Visual separator
    app.mods_state.select(Some(4));
    assert_eq!(app.selected_mod(), Some(0)); // bottom = PatchA, the highest priority
}

#[test]
fn collapsing_a_separator_hides_its_group_and_keeps_the_cursor() {
    let mut app = app_with_groups();
    app.mods_state.select(Some(2)); // the Gameplay separator
    app.handle_key(key(KeyCode::Char(' ')));
    assert!(app.is_collapsed(2));
    assert_eq!(
        app.visible_rows(),
        vec![4, 3, 2],
        "PatchB and PatchA are hidden"
    );
    assert_eq!(app.group_members(2), 2, "the group has two members");
    assert_eq!(
        app.selected_mod(),
        Some(2),
        "the cursor stays on the separator"
    );
    app.handle_key(key(KeyCode::Char(' ')));
    assert!(!app.is_collapsed(2), "space toggles back to expanded");
    assert_eq!(app.visible_rows(), vec![4, 3, 2, 1, 0]);
}

#[test]
fn navigation_skips_a_collapsed_groups_members() {
    let mut app = app_with_groups();
    app.mods_state.select(Some(2));
    app.handle_key(key(KeyCode::Char(' '))); // collapse Gameplay -> visible [4, 3, 2]
    app.mods_state.select(Some(0));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down)); // clamps: only three rows are visible
    assert_eq!(app.mods_state.selected(), Some(2));
}

#[test]
fn clamp_mod_selection_pulls_a_stale_index_into_view() {
    let mut app = app_with_groups();
    app.collapsed.insert("gameplay".to_owned()); // visible shrinks to [4, 3, 2]
    app.mods_state.select(Some(4)); // stale: past the new end
    app.clamp_mod_selection();
    assert_eq!(app.mods_state.selected(), Some(2));
}

#[test]
fn changing_session_clears_collapse_state() {
    let mut app = app_with_groups();
    app.collapsed.insert("gameplay".to_owned());
    app.after_session_changed();
    assert!(
        app.collapsed.is_empty(),
        "a session swap resets ephemeral collapse"
    );
}
