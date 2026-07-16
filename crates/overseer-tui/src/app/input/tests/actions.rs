//! Tests for main-view mutations: toggling, reordering, deploying, and purging

use super::*;
use crate::app::input::test_helpers::key;
use crate::app::{Select, SelectKind};
use ratatui::crossterm::event::KeyCode;

/// Foreign mod rows remain unchanged and explain why
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
    let display = app
        .mods
        .project(&app.session.profile.mods)
        .iter()
        .position(|row| row.model_index() == foreign)
        .expect("the foreign row is visible");
    app.mods.select(Some(display));
    app.toggle_selected();
    assert!(app.session.profile.mods[foreign].enabled, "left unchanged");
    assert!(app.message.is_some(), "user is told why");
}

/// Build a disk-backed app whose selected disabled mod provides a plugin
fn persisted_toggle_app() -> (tempfile::TempDir, App) {
    use overseer_core::test_support::{install_plugin, temp_instance};

    let (tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esm");
    install_plugin(&instance, "OffMod", "Off.esp");

    let mut app = App::sample();
    app.session.instance = instance;
    app.session.order.plugins.truncate(1);
    app.session
        .profile
        .save(&app.session.instance)
        .expect("seed profile");
    app.session
        .order
        .save(&app.session.instance)
        .expect("seed load order");
    app.mods.reset(&app.session.profile.mods);
    app.mods.select(Some(0));
    app.plugins
        .reset(&app.session.order.plugins, &app.session.plugin_separators);
    (tmp, app)
}

/// A successful mod toggle reaches disk before live state and plugins are replaced
#[test]
fn successful_mod_toggle_round_trips_and_refreshes_plugins() {
    use crate::app::ConflictsStatus;

    let (_tmp, mut app) = persisted_toggle_app();
    app.conflicts.status = ConflictsStatus::Ready(
        overseer_core::deploy::ConflictSnapshot::from_entries(Vec::new()),
    );
    app.plugins.select(Some(99));

    app.toggle_selected();

    assert!(app.session.profile.mods[1].enabled);
    let loaded = Profile::load(&app.session.instance, "Default").expect("reload toggled profile");
    assert!(loaded.mods[1].enabled, "the toggle reached modlist.txt");
    assert_eq!(
        app.session
            .discovered
            .iter()
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>(),
        ["Cool.esm", "Off.esp"]
    );
    assert_eq!(
        app.session
            .order
            .plugins
            .iter()
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>(),
        ["Cool.esm", "Off.esp"]
    );
    let loaded_order =
        overseer_core::plugins::PluginLoadOrder::load(&app.session.instance, "Default")
            .expect("reload refreshed order");
    assert_eq!(
        loaded_order
            .plugins
            .iter()
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>(),
        ["Cool.esm", "Off.esp"]
    );
    assert_eq!(app.plugins.index(), Some(1), "plugin selection is clamped");
    assert!(matches!(app.conflicts.status, ConflictsStatus::Stale));
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Saved")
    );
}

/// A failed modlist write discards the candidate and preserves every live component
#[test]
fn failed_mod_toggle_leaves_live_state_unchanged() {
    use crate::app::ConflictsStatus;

    let (_tmp, mut app) = persisted_toggle_app();
    app.conflicts.status = ConflictsStatus::Ready(
        overseer_core::deploy::ConflictSnapshot::from_entries(Vec::new()),
    );
    let mods_before = app.session.profile.mods.clone();
    let local_saves_before = app.session.profile.local_saves;
    let order_before = app.session.order.plugins.clone();
    let discovered_before = app.session.discovered.clone();
    let mods_selection_before = app.mods.index();
    let plugins_selection_before = app.plugins.index();
    let modlist = app
        .session
        .instance
        .profile_dir("Default")
        .join("modlist.txt");
    std::fs::remove_file(&modlist).expect("remove mod list");
    std::fs::create_dir(&modlist).expect("block mod list");

    app.toggle_selected();

    assert_eq!(app.session.profile.mods, mods_before);
    assert_eq!(app.session.profile.local_saves, local_saves_before);
    assert_eq!(app.session.order.plugins, order_before);
    assert_eq!(app.session.discovered, discovered_before);
    assert_eq!(app.mods.index(), mods_selection_before);
    assert_eq!(app.plugins.index(), plugins_selection_before);
    assert!(matches!(
        app.conflicts.status,
        ConflictsStatus::Ready(ref conflicts) if conflicts.is_empty()
    ));
    assert!(
        app.message
            .as_ref()
            .is_some_and(|notice| notice.text.starts_with("Could not save mod list: "))
    );
}

/// A plugin refresh failure keeps the durable mod toggle and the prior plugin pair
#[test]
fn failed_plugin_refresh_after_mod_save_reports_partial_success() {
    use crate::app::ConflictsStatus;

    let (_tmp, mut app) = persisted_toggle_app();
    let corrupt = app
        .session
        .instance
        .mods_dir()
        .join("OffMod")
        .join("Off.esp");
    std::fs::write(&corrupt, b"not a plugin").expect("corrupt plugin");
    app.conflicts.status = ConflictsStatus::Ready(
        overseer_core::deploy::ConflictSnapshot::from_entries(Vec::new()),
    );
    let order_before = app.session.order.plugins.clone();
    let discovered_before = app.session.discovered.clone();
    let plugins_selection_before = app.plugins.index();

    app.toggle_selected();

    assert!(app.session.profile.mods[1].enabled);
    let loaded = Profile::load(&app.session.instance, "Default").expect("reload toggled profile");
    assert!(loaded.mods[1].enabled, "the mod toggle remains durable");
    assert_eq!(app.session.order.plugins, order_before);
    assert_eq!(app.session.discovered, discovered_before);
    assert_eq!(app.plugins.index(), plugins_selection_before);
    assert!(matches!(app.conflicts.status, ConflictsStatus::Stale));
    assert!(app.message.as_ref().is_some_and(|notice| {
        notice
            .text
            .starts_with("Saved mod list, but plugin refresh failed: ")
    }));
}

/// Mod separators collapse only the view and never touch persistence
#[test]
fn toggling_a_mod_separator_remains_view_only() {
    let (_tmp, mut app) = persisted_toggle_app();
    app.session.profile.mods.push(separator("Group_separator"));
    app.session
        .profile
        .save_modlist(&app.session.instance)
        .expect("seed separator");
    app.mods.reset(&app.session.profile.mods);
    app.mods.select(Some(0));
    let modlist = app
        .session
        .instance
        .profile_dir("Default")
        .join("modlist.txt");
    let before = std::fs::read(&modlist).expect("read mod list");

    app.toggle_selected();

    assert_eq!(std::fs::read(&modlist).expect("reread mod list"), before);
    assert!(app.message.is_none());
    assert!(matches!(
        app.mods.project(&app.session.profile.mods)[0],
        ModPaneRow::Separator {
            collapsed: true,
            member_count: 2,
            ..
        }
    ));
}

/// Conflicts stay read-only and preserve their existing notice
#[test]
fn toggle_in_the_conflicts_workspace_is_read_only() {
    use crate::app::Workspace;
    let mut app = App::sample();
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Conflicts;
    let before = app.session.order.plugins[0].active;
    app.toggle_selected();
    assert_eq!(
        app.session.order.plugins[0].active, before,
        "plugin active flags are untouched"
    );
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Conflicts are read-only")
    );
}

/// Saves preserve the uppercase delete-key notice
#[test]
fn toggle_in_the_saves_workspace_names_the_uppercase_delete_key() {
    let mut app = App::sample();
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Saves;

    app.toggle_selected();
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Press X to delete a save")
    );
}

/// Staged reorders move the candidate and return its destination
#[test]
fn staged_reorder_moves_the_selected_mod_and_returns_selection() {
    let mut app = App::sample();
    let (profile, selection) = app.reordered_profile(1).expect("stage move down");
    assert_eq!(profile.mods[1].name, "CoolMod");
    assert_eq!(selection, 1);
    assert_eq!(
        app.session.profile.mods[0].name, "CoolMod",
        "live state stays unchanged"
    );
    assert_eq!(app.mods.index(), Some(0), "live cursor stays unchanged");
}

/// Reorder staging is inert at display edges and outside the Mods pane
#[test]
fn staged_reorder_is_a_noop_at_edges_and_in_the_plugins_pane() {
    let mut app = App::sample();
    assert!(app.reordered_profile(-1).is_none()); // at the top
    assert_eq!(app.mods.index(), Some(0));
    app.focus = Focus::Workspace;
    assert!(app.reordered_profile(1).is_none()); // unsupported pane
}

fn managed(name: &str) -> overseer_core::instance::ModListEntry {
    overseer_core::instance::ModListEntry {
        name: name.to_owned(),
        enabled: true,
        kind: ModKind::Managed,
    }
}

fn separator(name: &str) -> overseer_core::instance::ModListEntry {
    overseer_core::instance::ModListEntry {
        name: name.to_owned(),
        enabled: false,
        kind: ModKind::Separator,
    }
}

fn foreign(name: &str) -> overseer_core::instance::ModListEntry {
    overseer_core::instance::ModListEntry {
        name: name.to_owned(),
        enabled: true,
        kind: ModKind::Foreign,
    }
}

/// Collect profile entry names in model order
fn names(profile: &Profile) -> Vec<String> {
    profile.mods.iter().map(|m| m.name.clone()).collect()
}

/// J stages a downward display move as a priority raise
#[test]
fn j_stages_a_mod_down_the_ui_which_raises_its_priority() {
    // model [CoolMod(0), OffMod(1)]; display [OffMod, CoolMod] (highest priority at the bottom)
    let mut app = App::sample();
    app.mods.select(Some(0)); // OffMod: lowest priority, top of the UI
    let (profile, selection) = app.reordered_profile(1).expect("stage J");
    assert_eq!(names(&profile), vec!["OffMod", "CoolMod"]);
    assert_eq!(selection, 1, "the candidate cursor follows the moved mod");
}

/// Managed mods can cross expanded separators in a staged reorder
#[test]
fn a_managed_mod_crosses_an_expanded_separator() {
    let mut app = App::sample();
    app.session.profile.mods = vec![
        managed("PatchA"),
        separator("Group_separator"),
        managed("TextureX"),
    ];
    app.mods.reset(&app.session.profile.mods);
    // display: [TextureX(d0,m2), Group(d1,m1), PatchA(d2,m0)]
    app.mods.select(Some(0)); // TextureX, above the group
    let (profile, selection) = app.reordered_profile(1).expect("cross separator");
    assert_eq!(
        names(&profile),
        vec!["PatchA", "TextureX", "Group_separator"]
    );
    assert_eq!(selection, 1, "the candidate cursor crosses the boundary");
}

/// Reordering past a foreign entry keeps the existing rejection notice
#[test]
fn a_move_past_a_foreign_entry_is_refused() {
    let mut app = App::sample();
    app.session.profile.mods = vec![foreign("DLCArmor"), managed("TextureX")];
    app.mods.reset(&app.session.profile.mods);
    // display: [TextureX(d0,m1), DLCArmor(d1,m0)]
    app.mods.select(Some(0)); // TextureX
    assert!(
        app.reordered_profile(1).is_none(),
        "can't displace a base-game entry"
    );
    assert!(app.message.is_some());
}

/// Reordering a separator keeps the existing rejection notice
#[test]
fn reordering_a_separator_row_is_refused() {
    let mut app = App::sample();
    app.session.profile.mods = vec![managed("PatchA"), separator("Group_separator")];
    app.mods.reset(&app.session.profile.mods);
    // display: [Group(d0,m1), PatchA(d1,m0)]
    app.mods.select(Some(0)); // the separator
    assert!(
        app.reordered_profile(1).is_none(),
        "separators don't reorder in v1"
    );
    assert!(app.message.is_some());
}

/// Collapsed separators keep blocking staged reorders
#[test]
fn a_move_past_a_collapsed_separator_is_refused() {
    let mut app = App::sample();
    app.session.profile.mods = vec![
        managed("PatchA"),
        separator("Lower_separator"),
        managed("TextureX"),
        separator("Upper_separator"),
    ];
    app.mods.reset(&app.session.profile.mods);
    app.mods.toggle_separator(0);
    // display: [Upper(d0), TextureX(d1), Lower▶(d2)]
    app.mods.select(Some(1)); // TextureX
    assert!(
        app.reordered_profile(1).is_none(),
        "the collapsed separator blocks it"
    );
    assert!(app.message.is_some(), "the user is told to expand");
    assert_eq!(
        names(&app.session.profile),
        vec!["PatchA", "Lower_separator", "TextureX", "Upper_separator"],
        "nothing moved"
    );
}

/// Build a disk-backed app for reorder persistence tests
fn persisted_reorder_app() -> (tempfile::TempDir, App) {
    let (tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    app.session
        .profile
        .save(&app.session.instance)
        .expect("seed profile");
    app.mods.reset(&app.session.profile.mods);
    app.mods.select(Some(0));
    (tmp, app)
}

/// A successful reorder swaps live state only after its mod list reaches disk
#[test]
fn successful_reorder_round_trips_and_marks_conflicts_stale() {
    use crate::app::ConflictsStatus;

    let (_tmp, mut app) = persisted_reorder_app();
    app.conflicts.status = ConflictsStatus::Ready(
        overseer_core::deploy::ConflictSnapshot::from_entries(Vec::new()),
    );
    let order_before = app.session.order.plugins.clone();
    let discovered_before = app.session.discovered.clone();

    app.reorder_selected(1);

    assert_eq!(names(&app.session.profile), ["OffMod", "CoolMod"]);
    assert_eq!(app.mods.index(), Some(1));
    let loaded = Profile::load(&app.session.instance, "Default").expect("reload reordered profile");
    assert_eq!(names(&loaded), ["OffMod", "CoolMod"]);
    assert!(matches!(app.conflicts.status, ConflictsStatus::Stale));
    assert_eq!(app.session.order.plugins, order_before);
    assert_eq!(app.session.discovered, discovered_before);
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Saved")
    );
}

/// A failed reorder leaves every live state component unchanged
#[test]
fn failed_reorder_leaves_profile_cursor_conflicts_and_plugins_unchanged() {
    use crate::app::ConflictsStatus;

    let (_tmp, mut app) = persisted_reorder_app();
    app.conflicts.status = ConflictsStatus::Ready(
        overseer_core::deploy::ConflictSnapshot::from_entries(Vec::new()),
    );
    let mods_before = app.session.profile.mods.clone();
    let local_saves_before = app.session.profile.local_saves;
    let cursor_before = app.mods.index();
    let order_before = app.session.order.plugins.clone();
    let discovered_before = app.session.discovered.clone();
    let modlist = app
        .session
        .instance
        .profile_dir("Default")
        .join("modlist.txt");
    std::fs::remove_file(&modlist).expect("remove mod list");
    std::fs::create_dir(&modlist).expect("block mod list");

    app.reorder_selected(1);

    assert_eq!(app.session.profile.mods, mods_before);
    assert_eq!(app.session.profile.local_saves, local_saves_before);
    assert_eq!(app.mods.index(), cursor_before);
    assert!(matches!(
        app.conflicts.status,
        ConflictsStatus::Ready(ref conflicts) if conflicts.is_empty()
    ));
    assert_eq!(app.session.order.plugins, order_before);
    assert_eq!(app.session.discovered, discovered_before);
    assert!(
        app.message
            .as_ref()
            .is_some_and(|notice| notice.text.starts_with("Could not save mod list: "))
    );
}

#[test]
fn uppercase_d_opens_deploy_confirmation_without_running_work() {
    let mut app = App::sample();

    app.handle_key(key(KeyCode::Char('D')));

    assert!(matches!(
        app.modal,
        Some(Modal::Confirm(Confirm {
            action: ConfirmAction::Deploy,
            ..
        }))
    ));
    assert!(!app.operation_running(), "input only stages confirmation");
}

#[test]
fn uppercase_p_opens_purge_confirmation_without_running_work() {
    let mut app = App::sample();

    app.handle_key(key(KeyCode::Char('P')));

    assert!(matches!(
        app.modal,
        Some(Modal::Confirm(Confirm {
            action: ConfirmAction::Purge,
            ..
        }))
    ));
    assert!(!app.operation_running(), "input only stages confirmation");
}

#[test]
fn remove_and_replace_only_accept_managed_mod_rows() {
    for (entry, expected) in [
        (
            separator("Group_separator"),
            "Only managed mods can be removed",
        ),
        (foreign("BaseGame"), "Only managed mods can be removed"),
    ] {
        let mut app = App::sample();
        app.session.profile.mods = vec![entry];
        app.mods.reset(&app.session.profile.mods);

        app.begin_remove_mod();

        assert!(
            app.modal.is_none(),
            "non-managed rows do not open a confirm"
        );
        assert_eq!(
            app.message.as_ref().map(|notice| notice.text.as_str()),
            Some(expected)
        );
        app.begin_replace_mod();
        assert!(app.modal.is_none(), "non-managed rows do not open a picker");
        assert_eq!(
            app.message.as_ref().map(|notice| notice.text.as_str()),
            Some("Only managed mods can be replaced")
        );
    }
}

#[test]
fn managed_mod_actions_open_their_respective_modal() {
    let (_temp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;

    app.begin_remove_mod();
    assert!(matches!(
        app.modal,
        Some(Modal::Confirm(Confirm {
            action: ConfirmAction::RemoveMod(ref name),
            ..
        })) if name == "OffMod"
    ));

    app.modal = None;
    overseer_core::test_support::write_zip(
        &app.session.instance.downloads_dir().join("New.zip"),
        &[("Textures/a.dds", b"replacement")],
    );
    app.begin_replace_mod();
    assert!(matches!(
        app.modal,
        Some(Modal::Select(Select {
            kind: SelectKind::ReplaceArchive { ref target },
            ref items,
            ..
        })) if target == "OffMod" && items == &["New.zip"]
    ));
}

fn app_with_live_deployment_status() -> (tempfile::TempDir, App) {
    use overseer_core::deploy::NullSink;
    use overseer_core::instance::Instance;
    use overseer_core::test_support::{install_mod, save_profile, temp_instance};

    let (temp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone())
        .expect("initialize instance");
    instance.create_profile("Default").expect("create profile");
    install_mod(&instance, "Seed", &[("Textures/seed.dds", "texture")]);
    save_profile(&instance, "Default", &[("Seed", true)]);
    overseer_core::apply::deploy_profile(&instance, "Default", &NullSink).expect("deploy fixture");
    let status = overseer_core::apply::status(&instance)
        .expect("read deployment status")
        .expect("live deployment");

    let mut app = App::sample();
    app.session.instance = instance;
    app.session.status = Some(status);
    (temp, app)
}

#[test]
fn remove_confirmation_advises_only_when_a_deployment_is_live() {
    let (_temp, mut live) = app_with_live_deployment_status();

    live.begin_remove_mod();

    assert!(matches!(
        live.modal,
        Some(Modal::Confirm(Confirm {
            ref message,
            action: ConfirmAction::RemoveMod(_),
        })) if message.contains("a deployment looks live — this will fail unless you purge first")
    ));

    let mut idle = App::sample();
    idle.begin_remove_mod();
    assert!(matches!(
        idle.modal,
        Some(Modal::Confirm(Confirm {
            ref message,
            action: ConfirmAction::RemoveMod(_),
        })) if !message.contains("a deployment looks live")
    ));
}

#[test]
fn replace_confirmation_advises_only_when_a_deployment_is_live() {
    let (_temp, mut live) = app_with_live_deployment_status();
    overseer_core::test_support::write_zip(
        &live.session.instance.downloads_dir().join("New.zip"),
        &[("Textures/a.dds", b"replacement")],
    );

    live.begin_replace_mod();
    live.handle_key(key(KeyCode::Enter));

    assert!(matches!(
        live.modal,
        Some(Modal::Confirm(Confirm {
            ref message,
            action: ConfirmAction::ReplaceMod { .. },
        })) if message.contains("a deployment looks live — this will fail unless you purge first")
    ));

    let (_temp, instance) = overseer_core::test_support::temp_instance();
    let mut idle = App::sample();
    idle.session.instance = instance;
    overseer_core::test_support::write_zip(
        &idle.session.instance.downloads_dir().join("New.zip"),
        &[("Textures/a.dds", b"replacement")],
    );
    idle.begin_replace_mod();
    idle.handle_key(key(KeyCode::Enter));
    assert!(matches!(
        idle.modal,
        Some(Modal::Confirm(Confirm {
            ref message,
            action: ConfirmAction::ReplaceMod { .. },
        })) if !message.contains("a deployment looks live")
    ));
}
