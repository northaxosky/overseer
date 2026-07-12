//! Tests for Plugins pane projection and separator collapse state

use super::*;
use overseer_core::plugins::Separator;

/// Build a plugin fixture
fn plugin(name: &str) -> PluginEntry {
    PluginEntry {
        name: name.to_owned(),
        active: true,
    }
}

/// Build a sidecar separator fixture
fn separator(name: &str, anchor: Option<&str>) -> Separator {
    Separator {
        name: name.to_owned(),
        anchor: anchor.map(str::to_owned),
    }
}

/// Build a separator collection fixture
fn separators(items: Vec<Separator>) -> PluginSeparators {
    PluginSeparators { items }
}

/// Projection preserves core anchor ordering
#[test]
fn projection_matches_core_anchor_ordering() {
    let plugins = vec![plugin("A.esp"), plugin("B.esp")];
    let separators = separators(vec![
        separator("Trailing", None),
        separator("Above B", Some("B.esp")),
        separator("Above A", Some("A.esp")),
    ]);
    let pane = PluginsPane::new(&plugins, &separators);

    assert_eq!(
        pane.project(&plugins, &separators),
        [
            PluginPaneRow::Separator {
                separator_index: 2,
                collapsed: false,
                member_count: 1,
            },
            PluginPaneRow::Plugin { plugin_index: 0 },
            PluginPaneRow::Separator {
                separator_index: 1,
                collapsed: false,
                member_count: 1,
            },
            PluginPaneRow::Plugin { plugin_index: 1 },
            PluginPaneRow::Separator {
                separator_index: 0,
                collapsed: false,
                member_count: 0,
            },
        ]
    );
}

/// Collapse hides only members owned by the separator
#[test]
fn collapse_hides_only_the_separators_members() {
    let plugins = vec![plugin("A.esp"), plugin("B.esp"), plugin("C.esp")];
    let separators = separators(vec![
        separator("Group A", Some("A.esp")),
        separator("Group C", Some("C.esp")),
    ]);
    let mut pane = PluginsPane::new(&plugins, &separators);
    pane.toggle_separator(0);

    assert_eq!(
        pane.project(&plugins, &separators),
        [
            PluginPaneRow::Separator {
                separator_index: 0,
                collapsed: true,
                member_count: 2,
            },
            PluginPaneRow::Separator {
                separator_index: 1,
                collapsed: false,
                member_count: 1,
            },
            PluginPaneRow::Plugin { plugin_index: 2 },
        ]
    );
}

/// Same-anchor separators preserve sidecar order and latest ownership
#[test]
fn same_anchor_separators_keep_sidecar_order_and_latest_owns_members() {
    let plugins = vec![plugin("A.esp")];
    let separators = separators(vec![
        separator("First", Some("A.esp")),
        separator("Second", Some("A.esp")),
    ]);
    let pane = PluginsPane::new(&plugins, &separators);

    assert_eq!(
        pane.project(&plugins, &separators),
        [
            PluginPaneRow::Separator {
                separator_index: 0,
                collapsed: false,
                member_count: 0,
            },
            PluginPaneRow::Separator {
                separator_index: 1,
                collapsed: false,
                member_count: 1,
            },
            PluginPaneRow::Plugin { plugin_index: 0 },
        ]
    );
}

/// Trailing and stale separators own no plugins
#[test]
fn trailing_and_stale_separators_have_no_members() {
    let plugins = vec![plugin("A.esp")];
    let separators = separators(vec![
        separator("Trailing", None),
        separator("Stale", Some("Gone.esp")),
    ]);
    let pane = PluginsPane::new(&plugins, &separators);

    assert_eq!(
        pane.project(&plugins, &separators),
        [
            PluginPaneRow::Plugin { plugin_index: 0 },
            PluginPaneRow::Separator {
                separator_index: 0,
                collapsed: false,
                member_count: 0,
            },
            PluginPaneRow::Separator {
                separator_index: 1,
                collapsed: false,
                member_count: 0,
            },
        ]
    );
}

/// Duplicate labels retain independent collapse state
#[test]
fn duplicate_labels_collapse_independently() {
    let plugins = vec![plugin("A.esp"), plugin("B.esp")];
    let separators = separators(vec![
        separator("Duplicate", Some("A.esp")),
        separator("Duplicate", Some("B.esp")),
    ]);
    let mut pane = PluginsPane::new(&plugins, &separators);
    pane.toggle_separator(0);

    assert!(matches!(
        pane.project(&plugins, &separators)[0],
        PluginPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            ..
        }
    ));
    assert!(matches!(
        pane.project(&plugins, &separators)[1],
        PluginPaneRow::Separator {
            separator_index: 1,
            collapsed: false,
            ..
        }
    ));
}

/// Rename preserves collapse state by sidecar index
#[test]
fn rename_retains_collapse_by_sidecar_index() {
    let plugins = vec![plugin("A.esp")];
    let mut separators = separators(vec![separator("Old", Some("A.esp"))]);
    let mut pane = PluginsPane::new(&plugins, &separators);
    pane.toggle_separator(0);
    separators.items[0].name = "New".to_owned();

    assert!(matches!(
        pane.project(&plugins, &separators)[0],
        PluginPaneRow::Separator {
            collapsed: true,
            ..
        }
    ));
}

/// Recreation inserts expanded collapse state
#[test]
fn delete_then_recreate_inserts_an_expanded_entry() {
    let plugins = vec![plugin("A.esp")];
    let mut separators = separators(vec![separator("Group", Some("A.esp"))]);
    let mut pane = PluginsPane::new(&plugins, &separators);
    pane.toggle_separator(0);

    separators.items.remove(0);
    pane.remove_separator(0);
    separators.items.push(separator("Group", Some("A.esp")));
    pane.insert_separator(0);

    assert!(matches!(
        pane.project(&plugins, &separators)[0],
        PluginPaneRow::Separator {
            collapsed: false,
            ..
        }
    ));
}

#[test]
fn reconcile_preserves_collapse_and_clamps_when_separator_counts_match() {
    let initial_plugins = vec![plugin("A.esp"), plugin("B.esp"), plugin("C.esp")];
    let initial_separators = separators(vec![separator("Group", Some("B.esp"))]);
    let replacement_plugins = vec![plugin("A.esp"), plugin("B.esp")];
    let replacement_separators = separators(vec![separator("Renamed", Some("B.esp"))]);
    let mut pane = PluginsPane::new(&initial_plugins, &initial_separators);
    pane.toggle_separator(0);
    pane.select(Some(8));

    pane.reconcile_model(&replacement_plugins, &replacement_separators);

    let rows = pane.project(&replacement_plugins, &replacement_separators);
    assert!(rows.iter().any(|row| matches!(
        row,
        PluginPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            ..
        }
    )));
    assert_eq!(pane.index(), Some(rows.len() - 1));
}

#[test]
fn reconcile_resets_mismatched_collapse_before_projection_and_clamps() {
    let initial_plugins = vec![plugin("A.esp")];
    let initial_separators = separators(vec![separator("Group", Some("A.esp"))]);
    let replacement_plugins = vec![plugin("A.esp"), plugin("B.esp")];
    let replacement_separators = separators(vec![
        separator("Group", Some("A.esp")),
        separator("Other", Some("B.esp")),
    ]);
    let mut pane = PluginsPane::new(&initial_plugins, &initial_separators);
    pane.toggle_separator(0);
    pane.select(Some(8));

    pane.reconcile_model(&replacement_plugins, &replacement_separators);

    let rows = pane.project(&replacement_plugins, &replacement_separators);
    assert!(rows.iter().all(|row| !matches!(
        row,
        PluginPaneRow::Separator {
            collapsed: true,
            ..
        }
    )));
    assert_eq!(pane.index(), Some(rows.len() - 1));
    assert_ne!(pane.index(), Some(0));
}

/// Projection rejects plugin separator collapse misalignment
#[test]
#[should_panic(expected = "plugin separator collapse state must align with sidecar order")]
fn projection_rejects_collapse_misalignment() {
    let pane = PluginsPane::new(&[], &PluginSeparators::default());
    pane.project(&[], &separators(vec![separator("Unexpected", None)]));
}
