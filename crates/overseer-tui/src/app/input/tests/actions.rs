//! Tests for main-view mutations: toggling, reordering, deploying, and purging

use super::*;

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
    assert!(!app.flip_selected(), "foreign entries can't be flipped");
    assert!(app.session.profile.mods[foreign].enabled, "left unchanged");
    assert!(app.message.is_some(), "user is told why");
}

#[test]
fn flip_toggles_the_selected_mod() {
    let mut app = App::sample();
    let rows = app.mods.project(&app.session.profile.mods);
    let m = rows[app.mods.index().expect("a row is selected")].model_index();
    let before = app.session.profile.mods[m].enabled;
    assert!(app.flip_selected());
    assert_eq!(app.session.profile.mods[m].enabled, !before);
}

#[test]
fn flip_toggles_the_selected_plugin() {
    let mut app = App::sample();
    app.focus = Focus::Workspace;
    assert!(app.session.order.plugins[0].active);
    assert!(app.flip_selected());
    assert!(!app.session.order.plugins[0].active);
}

#[test]
fn flip_in_the_conflicts_workspace_is_read_only() {
    use crate::app::Workspace;
    let mut app = App::sample();
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Conflicts;
    let before = app.session.order.plugins[0].active;
    assert!(!app.flip_selected(), "conflicts mutate nothing");
    assert_eq!(
        app.session.order.plugins[0].active, before,
        "plugin active flags are untouched"
    );
    assert!(app.message.is_some(), "the user is told it is read-only");
}

#[test]
fn flip_in_the_saves_workspace_names_the_uppercase_delete_key() {
    let mut app = App::sample();
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Saves;

    assert!(!app.flip_selected());
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Press X to delete a save")
    );
}

#[test]
fn flipping_a_mod_marks_the_conflicts_scan_stale() {
    use crate::app::ConflictsStatus;
    let mut app = App::sample();
    app.conflicts.status = ConflictsStatus::Ready(Vec::new());
    assert!(app.flip_selected(), "a managed mod flips");
    assert!(
        matches!(app.conflicts.status, ConflictsStatus::Stale),
        "changing the enabled set invalidates the scan"
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
    app.conflicts.status = ConflictsStatus::Ready(Vec::new());
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
    app.conflicts.status = ConflictsStatus::Ready(Vec::new());
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

fn deployable_app() -> (tempfile::TempDir, App, camino::Utf8PathBuf) {
    use overseer_core::instance::Profile;
    use overseer_core::plugins::{PluginLoadOrder, PluginSeparators};
    use overseer_core::test_support::{install_mod, save_profile, temp_instance};

    let (tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    let deployed = instance.config.game_dir.join("Data").join("Textures/a.dds");
    let profile = Profile::load(&instance, "Default").expect("load profile");
    let order = PluginLoadOrder::load(&instance, "Default").expect("load order");

    let mut app = App::sample();
    app.session.instance = instance;
    app.session.profile = profile;
    app.session.order = order;
    app.session.plugin_separators = PluginSeparators::default();
    app.session.discovered = Vec::new();
    app.session.status = None;
    app.mods.reset(&app.session.profile.mods);
    (tmp, app, deployed)
}

#[test]
fn deploy_action_deploys_profile_and_refreshes_status() {
    let (_tmp, mut app, deployed) = deployable_app();

    app.deploy();

    assert_eq!(
        std::fs::read_to_string(&deployed).expect("deployed file"),
        "pixels"
    );
    assert!(
        app.session.status.is_some(),
        "deploy refreshes the cached deployment status"
    );
    assert!(
        app.message
            .as_ref()
            .is_some_and(|m| m.text.starts_with("Deployed ")),
        "deploy reports success"
    );
}

#[test]
fn purge_action_removes_deployment_and_refreshes_status() {
    let (_tmp, mut app, deployed) = deployable_app();
    app.deploy();
    assert!(deployed.exists());

    app.purge();

    assert!(!deployed.exists(), "purge removes the deployed file");
    assert!(
        app.session.status.is_none(),
        "purge refreshes the cached deployment status"
    );
    assert!(
        app.message
            .as_ref()
            .is_some_and(|m| m.text == "Purged the live deployment"),
        "purge reports success"
    );
}
