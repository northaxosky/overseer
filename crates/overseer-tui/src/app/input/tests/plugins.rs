//! Tests for the Plugins workspace's separators: CRUD, collapse, and the sidecar guard

use crate::app::input::test_helpers::key;
use crate::app::{App, Confirm, ConfirmAction, Focus, Modal, Prompt, PromptKind, Workspace};
use overseer_core::plugins::{
    PluginEntry, PluginLoadOrder, PluginRow, PluginSeparators, Separator,
};
use ratatui::crossterm::event::KeyCode;

/// An app on a temp instance with two plugins, focused on the Plugins pane
fn app_with_plugins() -> (tempfile::TempDir, App) {
    let (tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    app.session.order = PluginLoadOrder {
        profile: "Default".to_owned(),
        plugins: vec![
            PluginEntry {
                name: "Alpha.esp".to_owned(),
                active: true,
            },
            PluginEntry {
                name: "Beta.esp".to_owned(),
                active: false,
            },
        ],
    };
    app.session.plugin_separators = PluginSeparators::default();
    app.focus = Focus::Workspace;
    app.workspace = Workspace::Plugins;
    app.session
        .profile
        .save(&app.session.instance)
        .expect("seed the profile dir");
    app.plugins_state.select(Some(0));
    (tmp, app)
}

/// The display index of the first separator row, or a panic
fn separator_display_index(app: &App) -> usize {
    app.plugins_visible_rows()
        .iter()
        .position(|row| matches!(row, PluginRow::Separator(_)))
        .expect("a separator is visible")
}

#[test]
fn a_inserts_a_plugin_separator_above_the_selected_plugin_and_persists() {
    let (_tmp, mut app) = app_with_plugins();
    app.plugins_state.select(Some(1)); // Beta.esp

    app.handle_key(key(KeyCode::Char('A')));
    for c in "Middle".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(app.modal.is_none(), "a successful create closes the prompt");
    assert_eq!(app.session.plugin_separators.items.len(), 1);
    let sep = &app.session.plugin_separators.items[0];
    assert_eq!(sep.name, "Middle");
    assert_eq!(
        sep.anchor.as_deref(),
        Some("Beta.esp"),
        "anchored above the selected plugin"
    );

    let dir = app.session.instance.profile_dir("Default");
    let reloaded = PluginSeparators::load(&dir).expect("reload the sidecar");
    assert_eq!(reloaded.items.len(), 1, "the sidecar was persisted");
    assert_eq!(reloaded.items[0].anchor.as_deref(), Some("Beta.esp"));
}

#[test]
fn a_with_no_plugin_below_anchors_to_the_trailing_group() {
    let (_tmp, mut app) = app_with_plugins();
    // No plugins at all: the new separator can only trail the list
    app.session.order.plugins.clear();
    app.plugins_state.select(Some(0));

    app.handle_key(key(KeyCode::Char('A')));
    for c in "Tail".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.session.plugin_separators.items.len(), 1);
    assert_eq!(
        app.session.plugin_separators.items[0].anchor, None,
        "with no plugin below, the separator trails the list"
    );
}

#[test]
fn renaming_a_plugin_separator_round_trips() {
    let (_tmp, mut app) = app_with_plugins();
    app.session.plugin_separators.items.push(Separator {
        name: "Old".to_owned(),
        anchor: Some("Beta.esp".to_owned()),
    });
    app.plugins_state
        .select(Some(separator_display_index(&app)));

    app.handle_key(key(KeyCode::Char('R')));
    match &app.modal {
        Some(Modal::Prompt(Prompt {
            kind: PromptKind::RenamePluginSeparator { index, name },
            ..
        })) => {
            assert_eq!(*index, 0);
            assert_eq!(name, "Old");
        }
        other => panic!("expected a rename-plugin-separator prompt, got {other:?}"),
    }
    for c in "New".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(app.modal.is_none(), "a successful rename closes the prompt");
    assert_eq!(app.session.plugin_separators.items[0].name, "New");
    let dir = app.session.instance.profile_dir("Default");
    let reloaded = PluginSeparators::load(&dir).expect("reload");
    assert_eq!(reloaded.items[0].name, "New", "persisted to disk");
}

#[test]
fn r_on_a_plugin_row_notes_instead_of_renaming() {
    let (_tmp, mut app) = app_with_plugins();
    app.plugins_state.select(Some(0)); // Alpha.esp, a plugin row

    app.handle_key(key(KeyCode::Char('R')));

    assert!(app.modal.is_none(), "no prompt opens on a plugin row");
    assert!(app.message.is_some(), "the user is told why");
}

#[test]
fn deleting_a_plugin_separator_removes_it_and_persists() {
    let (_tmp, mut app) = app_with_plugins();
    app.session.plugin_separators.items.push(Separator {
        name: "Group".to_owned(),
        anchor: Some("Beta.esp".to_owned()),
    });
    app.session
        .plugin_separators
        .save(&app.session.instance.profile_dir("Default"))
        .expect("seed the sidecar");
    app.plugins_state
        .select(Some(separator_display_index(&app)));

    app.handle_key(key(KeyCode::Char('x')));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Confirm(Confirm {
                action: ConfirmAction::DeletePluginSeparator { index: 0 },
                ..
            }))
        ),
        "x on a plugin separator opens a delete confirm"
    );

    app.handle_key(key(KeyCode::Char('y')));

    assert!(app.modal.is_none(), "accepting closes the confirm");
    assert!(
        app.session.plugin_separators.items.is_empty(),
        "the separator is gone"
    );
    let dir = app.session.instance.profile_dir("Default");
    let reloaded = PluginSeparators::load(&dir).expect("reload");
    assert!(reloaded.items.is_empty(), "persisted to disk");
}

#[test]
fn x_on_a_plugin_row_notes_and_deletes_nothing() {
    let (_tmp, mut app) = app_with_plugins();
    app.plugins_state.select(Some(0)); // Alpha.esp, a plugin row

    app.handle_key(key(KeyCode::Char('x')));

    assert!(app.modal.is_none(), "no confirm opens on a plugin row");
    assert!(
        app.message.is_some(),
        "the user is told plugins aren't deleted"
    );
    assert_eq!(app.session.order.plugins.len(), 2, "nothing was removed");
}

#[test]
fn space_on_a_plugin_separator_collapses_its_group_and_hides_members() {
    let (_tmp, mut app) = app_with_plugins();
    app.session.plugin_separators.items.push(Separator {
        name: "Group".to_owned(),
        anchor: Some("Beta.esp".to_owned()),
    });
    let sep_display = separator_display_index(&app);
    app.plugins_state.select(Some(sep_display));

    // rows before collapse: Alpha, <sep>, Beta
    assert_eq!(app.plugins_visible_rows().len(), 3);

    app.handle_key(key(KeyCode::Char(' ')));

    assert!(app.is_plugin_collapsed(0), "the group is now collapsed");
    let rows = app.plugins_visible_rows();
    assert_eq!(
        rows.len(),
        2,
        "Beta is hidden under the collapsed separator"
    );
    assert!(
        !rows.iter().any(|r| matches!(r, PluginRow::Plugin(1))),
        "the member plugin is not shown"
    );
}

#[test]
fn space_on_a_plugin_still_toggles_its_active_flag() {
    let (_tmp, mut app) = app_with_plugins();
    app.plugins_state.select(Some(0)); // Alpha.esp
    assert!(app.session.order.plugins[0].active);

    assert!(
        app.toggle_selected_plugin_row(),
        "toggling a plugin reports a persistent change"
    );
    assert!(!app.session.order.plugins[0].active, "the plugin flipped");
}

#[test]
fn separators_live_in_the_sidecar_and_never_reach_plugins_txt() {
    let (_tmp, mut app) = app_with_plugins();
    app.session.plugin_separators.items.push(Separator {
        name: "Group".to_owned(),
        anchor: Some("Beta.esp".to_owned()),
    });
    app.session
        .plugin_separators
        .save(&app.session.instance.profile_dir("Default"))
        .expect("save the sidecar");
    app.session
        .order
        .save(&app.session.instance)
        .expect("save the load order");

    // What gets handed onward to the game is the reloaded plugins.txt, never the sidecar
    let reloaded = PluginLoadOrder::load(&app.session.instance, "Default").expect("reload");
    let names: Vec<&str> = reloaded.plugins.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["Alpha.esp", "Beta.esp"],
        "plugins.txt holds only real plugins"
    );
    assert!(
        !names.contains(&"Group"),
        "the separator never entered plugins.txt"
    );

    let seps =
        PluginSeparators::load(&app.session.instance.profile_dir("Default")).expect("reload");
    assert_eq!(seps.items.len(), 1, "the separator lives in the sidecar");
}
