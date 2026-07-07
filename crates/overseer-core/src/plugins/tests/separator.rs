//! Tests for plugin separators: the sidecar, validation, and merge ordering

use super::*;
use crate::test_support::temp;

fn plugin(name: &str) -> PluginEntry {
    PluginEntry {
        name: name.to_owned(),
        active: true,
    }
}

fn sep(name: &str, anchor: Option<&str>) -> Separator {
    Separator {
        name: name.to_owned(),
        anchor: anchor.map(str::to_owned),
    }
}

#[test]
fn the_sidecar_round_trips_including_a_trailing_none_anchor() {
    let (_tmp, dir) = temp();
    let seps = PluginSeparators {
        items: vec![
            sep("Masters", Some("Base.esm")),
            sep("Gameplay", Some("Cool.esp")),
            sep("Bottom", None),
        ],
    };
    seps.save(&dir).expect("save");
    let loaded = PluginSeparators::load(&dir).expect("load");
    assert_eq!(loaded.items, seps.items);
}

#[test]
fn a_missing_sidecar_loads_empty() {
    let (_tmp, dir) = temp();
    let loaded = PluginSeparators::load(&dir).expect("missing file is not an error");
    assert!(loaded.items.is_empty());
}

#[test]
fn validate_separator_name_rejects_unsafe_names_and_accepts_a_normal_one() {
    assert!(validate_separator_name("").is_err(), "empty");
    assert!(validate_separator_name("   ").is_err(), "whitespace only");
    assert!(
        validate_separator_name("a\tb").is_err(),
        "tab is a control char"
    );
    assert!(validate_separator_name("a\nb").is_err(), "newline");
    assert!(validate_separator_name("a/b").is_err(), "forward slash");
    assert!(validate_separator_name("a\\b").is_err(), "backslash");
    assert!(validate_separator_name("#lead").is_err(), "leading #");
    assert!(validate_separator_name("*lead").is_err(), "leading *");
    assert_eq!(
        validate_separator_name("  Gameplay Tweaks  ").expect("valid"),
        "Gameplay Tweaks",
        "a normal name is accepted and trimmed"
    );
}

#[test]
fn merge_rows_places_a_separator_above_its_anchor() {
    let plugins = [plugin("A.esp"), plugin("B.esp")];
    let seps = [sep("Group", Some("B.esp"))];
    assert_eq!(
        merge_rows(&plugins, &seps),
        vec![
            PluginRow::Plugin(0),
            PluginRow::Separator(0),
            PluginRow::Plugin(1),
        ]
    );
}

#[test]
fn merge_rows_stacks_two_separators_sharing_an_anchor_in_items_order() {
    let plugins = [plugin("A.esp")];
    let seps = [sep("First", Some("A.esp")), sep("Second", Some("A.esp"))];
    assert_eq!(
        merge_rows(&plugins, &seps),
        vec![
            PluginRow::Separator(0),
            PluginRow::Separator(1),
            PluginRow::Plugin(0),
        ]
    );
}

#[test]
fn merge_rows_sends_none_and_stale_anchors_to_the_end_in_order() {
    let plugins = [plugin("A.esp")];
    let seps = [
        sep("Trailing", None),
        sep("Anchored", Some("A.esp")),
        sep("Stale", Some("Gone.esp")),
    ];
    assert_eq!(
        merge_rows(&plugins, &seps),
        vec![
            PluginRow::Separator(1), // anchored above A.esp
            PluginRow::Plugin(0),
            PluginRow::Separator(0), // None-anchor, first in items order
            PluginRow::Separator(2), // stale anchor, after it
        ]
    );
}

#[test]
fn merge_rows_matches_the_anchor_case_insensitively() {
    let plugins = [plugin("Cool.ESP")];
    let seps = [sep("Group", Some("cool.esp"))];
    assert_eq!(
        merge_rows(&plugins, &seps),
        vec![PluginRow::Separator(0), PluginRow::Plugin(0)]
    );
}

#[test]
fn merge_rows_emits_a_separator_once_when_duplicate_plugin_names_match_its_anchor() {
    let plugins = [plugin("Dup.esp"), plugin("dup.ESP")];
    let seps = [sep("Group", Some("dup.esp"))];
    let rows = merge_rows(&plugins, &seps);
    let separators: Vec<PluginRow> = rows
        .iter()
        .copied()
        .filter(|r| matches!(r, PluginRow::Separator(_)))
        .collect();
    assert_eq!(
        separators,
        vec![PluginRow::Separator(0)],
        "a separator is emitted exactly once despite duplicate anchor matches"
    );
    let first_plugin = rows
        .iter()
        .position(|r| matches!(r, PluginRow::Plugin(_)))
        .expect("a plugin row exists");
    let separator_at = rows
        .iter()
        .position(|r| matches!(r, PluginRow::Separator(0)))
        .expect("the separator row exists");
    assert!(
        separator_at < first_plugin,
        "the separator sits above the first matching plugin"
    );
}

#[test]
fn insert_clamps_out_of_range_and_rename_remove_are_index_based() {
    let mut seps = PluginSeparators::default();
    seps.insert(99, Some("A.esp".to_owned()), "Group")
        .expect("insert clamps to the end");
    assert_eq!(seps.items.len(), 1);

    seps.rename(0, "Renamed").expect("rename");
    assert_eq!(seps.items[0].name, "Renamed");
    assert!(
        seps.rename(5, "Nope").is_err(),
        "rename out of bounds errors"
    );

    seps.remove(0).expect("remove");
    assert!(seps.items.is_empty());
    assert!(seps.remove(0).is_err(), "remove out of bounds errors");
}
