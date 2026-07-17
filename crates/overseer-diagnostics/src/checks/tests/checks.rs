//! Tests for the check registry and shared check plumbing

use super::*;
use std::collections::BTreeSet;

#[test]
fn every_check_id_is_non_empty_and_unique() {
    let ids: Vec<&str> = CHECKS.iter().map(|c| c.id).collect();
    assert!(ids.iter().all(|id| !id.is_empty()), "no check id is empty");
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "check ids are unique");
}

#[test]
fn the_registry_lists_every_check_in_display_order() {
    let ids: Vec<&str> = CHECKS.iter().map(|c| c.id).collect();
    assert_eq!(
        ids,
        vec![
            "plugins",
            "plugin-count",
            "plugin-order",
            "missing-masters",
            "race-subgraphs",
            "loose-files",
            "loose-folders",
            "creation-club",
            "ini-config",
            "archives",
            "f4se",
            "header-versions",
            "archive-names",
            "script-overrides",
            "binaries",
            "dlc-consistency",
        ]
    );
}
