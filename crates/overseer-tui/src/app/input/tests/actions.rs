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

#[test]
fn shift_moves_the_selected_mod_and_keeps_selection() {
    let mut app = App::sample();
    assert!(app.shift_selected_mod(1));
    assert_eq!(app.session.profile.mods[1].name, "CoolMod");
    assert_eq!(app.mods.index(), Some(1));
    assert!(app.shift_selected_mod(-1));
    assert_eq!(app.session.profile.mods[0].name, "CoolMod");
    assert_eq!(app.mods.index(), Some(0));
}

#[test]
fn shift_is_a_noop_at_edges_and_in_the_plugins_pane() {
    let mut app = App::sample();
    assert!(!app.shift_selected_mod(-1)); // at the top
    assert_eq!(app.mods.index(), Some(0));
    app.focus = Focus::Workspace;
    assert!(!app.shift_selected_mod(1)); // unsupported pane
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

fn names(app: &App) -> Vec<String> {
    app.session
        .profile
        .mods
        .iter()
        .map(|m| m.name.clone())
        .collect()
}

#[test]
fn j_moves_a_mod_down_the_ui_which_raises_its_priority() {
    // model [CoolMod(0), OffMod(1)]; display [OffMod, CoolMod] (highest priority at the bottom)
    let mut app = App::sample();
    app.mods.select(Some(0)); // OffMod: lowest priority, top of the UI
    assert!(app.shift_selected_mod(1)); // J: move down the UI
    assert_eq!(names(&app), vec!["OffMod", "CoolMod"]); // OffMod is now model 0 = highest priority
    assert_eq!(app.mods.index(), Some(1), "selection follows the moved mod");
}

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
    assert!(
        app.shift_selected_mod(1),
        "swaps with the expanded separator, crossing into the group"
    );
    assert_eq!(names(&app), vec!["PatchA", "TextureX", "Group_separator"]);
    assert_eq!(
        app.mods.index(),
        Some(1),
        "selection follows across the boundary"
    );
}

#[test]
fn a_move_past_a_foreign_entry_is_refused() {
    let mut app = App::sample();
    app.session.profile.mods = vec![foreign("DLCArmor"), managed("TextureX")];
    app.mods.reset(&app.session.profile.mods);
    // display: [TextureX(d0,m1), DLCArmor(d1,m0)]
    app.mods.select(Some(0)); // TextureX
    assert!(
        !app.shift_selected_mod(1),
        "can't displace a base-game entry"
    );
    assert!(app.message.is_some());
}

#[test]
fn reordering_a_separator_row_is_refused() {
    let mut app = App::sample();
    app.session.profile.mods = vec![managed("PatchA"), separator("Group_separator")];
    app.mods.reset(&app.session.profile.mods);
    // display: [Group(d0,m1), PatchA(d1,m0)]
    app.mods.select(Some(0)); // the separator
    assert!(!app.shift_selected_mod(1), "separators don't reorder in v1");
    assert!(app.message.is_some());
}

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
        !app.shift_selected_mod(1),
        "the collapsed separator blocks it"
    );
    assert!(app.message.is_some(), "the user is told to expand");
    assert_eq!(
        names(&app),
        vec!["PatchA", "Lower_separator", "TextureX", "Upper_separator"],
        "nothing moved"
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
