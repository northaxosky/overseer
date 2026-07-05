//! Tests for deployment plan generation and file resolution

use super::*;

use crate::test_support::{temp, write};

#[test]
fn empty_mod_list_yields_empty_plan() {
    let (_tmp, base) = temp();
    let plan = DeployPlan::from_mods(&base, &[]).expect("plan builds");
    assert!(plan.is_empty());
    assert_eq!(plan.len(), 0);
    assert_eq!(plan.files().len(), 0);
}

#[test]
fn single_mod_plans_all_its_files() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("Textures/a.dds"), "a");
    write(&m.join("Meshes/b.nif"), "b");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    assert_eq!(plan.len(), 2);
    for f in plan.files() {
        assert_eq!(f.winner, "A");
        assert!(
            f.source.starts_with(&m),
            "source lives under the staging dir"
        );
    }
}

#[test]
fn higher_priority_mod_wins_conflict() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    write(&a.join("Textures/shared.dds"), "from-a");
    write(&b.join("Textures/shared.dds"), "from-b");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("plan");

    assert_eq!(plan.len(), 1, "the shared path collapses to one winner");
    let winner = &plan.files()[0];
    assert_eq!(winner.winner, "B");
    assert!(winner.source.starts_with(&b));
}

#[test]
fn conflict_resolution_is_case_insensitive() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    write(&a.join("Textures/Armor.dds"), "a");
    write(&b.join("textures/armor.dds"), "b");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("plan");

    // Different casing, same logical path on a case-insensitive filesystem
    assert_eq!(plan.len(), 1);
    let winner = &plan.files()[0];
    assert_eq!(winner.winner, "B");
    // The winner keeps its own casing
    assert_eq!(winner.relative.file_name(), Some("armor.dds"));
}

#[test]
fn non_conflicting_files_from_multiple_mods_are_unioned() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    write(&a.join("x.txt"), "x");
    write(&b.join("y.txt"), "y");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("plan");
    assert_eq!(plan.len(), 2);
}

#[test]
fn missing_staging_directory_is_an_error() {
    let (_tmp, base) = temp();
    let missing = base.join("does/not/exist");
    let data = base.join("Data");

    let err = DeployPlan::from_mods(&data, &[ModSource::new("Ghost", &missing)])
        .expect_err("should fail");
    match err {
        DeployError::MissingStaging { mod_name, path } => {
            assert_eq!(mod_name, "Ghost");
            assert_eq!(path, missing);
        }
        other => panic!("expected MissingStaging, got {other:?}"),
    }
}

#[test]
fn empty_staging_directory_contributes_nothing() {
    let (_tmp, base) = temp();
    let m = base.join("mods/Empty");
    std::fs::create_dir_all(&m).expect("create empty staging");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("Empty", &m)]).expect("plan");
    assert!(plan.is_empty());
}

#[test]
fn files_are_ordered_deterministically() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("zeta.txt"), "z");
    write(&m.join("alpha.txt"), "a");
    write(&m.join("mid/beta.txt"), "b");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    let keys: Vec<String> = plan
        .files()
        .iter()
        .map(|f| f.relative.as_str().to_lowercase())
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(
        keys, sorted,
        "files() is sorted by lowercased path (BTreeMap order)"
    );
}

#[test]
fn nested_paths_are_relative_to_staging_root() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("a/b/c.txt"), "c");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    assert_eq!(plan.len(), 1);
    // Build the expectation with join() so the path separator matches the platform
    let expected = Utf8Path::new("a").join("b").join("c.txt");
    assert_eq!(plan.files()[0].relative, expected);
}

#[test]
fn last_mod_in_a_three_way_conflict_wins() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    let c = base.join("mods/C");
    write(&a.join("f.txt"), "a");
    write(&b.join("f.txt"), "b");
    write(&c.join("f.txt"), "c");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(
        &data,
        &[
            ModSource::new("A", &a),
            ModSource::new("B", &b),
            ModSource::new("C", &c),
        ],
    )
    .expect("plan");
    assert_eq!(plan.len(), 1);
    assert_eq!(plan.files()[0].winner, "C");
}

#[test]
fn target_root_is_recorded_on_the_plan() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("f.txt"), "f");
    let data = base.join("Game/Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    assert_eq!(plan.target_root, data);
}

// --- root deployment (from_rooted_mods) ---

#[test]
fn rooted_plan_targets_the_game_dir_and_prefixes_data_content() {
    let (_tmp, base) = temp();
    let m = base.join("mods/M");
    write(&m.join("Textures/x.dds"), "x");
    let game = base.join("Game");

    let plan = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)]).expect("plan");
    assert_eq!(plan.target_root, game);
    assert_eq!(plan.len(), 1);
    assert_eq!(
        plan.files()[0].relative,
        Utf8Path::new("Data").join("Textures").join("x.dds")
    );
}

#[test]
fn rooted_plan_sends_root_content_to_the_game_root() {
    let (_tmp, base) = temp();
    let m = base.join("mods/M");
    write(&m.join("Root/f4se_loader.exe"), "exe");
    write(&m.join("Root/enbseries/enb.ini"), "ini");
    let game = base.join("Game");

    let plan = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)]).expect("plan");
    let relatives: Vec<&Utf8Path> = plan.files().iter().map(|f| f.relative.as_path()).collect();
    // The loose loader lands directly in the game root...
    assert!(relatives.contains(&Utf8Path::new("f4se_loader.exe")));
    // ...and subfolders under Root/ are preserved verbatim
    assert!(relatives.contains(&Utf8Path::new("enbseries").join("enb.ini").as_path()));
}

#[test]
fn rooted_plan_root_marker_is_case_insensitive() {
    let (_tmp, base) = temp();
    let m = base.join("mods/M");
    write(&m.join("root/dxgi.dll"), "dll");
    let game = base.join("Game");

    let plan = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)]).expect("plan");
    assert_eq!(plan.files()[0].relative, Utf8Path::new("dxgi.dll"));
}

#[test]
fn rooted_plan_rejects_a_data_folder_nested_in_root() {
    let (_tmp, base) = temp();
    let m = base.join("mods/M");
    write(&m.join("Root/Data/Sneaky.esp"), "esp");
    let game = base.join("Game");

    let err = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)])
        .expect_err("Root/Data must be rejected");
    match err {
        DeployError::RootDataConflict { name, .. } => assert_eq!(name, "M"),
        other => panic!("expected RootDataConflict, got {other:?}"),
    }
}

#[test]
fn rooted_plan_keeps_same_named_root_and_data_files_separate() {
    let (_tmp, base) = temp();
    let m = base.join("mods/M");
    write(&m.join("Root/x.dll"), "root-side");
    write(&m.join("x.dll"), "data-side");
    let game = base.join("Game");

    let plan = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)]).expect("plan");
    assert_eq!(plan.len(), 2, "the two x.dll target different roots");
    let relatives: Vec<&Utf8Path> = plan.files().iter().map(|f| f.relative.as_path()).collect();
    assert!(relatives.contains(&Utf8Path::new("x.dll")));
    assert!(relatives.contains(&Utf8Path::new("Data").join("x.dll").as_path()));
}

/// A conflict between two mods on the same Root/ file resolves before the Root->game-root remap
#[test]
fn rooted_plan_resolves_a_conflict_among_root_content() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    // Two stacked root-mods ship the same game-root DLL
    write(&a.join("Root/dxgi.dll"), "from-a");
    write(&b.join("Root/dxgi.dll"), "from-b");
    let game = base.join("Game");

    let plan =
        DeployPlan::from_rooted_mods(&game, &[ModSource::new("A", &a), ModSource::new("B", &b)])
            .expect("plan");

    // The conflict collapses to one winner, resolved before the Root/ remap, then mapped to the game root
    assert_eq!(plan.len(), 1);
    let winner = &plan.files()[0];
    assert_eq!(winner.winner, "B");
    assert_eq!(winner.relative, Utf8Path::new("dxgi.dll"));
    assert!(winner.source.starts_with(&b));
}

/// MO2 stamps a meta.ini into every mod root; the deploy plan must never link it into the game
#[test]
fn plan_excludes_a_mods_meta_ini() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    // MO2 metadata, not game content
    write(&m.join("meta.ini"), "[General]");
    write(&m.join("Textures/x.dds"), "pix");
    let data = base.join("Data");

    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    assert_eq!(plan.len(), 1, "only the real asset is planned");
    assert_eq!(
        plan.files()[0].relative,
        Utf8Path::new("Textures").join("x.dds")
    );
}
