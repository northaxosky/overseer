//! Tests for plugin discovery

use super::*;
use crate::instance::{ModKind, ModListEntry, Profile};
use crate::test_support::{FLAG_MASTER, temp_instance, write_plugin};

fn entry(name: &str, enabled: bool) -> ModListEntry {
    ModListEntry {
        name: name.to_owned(),
        enabled,
        kind: ModKind::Managed,
    }
}

fn profile(mods: Vec<ModListEntry>) -> Profile {
    Profile {
        name: "P".to_owned(),
        mods,
        local_saves: false,
    }
}

fn names(plugins: &[PluginMeta]) -> Vec<&str> {
    plugins.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn discovers_plugins_from_enabled_mods_in_priority_order() {
    let (_t, instance) = temp_instance();
    write_plugin(&instance.mods_dir().join("ModA"), "Alpha.esp", 0, &[]);
    write_plugin(&instance.mods_dir().join("ModB"), "Beta.esp", 0, &[]);
    let profile = profile(vec![entry("ModA", true), entry("ModB", true)]);

    let found = discover_plugins(&instance, &profile).expect("discover");
    assert_eq!(names(&found), ["Alpha.esp", "Beta.esp"]);
}

#[test]
fn skips_disabled_mods() {
    let (_t, instance) = temp_instance();
    write_plugin(&instance.mods_dir().join("On"), "On.esp", 0, &[]);
    write_plugin(&instance.mods_dir().join("Off"), "Off.esp", 0, &[]);
    let profile = profile(vec![entry("On", true), entry("Off", false)]);

    let found = discover_plugins(&instance, &profile).expect("discover");
    assert_eq!(names(&found), ["On.esp"]);
}

#[test]
fn higher_priority_mod_wins_a_plugin_name_conflict() {
    let (_t, instance) = temp_instance();
    // Both mods provide Shared.esp; the higher-priority one (ModA, listed first) is a master, the lower-priority one is not — we must read the winner's metadata
    write_plugin(
        &instance.mods_dir().join("ModA"),
        "Shared.esp",
        FLAG_MASTER,
        &[],
    );
    write_plugin(&instance.mods_dir().join("ModB"), "Shared.esp", 0, &[]);
    let profile = profile(vec![entry("ModA", true), entry("ModB", true)]);

    let found = discover_plugins(&instance, &profile).expect("discover");
    assert_eq!(found.len(), 1, "the shared plugin collapses to one");
    assert!(
        found[0].is_master,
        "the winning (higher-priority) copy is the master"
    );
}

#[test]
fn only_top_level_plugins_are_discovered() {
    let (_t, instance) = temp_instance();
    let mod_dir = instance.mods_dir().join("ModA");
    write_plugin(&mod_dir, "Top.esp", 0, &[]);
    // A plugin buried in a subdirectory is loose data, not a loadable plugin
    write_plugin(&mod_dir.join("Meshes"), "Nested.esp", 0, &[]);
    let profile = profile(vec![entry("ModA", true)]);

    let found = discover_plugins(&instance, &profile).expect("discover");
    assert_eq!(names(&found), ["Top.esp"]);
}

#[test]
fn ignores_non_plugin_files() {
    let (_t, instance) = temp_instance();
    let mod_dir = instance.mods_dir().join("ModA");
    write_plugin(&mod_dir, "Real.esp", 0, &[]);
    std::fs::write(mod_dir.join("Textures.ba2"), b"archive").unwrap();
    std::fs::write(mod_dir.join("readme.txt"), b"hi").unwrap();
    let profile = profile(vec![entry("ModA", true)]);

    let found = discover_plugins(&instance, &profile).expect("discover");
    assert_eq!(names(&found), ["Real.esp"]);
}

#[test]
fn missing_mod_folder_contributes_nothing() {
    let (_t, instance) = temp_instance();
    write_plugin(&instance.mods_dir().join("Present"), "Here.esp", 0, &[]);
    // "Absent" is in the list but was never installed (no folder)
    let profile = profile(vec![entry("Absent", true), entry("Present", true)]);

    let found = discover_plugins(&instance, &profile).expect("discover");
    assert_eq!(names(&found), ["Here.esp"]);
}

#[test]
fn carries_metadata_through() {
    let (_t, instance) = temp_instance();
    write_plugin(
        &instance.mods_dir().join("ModA"),
        "Dep.esp",
        0,
        &["Fallout4.esm"],
    );
    let profile = profile(vec![entry("ModA", true)]);

    let found = discover_plugins(&instance, &profile).expect("discover");
    assert_eq!(found[0].masters, ["Fallout4.esm"]);
}

#[test]
fn plugins_within_a_mod_are_sorted_deterministically() {
    let (_t, instance) = temp_instance();
    // Written out of order; discovery must return them name-sorted, not in FS order
    write_plugin(&instance.mods_dir().join("ModA"), "Zeta.esp", 0, &[]);
    write_plugin(&instance.mods_dir().join("ModA"), "alpha.esp", 0, &[]);
    let profile = profile(vec![entry("ModA", true)]);

    let found = discover_plugins(&instance, &profile).expect("discover");
    assert_eq!(names(&found), ["alpha.esp", "Zeta.esp"]);
}

#[test]
fn empty_profile_discovers_nothing() {
    let (_t, instance) = temp_instance();
    let found = discover_plugins(&instance, &profile(vec![])).expect("discover");
    assert!(found.is_empty());
}

#[test]
fn lenient_discovery_collects_unreadable_plugins_and_keeps_the_rest() {
    let (_t, instance) = temp_instance();
    let mod_dir = instance.mods_dir().join("ModA");
    write_plugin(&mod_dir, "Good.esp", 0, &[]);
    std::fs::write(mod_dir.join("Corrupt.esp"), b"not a valid plugin").unwrap();
    let profile = profile(vec![entry("ModA", true)]);

    let (readable, unreadable) =
        discover_plugins_lenient(&instance, &profile).expect("lenient discovery");
    assert_eq!(names(&readable), ["Good.esp"]);
    assert_eq!(unreadable.len(), 1);
    assert_eq!(unreadable[0].name, "Corrupt.esp");
    assert!(!unreadable[0].reason.is_empty());

    // Strict discovery still fails hard on the same corrupt plugin
    assert!(discover_plugins(&instance, &profile).is_err());
}
