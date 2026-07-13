//! Tests for the list-driven merge transaction

use super::*;
use crate::instance::{Instance, Profile};
use crate::plugins::{PluginEntry, PluginLoadOrder};
use crate::test_support::{save_profile, temp_instance, write_plugin};
use camino::Utf8Path;
use tempfile::TempDir;

/// The instance's game `Data/` directory
fn data_dir(instance: &Instance) -> camino::Utf8PathBuf {
    instance.config.game_dir.join(crate::deploy::DATA_DIR)
}

/// Write a one-file GNRL archive at `path`
fn write_main_ba2(path: &Utf8Path, entry: &str) {
    let files = [crate::ba2::Ba2File {
        path: entry.to_owned(),
        bytes: b"payload".to_vec(),
    }];
    let img = crate::ba2::pack_general(&files, |_| false).expect("pack general");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create data dir");
    }
    std::fs::write(path, img).expect("write ba2");
}

/// Persist a profile's `plugins.txt` from `(name, active)` pairs in load order
fn save_load_order(instance: &Instance, profile: &str, entries: &[(&str, bool)]) {
    PluginLoadOrder {
        profile: profile.to_owned(),
        plugins: entries
            .iter()
            .map(|(name, active)| PluginEntry {
                name: (*name).to_owned(),
                active: *active,
            })
            .collect(),
    }
    .save(instance)
    .expect("save plugins.txt");
}

/// An instance with two active source plugins (`ModA`/`ModB`, each with a Main BA2) and two profiles
fn setup() -> (TempDir, Instance, Profile) {
    let (dir, instance) = temp_instance();
    let data = data_dir(&instance);
    std::fs::create_dir_all(&data).expect("create data dir");
    write_plugin(&data, "ModA.esp", 0, &[]);
    write_plugin(&data, "ModB.esp", 0, &[]);
    write_main_ba2(&data.join("ModA - Main.ba2"), "a/one.nif");
    write_main_ba2(&data.join("ModB - Main.ba2"), "b/two.nif");
    save_profile(&instance, "Default", &[]);
    save_profile(&instance, "Alt", &[]);
    save_load_order(
        &instance,
        "Default",
        &[("ModA.esp", true), ("ModB.esp", true)],
    );
    let profile = Profile::load(&instance, "Default").expect("load profile");
    (dir, instance, profile)
}

/// A merge request with the standard texture cap
fn request(name: &str, plugins: &[&str]) -> MergeRequest {
    MergeRequest {
        name: name.to_owned(),
        plugins: plugins.iter().map(|s| (*s).to_owned()).collect(),
        texture_group_bytes: crate::merge::DEFAULT_TEXTURE_GROUP_BYTES,
    }
}

#[test]
fn resolve_partitions_the_plugin_list() {
    let (_dir, instance, _profile) = setup();
    let data = data_dir(&instance);
    write_plugin(&data, "Inactive.esp", 0, &[]);
    write_plugin(&data, "Orphan.esp", 0, &[]);
    save_load_order(
        &instance,
        "Default",
        &[
            ("ModA.esp", true),
            ("ModB.esp", true),
            ("Inactive.esp", false),
            ("Orphan.esp", true),
        ],
    );
    let profile = Profile::load(&instance, "Default").expect("reload profile");

    let requested: Vec<String> = [
        "ModA.esp",
        "ModB.esp",
        "Inactive.esp",
        "Orphan.esp",
        "Ghost.esp",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let plan = resolve(&instance, &profile, &requested).expect("resolve");

    let items: Vec<&str> = plan.items.iter().map(|i| i.plugin.as_str()).collect();
    assert_eq!(items, ["ModA.esp", "ModB.esp"]);
    assert_eq!(plan.items[0].rank, 0);
    assert_eq!(plan.items[1].rank, 1);
    assert_eq!(plan.inactive, ["Inactive.esp"]);
    assert_eq!(plan.orphaned, ["Orphan.esp"]);
    assert_eq!(plan.missing, ["Ghost.esp"]);
}

#[test]
fn resolve_deduplicates_requested_names_case_insensitively() {
    let (_dir, instance, profile) = setup();
    let requested: Vec<String> = ["ModA.esp", "moda.ESP", "ModA.esp"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let plan = resolve(&instance, &profile, &requested).expect("resolve");
    assert_eq!(plan.items.len(), 1);
}

#[test]
fn run_materializes_a_mod_and_backs_up_sources() {
    let (_dir, instance, profile) = setup();
    let report = run(
        &instance,
        &profile,
        &request("Merged", &["ModA.esp", "ModB.esp"]),
    )
    .expect("run merge");

    assert_eq!(report.sources_removed, 2);
    assert_eq!(report.archives.gnrl, 1);
    assert_eq!(report.archives.dx10, 0);

    let mod_dir = instance.mods_dir().join("Merged");
    assert!(mod_dir.join("Merged_Main - Main.ba2").exists());
    assert!(mod_dir.join("Merged_Main.esl").exists());

    let data = data_dir(&instance);
    assert!(!data.join("ModA - Main.ba2").exists());
    assert!(!data.join("ModB - Main.ba2").exists());

    let backup = instance.root.join("merges").join("Merged");
    assert!(backup.join("ModA - Main.ba2").exists());
    assert!(backup.join("ModB - Main.ba2").exists());
    assert!(instance.root.join("merges").join("Merged.json").exists());
}

#[test]
fn run_registers_the_mod_in_every_profile() {
    let (_dir, instance, profile) = setup();
    run(
        &instance,
        &profile,
        &request("Merged", &["ModA.esp", "ModB.esp"]),
    )
    .expect("run merge");
    for name in ["Default", "Alt"] {
        let profile = Profile::load(&instance, name).expect("load profile");
        assert!(
            profile.contains("Merged"),
            "profile {name} should carry the merge"
        );
    }
}

#[test]
fn restore_reverses_a_committed_merge() {
    let (_dir, instance, profile) = setup();
    run(
        &instance,
        &profile,
        &request("Merged", &["ModA.esp", "ModB.esp"]),
    )
    .expect("run merge");
    restore(&instance, "Merged").expect("restore");

    let data = data_dir(&instance);
    assert!(data.join("ModA - Main.ba2").exists());
    assert!(data.join("ModB - Main.ba2").exists());
    assert!(!instance.mods_dir().join("Merged").exists());
    assert!(!instance.root.join("merges").join("Merged.json").exists());
    assert!(!instance.root.join("merges").join("Merged").exists());
    for name in ["Default", "Alt"] {
        assert!(
            !Profile::load(&instance, name)
                .expect("load")
                .contains("Merged")
        );
    }
}

#[test]
fn pending_mod_state_blocks_restore_before_source_mutation() {
    let (_dir, instance, profile) = setup();
    run(
        &instance,
        &profile,
        &request("Merged", &["ModA.esp", "ModB.esp"]),
    )
    .expect("run merge");
    let profile_path = instance.profile_dir("Default").join("modlist.txt");
    let profile_before = std::fs::read(&profile_path).expect("read profile");
    let data = data_dir(&instance);
    let backup = instance.root.join("merges/Merged/ModA - Main.ba2");
    let manifest = instance.root.join("merges/Merged.json");
    let pending = instance.pending_mod_operation_dir();
    std::fs::create_dir_all(instance.state_dir()).expect("create state");
    std::fs::write(&pending, "pending").expect("write residue");

    let error = restore(&instance, "Merged").expect_err("blocked restore");

    assert!(matches!(
        error,
        MergeTxnError::Instance(InstanceError::PendingModOperation { path }) if path == pending
    ));
    assert!(!data.join("ModA - Main.ba2").exists());
    assert!(backup.exists());
    assert!(manifest.exists());
    assert!(instance.mods_dir().join("Merged").exists());
    assert_eq!(
        std::fs::read(profile_path).expect("read profile"),
        profile_before
    );
}

#[test]
fn run_refuses_when_deployed() {
    let (_dir, instance, profile) = setup();
    std::fs::create_dir_all(instance.state_dir()).expect("state dir");
    std::fs::write(instance.state_dir().join("deployment.json"), "{}").expect("fake deployment");
    let err = run(&instance, &profile, &request("Merged", &["ModA.esp"])).unwrap_err();
    assert!(matches!(err, MergeTxnError::Deployed));
}

#[test]
fn run_rejects_an_empty_plan() {
    let (_dir, instance, profile) = setup();
    let err = run(&instance, &profile, &request("Merged", &["Ghost.esp"])).unwrap_err();
    assert!(matches!(err, MergeTxnError::NothingToMerge));
}

#[test]
fn run_refuses_a_duplicate_name() {
    let (_dir, instance, profile) = setup();
    run(&instance, &profile, &request("Merged", &["ModA.esp"])).expect("first merge");
    let err = run(&instance, &profile, &request("Merged", &["ModB.esp"])).unwrap_err();
    assert!(matches!(err, MergeTxnError::NameExists(_)));
}

#[test]
fn restore_of_an_unknown_merge_errs() {
    let (_dir, instance, _profile) = setup();
    let err = restore(&instance, "Nope").unwrap_err();
    assert!(matches!(err, MergeTxnError::NoSuchMerge(_)));
}

#[test]
fn resolve_flags_plugins_already_owned_by_a_merge() {
    let (_dir, instance, profile) = setup();
    run(&instance, &profile, &request("Merged", &["ModA.esp"])).expect("first merge");

    let requested = vec!["ModA.esp".to_owned()];
    let plan = resolve(&instance, &profile, &requested).expect("resolve");
    assert!(plan.items.is_empty());
    assert_eq!(
        plan.already_merged,
        vec![("ModA.esp".to_owned(), "Merged".to_owned())]
    );
}

#[test]
fn restore_preserves_backups_when_an_original_reappears() {
    let (_dir, instance, profile) = setup();
    run(
        &instance,
        &profile,
        &request("Merged", &["ModA.esp", "ModB.esp"]),
    )
    .expect("run merge");
    // a storefront or reinstall re-adds a source BA2 while its backup still exists
    write_main_ba2(&data_dir(&instance).join("ModA - Main.ba2"), "a/one.nif");

    let err = restore(&instance, "Merged").unwrap_err();
    assert!(matches!(err, MergeTxnError::RestoreConflict(_)));

    // nothing was destroyed: backups, manifest, and the mod all survive for manual resolution
    let backup = instance.root.join("merges").join("Merged");
    assert!(backup.join("ModA - Main.ba2").exists());
    assert!(backup.join("ModB - Main.ba2").exists());
    assert!(instance.root.join("merges").join("Merged.json").exists());
    assert!(instance.mods_dir().join("Merged").exists());
}

#[test]
fn restore_rejects_an_unsafe_name() {
    let (_dir, instance, _profile) = setup();
    let err = restore(&instance, "../escape").unwrap_err();
    assert!(matches!(err, MergeTxnError::InvalidName(_)));
}
