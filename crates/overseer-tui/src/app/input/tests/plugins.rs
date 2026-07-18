//! Tests for the Plugins workspace's toggles, separators, collapse, and sidecar guard

use super::*;
use crate::app::input::test_helpers::key;
use overseer_core::plugins::{PluginEntry, PluginLoadOrder, PluginSeparators, Separator};
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
    app.plugins
        .reset(&app.session.order.plugins, &app.session.plugin_separators);
    app.plugins.select(Some(0));
    (tmp, app)
}

/// Find a separator's display index from its sidecar index
fn separator_display_index(app: &App, separator_index: usize) -> usize {
    app.plugins
        .project(&app.session.order.plugins, &app.session.plugin_separators)
        .iter()
        .position(|row| {
            matches!(
                row,
                PluginPaneRow::Separator {
                    separator_index: index,
                    ..
                } if *index == separator_index
            )
        })
        .expect("a separator is visible")
}

/// Find a plugin's display index from its load-order index
fn plugin_display_index(app: &App, plugin_index: usize) -> usize {
    app.plugins
        .project(&app.session.order.plugins, &app.session.plugin_separators)
        .iter()
        .position(|row| {
            matches!(
                row,
                PluginPaneRow::Plugin {
                    plugin_index: index
                } if *index == plugin_index
            )
        })
        .expect("a plugin is visible")
}

/// Reset Plugins pane state after direct fixture mutation
fn sync_plugins(app: &mut App) {
    app.plugins
        .reset(&app.session.order.plugins, &app.session.plugin_separators);
}

/// Add a sidecar separator fixture and reset pane state
fn add_separator(app: &mut App, name: &str, anchor: Option<&str>) {
    app.session.plugin_separators.items.push(Separator {
        name: name.to_owned(),
        anchor: anchor.map(str::to_owned),
    });
    sync_plugins(app);
}

/// Select a separator by sidecar index
fn select_separator(app: &mut App, separator_index: usize) {
    app.plugins
        .select(Some(separator_display_index(app, separator_index)));
}

/// Submit the new plugin separator prompt with `name`
fn submit_new_separator(app: &mut App, name: &str) {
    app.handle_key(key(KeyCode::Char('A')));
    for c in name.chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
}

/// Block profile writes to exercise persistence rollback
fn block_profile_writes(app: &App) {
    let profiles = app.session.instance.profiles_dir();
    std::fs::remove_dir_all(&profiles).expect("remove profiles");
    std::fs::write(&profiles, b"not a directory").expect("block profiles");
}

/// Plugin insertion anchors above the selection and preserves sidecar bytes
#[test]
fn a_inserts_a_plugin_separator_above_the_selected_plugin_and_persists() {
    let (_tmp, mut app) = app_with_plugins();
    app.plugins.select(Some(1)); // Beta.esp

    submit_new_separator(&mut app, "Middle");

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
    assert_eq!(
        std::fs::read_to_string(dir.join("separators.txt")).expect("read sidecar"),
        "Beta.esp\tMiddle\n"
    );
}

#[test]
fn a_with_no_plugin_below_anchors_to_the_trailing_group() {
    let (_tmp, mut app) = app_with_plugins();
    // No plugins at all: the new separator can only trail the list
    app.session.order.plugins.clear();
    app.plugins.select(None);

    submit_new_separator(&mut app, "Tail");

    assert_eq!(app.session.plugin_separators.items.len(), 1);
    assert_eq!(
        app.session.plugin_separators.items[0].anchor, None,
        "with no plugin below, the separator trails the list"
    );
}

/// Plugin insertion follows existing separators with the same anchor
#[test]
fn inserting_above_a_plugin_appends_after_same_anchor_separators() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "First", Some("Beta.esp"));
    add_separator(&mut app, "Second", Some("Beta.esp"));
    app.plugins.select(Some(plugin_display_index(&app, 1)));

    submit_new_separator(&mut app, "Third");

    assert_eq!(
        app.session
            .plugin_separators
            .items
            .iter()
            .map(|separator| separator.name.as_str())
            .collect::<Vec<_>>(),
        ["First", "Second", "Third"]
    );
    assert!(
        app.session
            .plugin_separators
            .items
            .iter()
            .all(|separator| separator.anchor.as_deref() == Some("Beta.esp"))
    );
    assert_eq!(app.plugins.index(), Some(separator_display_index(&app, 2)));
}

/// Expanded separator insertion uses the selected sidecar position
#[test]
fn inserting_above_an_expanded_separator_uses_its_sidecar_position() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "First", Some("Beta.esp"));
    add_separator(&mut app, "Target", Some("Beta.esp"));
    select_separator(&mut app, 1);

    submit_new_separator(&mut app, "New");

    assert_eq!(
        app.session
            .plugin_separators
            .items
            .iter()
            .map(|separator| separator.name.as_str())
            .collect::<Vec<_>>(),
        ["First", "New", "Target"]
    );
    assert_eq!(
        app.session.plugin_separators.items[1].anchor.as_deref(),
        Some("Beta.esp")
    );
}

/// Collapsed separator insertion preserves the selected collapse entry
#[test]
fn inserting_above_a_collapsed_separator_preserves_its_collapse() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "First", Some("Beta.esp"));
    add_separator(&mut app, "Target", Some("Beta.esp"));
    select_separator(&mut app, 1);
    app.handle_key(key(KeyCode::Char(' ')));

    submit_new_separator(&mut app, "New");

    let rows = app
        .plugins
        .project(&app.session.order.plugins, &app.session.plugin_separators);
    assert!(matches!(
        rows[separator_display_index(&app, 1)],
        PluginPaneRow::Separator {
            collapsed: false,
            ..
        }
    ));
    assert!(matches!(
        rows[separator_display_index(&app, 2)],
        PluginPaneRow::Separator {
            collapsed: true,
            member_count: 1,
            ..
        }
    ));
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, PluginPaneRow::Plugin { plugin_index: 1 }))
    );
}

/// Stale trailing insertion preserves trailing sidecar order
#[test]
fn inserting_above_a_stale_trailing_separator_keeps_trailing_order() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Trailing", None);
    add_separator(&mut app, "Stale", Some("Missing.esp"));
    select_separator(&mut app, 1);

    submit_new_separator(&mut app, "New");

    assert_eq!(
        app.session
            .plugin_separators
            .items
            .iter()
            .map(|separator| separator.name.as_str())
            .collect::<Vec<_>>(),
        ["Trailing", "New", "Stale"]
    );
    assert_eq!(
        app.session.plugin_separators.items[1].anchor.as_deref(),
        Some("Missing.esp")
    );
}

/// Missing selection falls back to the first projected row
#[test]
fn inserting_without_selection_uses_the_first_projected_row() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Existing", Some("Alpha.esp"));
    app.plugins.select(None);

    submit_new_separator(&mut app, "New");

    assert_eq!(
        app.session
            .plugin_separators
            .items
            .iter()
            .map(|separator| separator.name.as_str())
            .collect::<Vec<_>>(),
        ["New", "Existing"]
    );
    assert_eq!(
        app.session.plugin_separators.items[0].anchor.as_deref(),
        Some("Alpha.esp")
    );
}

/// Insertion resolves sidecar index when render order differs
#[test]
fn insertion_uses_sidecar_index_when_render_order_differs() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Trailing", None);
    add_separator(&mut app, "Above Alpha", Some("Alpha.esp"));
    assert_eq!(separator_display_index(&app, 1), 0);
    select_separator(&mut app, 1);

    submit_new_separator(&mut app, "New");

    assert_eq!(
        app.session
            .plugin_separators
            .items
            .iter()
            .map(|separator| separator.name.as_str())
            .collect::<Vec<_>>(),
        ["Trailing", "New", "Above Alpha"]
    );
    assert_eq!(
        app.session.plugin_separators.items[1].anchor.as_deref(),
        Some("Alpha.esp")
    );
    assert_eq!(app.plugins.index(), Some(separator_display_index(&app, 1)));
}

#[test]
fn renaming_a_plugin_separator_round_trips() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Old", Some("Beta.esp"));
    select_separator(&mut app, 0);
    app.handle_key(key(KeyCode::Char(' ')));

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
    assert!(matches!(
        app.plugins
            .project(&app.session.order.plugins, &app.session.plugin_separators)[1],
        PluginPaneRow::Separator {
            collapsed: true,
            ..
        }
    ));
}

#[test]
fn r_on_a_plugin_row_notes_instead_of_renaming() {
    let (_tmp, mut app) = app_with_plugins();
    app.plugins.select(Some(0)); // Alpha.esp, a plugin row

    app.handle_key(key(KeyCode::Char('R')));

    assert!(app.modal.is_none(), "no prompt opens on a plugin row");
    assert!(app.message.is_some(), "the user is told why");
}

#[test]
fn deleting_a_plugin_separator_removes_it_and_persists() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Group", Some("Beta.esp"));
    app.session
        .plugin_separators
        .save(&app.session.instance.profile_dir("Default"))
        .expect("seed the sidecar");
    select_separator(&mut app, 0);

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

/// Deletion keeps later collapse entries aligned
#[test]
fn deleting_a_separator_keeps_later_collapse_state_aligned() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "First", Some("Alpha.esp"));
    add_separator(&mut app, "Second", Some("Beta.esp"));
    select_separator(&mut app, 1);
    app.handle_key(key(KeyCode::Char(' ')));
    select_separator(&mut app, 0);

    app.handle_key(key(KeyCode::Char('x')));
    app.handle_key(key(KeyCode::Char('y')));

    assert_eq!(app.session.plugin_separators.items[0].name, "Second");
    assert!(matches!(
        app.plugins
            .project(&app.session.order.plugins, &app.session.plugin_separators)[1],
        PluginPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            ..
        }
    ));
}

/// Recreating a deleted label starts expanded
#[test]
fn recreating_a_deleted_label_starts_expanded() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Group", Some("Beta.esp"));
    select_separator(&mut app, 0);
    app.handle_key(key(KeyCode::Char(' ')));
    app.handle_key(key(KeyCode::Char('x')));
    app.handle_key(key(KeyCode::Char('y')));
    app.plugins.select(Some(plugin_display_index(&app, 1)));

    submit_new_separator(&mut app, "Group");

    assert!(matches!(
        app.plugins
            .project(&app.session.order.plugins, &app.session.plugin_separators)[1],
        PluginPaneRow::Separator {
            collapsed: false,
            ..
        }
    ));
}

#[test]
fn x_on_a_plugin_row_notes_and_deletes_nothing() {
    let (_tmp, mut app) = app_with_plugins();
    app.plugins.select(Some(0)); // Alpha.esp, a plugin row

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
    add_separator(&mut app, "Group", Some("Beta.esp"));
    select_separator(&mut app, 0);
    let order_before = app.session.order.plugins.clone();
    let plugins_txt = app
        .session
        .instance
        .profile_dir("Default")
        .join("plugins.txt");
    assert!(!plugins_txt.exists());

    // rows before collapse: Alpha, <sep>, Beta
    assert_eq!(
        app.plugins
            .project(&app.session.order.plugins, &app.session.plugin_separators)
            .len(),
        3
    );

    app.handle_key(key(KeyCode::Char(' ')));

    let rows = app
        .plugins
        .project(&app.session.order.plugins, &app.session.plugin_separators);
    assert_eq!(
        rows.len(),
        2,
        "Beta is hidden under the collapsed separator"
    );
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, PluginPaneRow::Plugin { plugin_index: 1 })),
        "the member plugin is not shown"
    );
    assert!(matches!(
        rows[1],
        PluginPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            member_count: 1,
        }
    ));
    assert_eq!(app.session.order.plugins, order_before);
    assert!(!plugins_txt.exists(), "collapse does not persist");
    assert!(app.message.is_none());
}

/// A successful plugin toggle reaches disk before replacing the live load order
#[test]
fn successful_plugin_toggle_persists_and_swaps_live_order() {
    let (_tmp, mut app) = app_with_plugins();
    app.plugins.select(Some(0)); // Alpha.esp
    assert!(app.session.order.plugins[0].active);

    app.toggle_selected();

    assert!(!app.session.order.plugins[0].active, "the plugin flipped");
    let loaded = PluginLoadOrder::load(&app.session.instance, "Default").expect("reload");
    assert!(!loaded.plugins[0].active, "the toggle reached plugins.txt");
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Saved")
    );
}

/// A failed plugins write discards the candidate and preserves all live state
#[test]
fn failed_plugin_toggle_leaves_live_state_unchanged() {
    let (_tmp, mut app) = app_with_plugins();
    app.session
        .order
        .save(&app.session.instance)
        .expect("seed load order");
    let order_before = app.session.order.plugins.clone();
    let profile_before = app.session.profile.rows().to_vec();
    let discovered_before = app.session.discovered.clone();
    let selection_before = app.plugins.index();
    let plugins_txt = app
        .session
        .instance
        .profile_dir("Default")
        .join("plugins.txt");
    std::fs::remove_file(&plugins_txt).expect("remove load order");
    std::fs::create_dir(&plugins_txt).expect("block load order");

    app.toggle_selected();

    assert_eq!(app.session.order.plugins, order_before);
    assert_eq!(app.session.profile.rows(), profile_before);
    assert_eq!(app.session.discovered, discovered_before);
    assert_eq!(app.plugins.index(), selection_before);
    assert!(
        app.message
            .as_ref()
            .is_some_and(|notice| notice.text.starts_with("Could not save load order: "))
    );
}

/// Plugin toggles ignore an obstructed modlist because they persist only plugins.txt
#[test]
fn plugin_toggle_succeeds_when_only_modlist_is_obstructed() {
    let (_tmp, mut app) = app_with_plugins();
    let profile_before = app.session.profile.rows().to_vec();
    let discovered_before = app.session.discovered.clone();
    let modlist = app
        .session
        .instance
        .profile_dir("Default")
        .join("modlist.txt");
    std::fs::remove_file(&modlist).expect("remove mod list");
    std::fs::create_dir(&modlist).expect("block mod list");

    app.toggle_selected();

    assert!(!app.session.order.plugins[0].active);
    let loaded = PluginLoadOrder::load(&app.session.instance, "Default").expect("reload");
    assert!(!loaded.plugins[0].active, "the toggle reached plugins.txt");
    assert_eq!(app.session.profile.rows(), profile_before);
    assert_eq!(app.session.discovered, discovered_before);
    assert!(
        modlist.is_dir(),
        "the mod list obstruction remains untouched"
    );
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Saved")
    );
}

/// Failed insertion restores sidecar and collapse alignment
#[test]
fn failed_insert_restores_sidecar_and_collapse_alignment() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Group", Some("Beta.esp"));
    app.session
        .plugin_separators
        .save(&app.session.instance.profile_dir("Default"))
        .expect("seed sidecar");
    select_separator(&mut app, 0);
    app.handle_key(key(KeyCode::Char(' ')));
    block_profile_writes(&app);

    submit_new_separator(&mut app, "New");

    assert_eq!(app.session.plugin_separators.items.len(), 1);
    assert_eq!(app.session.plugin_separators.items[0].name, "Group");
    assert!(matches!(
        app.plugins
            .project(&app.session.order.plugins, &app.session.plugin_separators)[1],
        PluginPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            ..
        }
    ));
}

/// Failed rename restores sidecar and collapse alignment
#[test]
fn failed_rename_restores_sidecar_and_collapse_alignment() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Old", Some("Beta.esp"));
    app.session
        .plugin_separators
        .save(&app.session.instance.profile_dir("Default"))
        .expect("seed sidecar");
    select_separator(&mut app, 0);
    app.handle_key(key(KeyCode::Char(' ')));
    block_profile_writes(&app);

    app.handle_key(key(KeyCode::Char('R')));
    for c in "New".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.session.plugin_separators.items[0].name, "Old");
    assert!(matches!(
        app.plugins
            .project(&app.session.order.plugins, &app.session.plugin_separators)[1],
        PluginPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            ..
        }
    ));
}

/// Failed deletion restores sidecar and collapse alignment
#[test]
fn failed_delete_restores_sidecar_and_collapse_alignment() {
    let (_tmp, mut app) = app_with_plugins();
    add_separator(&mut app, "Group", Some("Beta.esp"));
    app.session
        .plugin_separators
        .save(&app.session.instance.profile_dir("Default"))
        .expect("seed sidecar");
    select_separator(&mut app, 0);
    app.handle_key(key(KeyCode::Char(' ')));
    block_profile_writes(&app);

    app.handle_key(key(KeyCode::Char('x')));
    app.handle_key(key(KeyCode::Char('y')));

    assert_eq!(app.session.plugin_separators.items[0].name, "Group");
    assert!(matches!(
        app.plugins
            .project(&app.session.order.plugins, &app.session.plugin_separators)[1],
        PluginPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            ..
        }
    ));
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
