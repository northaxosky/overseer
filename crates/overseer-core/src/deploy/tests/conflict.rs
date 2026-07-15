//! Tests for conflict detection between mods

use super::*;

use crate::test_support::{temp, write};
use camino::Utf8Path;

fn provider_names(entry: &DestinationEntry) -> Vec<&str> {
    entry
        .providers
        .iter()
        .map(|provider| provider.origin.display_name())
        .collect()
}

#[test]
fn two_mods_sharing_a_file_report_one_conflict_in_priority_order() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    write(&a.join("Textures/shared.dds"), "from-a");
    write(&b.join("Textures/shared.dds"), "from-b");

    let snapshot = ConflictSnapshot::build(&[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("build conflict snapshot");
    let conflicts = snapshot.conflicts();

    assert_eq!(conflicts.len(), 1);
    // Providers in priority order, the higher-priority mod last
    assert_eq!(provider_names(&conflicts[0]), ["A", "B"]);
    assert_eq!(
        conflicts[0].destination,
        Utf8Path::new("Data").join("Textures").join("shared.dds")
    );
}

#[test]
fn three_mods_sharing_a_file_list_all_providers_winner_last() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    let c = base.join("mods/C");
    write(&a.join("f.txt"), "a");
    write(&b.join("f.txt"), "b");
    write(&c.join("f.txt"), "c");

    let snapshot = ConflictSnapshot::build(&[
        ModSource::new("A", &a),
        ModSource::new("B", &b),
        ModSource::new("C", &c),
    ])
    .expect("build conflict snapshot");
    let conflicts = snapshot.conflicts();

    assert_eq!(conflicts.len(), 1);
    assert_eq!(provider_names(&conflicts[0]), ["A", "B", "C"]);
}

#[test]
fn case_only_differences_collapse_to_one_conflict() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    write(&a.join("Textures/foo.dds"), "a");
    write(&b.join("textures/Foo.dds"), "b");

    let snapshot = ConflictSnapshot::build(&[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("build conflict snapshot");
    let conflicts = snapshot.conflicts();

    assert_eq!(conflicts.len(), 1);
    assert_eq!(provider_names(&conflicts[0]), ["A", "B"]);
    // The winner's casing is retained for display
    assert_eq!(
        conflicts[0].destination,
        Utf8Path::new("Data").join("textures").join("Foo.dds")
    );
}

#[test]
fn files_unique_to_one_mod_are_not_conflicts() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    let c = base.join("mods/C");
    write(&a.join("shared.dds"), "a");
    write(&a.join("only_a.dds"), "a");
    write(&b.join("shared.dds"), "b");
    // C overlaps nothing and must contribute no conflicts
    write(&c.join("only_c.dds"), "c");

    let snapshot = ConflictSnapshot::build(&[
        ModSource::new("A", &a),
        ModSource::new("B", &b),
        ModSource::new("C", &c),
    ])
    .expect("build conflict snapshot");
    let conflicts = snapshot.conflicts();

    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        conflicts[0].destination,
        Utf8Path::new("Data").join("shared.dds")
    );
    assert_eq!(provider_names(&conflicts[0]), ["A", "B"]);
}

#[test]
fn nested_files_conflict_and_directories_are_skipped() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    write(&a.join("Meshes/armor/x.nif"), "a");
    write(&b.join("Meshes/armor/x.nif"), "b");

    let snapshot = ConflictSnapshot::build(&[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("build conflict snapshot");
    let conflicts = snapshot.conflicts();

    // Only the file collides; the shared `Meshes` and `Meshes/armor` dirs are skipped
    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        conflicts[0].destination,
        Utf8Path::new("Data")
            .join("Meshes")
            .join("armor")
            .join("x.nif")
    );
    assert_eq!(provider_names(&conflicts[0]), ["A", "B"]);
}

#[test]
fn empty_mod_list_has_no_conflicts() {
    let snapshot = ConflictSnapshot::build(&[]).expect("build conflict snapshot");
    assert!(snapshot.is_empty());
}

#[test]
fn a_single_mod_has_no_conflicts() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    write(&a.join("Textures/x.dds"), "a");
    write(&a.join("Meshes/y.nif"), "a");

    let snapshot =
        ConflictSnapshot::build(&[ModSource::new("A", &a)]).expect("build conflict snapshot");
    assert!(snapshot.is_empty());
}

// Two files differing only by case are distinct on a case-sensitive FS but collapse to one key on a case-insensitive one, so a mod must never be reported as conflicting with itself. This can't be staged on Windows's case-insensitive FS, hence cfg(unix)
#[cfg(unix)]
#[test]
fn case_collision_within_one_mod_is_not_a_self_conflict() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    write(&a.join("Foo.dds"), "upper");
    write(&a.join("foo.dds"), "lower");

    let snapshot =
        ConflictSnapshot::build(&[ModSource::new("A", &a)]).expect("build conflict snapshot");
    assert!(snapshot.is_empty());
}

#[test]
fn missing_staging_directory_is_an_error() {
    let (_tmp, base) = temp();
    let missing = base.join("does/not/exist");

    let err = ConflictSnapshot::build(&[ModSource::new("Ghost", &missing)])
        .expect_err("missing staging directory should fail");
    match err {
        DeployError::MissingStaging { mod_name, path } => {
            assert_eq!(mod_name, "Ghost");
            assert_eq!(path, missing);
        }
        other => panic!("expected MissingStaging, got {other:?}"),
    }
}

#[test]
fn conflicts_are_sorted_by_destination() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    // Stage shared files out of order to prove the output is sorted
    write(&a.join("zeta.txt"), "a");
    write(&a.join("alpha.txt"), "a");
    write(&a.join("mid/beta.txt"), "a");
    write(&b.join("zeta.txt"), "b");
    write(&b.join("alpha.txt"), "b");
    write(&b.join("mid/beta.txt"), "b");

    let snapshot = ConflictSnapshot::build(&[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("build conflict snapshot");
    let conflicts = snapshot.conflicts();

    assert_eq!(conflicts.len(), 3);
    let destinations: Vec<_> = conflicts
        .iter()
        .map(|conflict| conflict.destination.clone())
        .collect();
    assert_eq!(
        destinations,
        [
            Utf8Path::new("Data").join("alpha.txt"),
            Utf8Path::new("Data").join("mid").join("beta.txt"),
            Utf8Path::new("Data").join("zeta.txt")
        ]
    );
}

#[test]
fn per_mod_meta_ini_is_excluded_from_conflicts() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    // MO2 writes a meta.ini into every mod root; it must not register as a conflict
    write(&a.join("meta.ini"), "[General]");
    write(&b.join("meta.ini"), "[General]");
    write(&a.join("Textures/shared.dds"), "a");
    write(&b.join("Textures/shared.dds"), "b");

    let snapshot = ConflictSnapshot::build(&[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("build conflict snapshot");
    let conflicts = snapshot.conflicts();

    // Only the real shared asset conflicts; the two meta.ini files are ignored
    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        conflicts[0].destination,
        Utf8Path::new("Data").join("Textures").join("shared.dds")
    );
}

#[test]
fn overseer_provenance_is_excluded_from_conflicts() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    write(&a.join(".overseer-mod.toml"), "archive = \"A.zip\"");
    write(&b.join(".OVERSEER-MOD.TOML"), "archive = \"B.zip\"");

    let snapshot = ConflictSnapshot::build(&[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("build conflict snapshot");

    assert!(snapshot.is_empty());
}
