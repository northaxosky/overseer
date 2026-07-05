//! Tests for the Fallout 4 core-binary edition policy

use super::*;

#[test]
fn explicit_item_resolves_a_core_binary() {
    let item = explicit_item(Generation::OldGen, "Fallout4.exe").unwrap();
    assert_eq!(item.rel_path, "Fallout4.exe");
    assert_eq!(item.group, "core");
}

#[test]
fn explicit_item_rejects_a_non_core_file() {
    assert!(explicit_item(Generation::OldGen, "Data/DLCCoast.esm").is_none());
}

#[test]
fn core_group_holds_exactly_the_three_binaries() {
    assert_eq!(CORE_GROUP.files, CORE_BINARIES);
    assert_eq!(CORE_BINARIES.len(), 3);
}

#[test]
fn target_completeness_tracks_known_editions() {
    assert!(target_is_complete(Generation::OldGen));
    assert!(target_is_complete(Generation::Anniversary));
    assert!(!target_is_complete(Generation::NextGen));
}
