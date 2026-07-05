//! Tests for the NTFS hard-link deployment backend

use super::*;
use crate::deploy::{ModSource, NullSink};
use camino::Utf8PathBuf;

use crate::test_support::{temp, write};

#[test]
fn launch_reports_a_missing_executable() {
    let target = LaunchTarget {
        program: Utf8PathBuf::from("definitely/missing/tool.exe"),
        args: vec![],
        working_dir: Utf8PathBuf::from("."),
    };
    let err = HardlinkDeployer::new()
        .launch(&target)
        .expect_err("a missing program must error");
    assert!(matches!(err, DeployError::Launch { .. }));
}

#[cfg(windows)]
#[test]
fn launch_spawns_an_executable() {
    // cmd.exe is on every Windows runner; `/c exit` returns immediately
    let (_tmp, base) = temp();
    let cmd =
        std::env::var("COMSPEC").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".to_owned());
    let target = LaunchTarget {
        program: Utf8PathBuf::from(cmd),
        args: vec!["/c".to_owned(), "exit".to_owned()],
        working_dir: base,
    };
    HardlinkDeployer::new()
        .launch(&target)
        .expect("spawning a real exe should succeed");
}

/// A one-file plan: stage `rel` in a mod under `base`, targeting `base/Data`
fn plan_one(base: &Utf8Path, rel: &str, contents: &str) -> (DeployPlan, Utf8PathBuf) {
    let m = base.join("mods/M");
    write(&m.join(rel), contents);
    let data = base.join("Data");
    let plan = DeployPlan::from_mods(&data, &[ModSource::new("M", &m)]).expect("plan");
    (plan, data)
}

/// A one-file plan turned into a record, with a sibling backup root
fn record_one(base: &Utf8Path, rel: &str, contents: &str) -> (DeployRecord, Utf8PathBuf) {
    let (plan, data) = plan_one(base, rel, contents);
    let record =
        DeployRecord::from_plan(&plan, base.join(".overseer-backup"), DeployerKind::HardLink)
            .expect("record");
    (record, data)
}

#[cfg(windows)]
#[test]
fn volume_id_extracts_lowercase_drive_letter() {
    assert_eq!(volume_id(Utf8Path::new(r"C:\a\b")).as_deref(), Some("c"));
    assert_eq!(volume_id(Utf8Path::new(r"D:\x")).as_deref(), Some("d"));
}

#[test]
fn volume_id_is_none_for_relative_paths() {
    assert_eq!(volume_id(Utf8Path::new("a/b/c")), None);
}

#[cfg(windows)]
#[test]
fn same_volume_true_for_same_drive_regardless_of_case() {
    assert!(same_volume(Utf8Path::new(r"c:\a"), Utf8Path::new(r"C:\b")));
}

#[cfg(windows)]
#[test]
fn same_volume_false_across_drives() {
    assert!(!same_volume(Utf8Path::new(r"C:\a"), Utf8Path::new(r"D:\b")));
}

#[test]
fn same_volume_true_when_undetermined() {
    assert!(same_volume(Utf8Path::new("a/b"), Utf8Path::new("c/d")));
}

#[test]
fn deploy_creates_real_hard_links() {
    let (_tmp, base) = temp();
    let (record, data) = record_one(&base, "Textures/x.dds", "original");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    let dest = data.join("Textures/x.dds");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "original");
    // Hard-link proof: editing the source is visible through the deployed name
    fs::write(&record.entries[0].source, "edited").unwrap();
    assert_eq!(fs::read_to_string(&dest).unwrap(), "edited");
    assert_eq!(record.target_root, data);
    assert_eq!(record.entries.len(), 1);
}

#[test]
fn deploy_mirrors_nested_directories() {
    let (_tmp, base) = temp();
    let (record, data) = record_one(&base, "a/b/c.txt", "deep");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    assert!(data.join("a/b/c.txt").exists());
    assert!(!record.created_dirs.is_empty());
}

#[test]
fn verify_passes_then_reports_missing() {
    let (_tmp, base) = temp();
    let (record, data) = record_one(&base, "x.txt", "x");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    assert!(d.verify(&record).is_ok());
    fs::remove_file(data.join("x.txt")).unwrap();
    let report = d.verify(&record);
    assert!(!report.is_ok());
    assert_eq!(report.expected, 1);
    assert_eq!(report.missing.len(), 1);
}

#[test]
fn undeploy_removes_files_and_created_dirs() {
    let (_tmp, base) = temp();
    let (record, data) = record_one(&base, "sub/x.txt", "x");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    assert!(data.join("sub/x.txt").exists());
    let report = d.undeploy(&record, &NullSink);
    assert!(report.is_fully_resolved());
    assert!(!data.join("sub/x.txt").exists());
    assert!(!data.join("sub").exists(), "created dir removed");
    assert!(data.exists(), "target root preserved");
}

#[test]
fn undeploy_is_idempotent() {
    let (_tmp, base) = temp();
    let (record, _data) = record_one(&base, "x.txt", "x");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    assert!(d.undeploy(&record, &NullSink).is_fully_resolved());
    // Re-running tolerates already-missing files
    assert!(d.undeploy(&record, &NullSink).is_fully_resolved());
}

#[test]
fn undeploy_preserves_foreign_files_in_shared_dirs() {
    let (_tmp, base) = temp();
    let (record, data) = record_one(&base, "sub/mine.txt", "mine");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    // A file we did not create, inside a directory we did
    let foreign = data.join("sub/foreign.txt");
    write(&foreign, "not ours");
    let report = d.undeploy(&record, &NullSink);
    assert!(report.is_fully_resolved());
    assert!(!data.join("sub/mine.txt").exists(), "our file removed");
    assert!(foreign.exists(), "foreign file untouched");
    assert!(data.join("sub").exists(), "non-empty dir left in place");
}

#[test]
fn deploy_leaves_staging_sources_intact() {
    let (_tmp, base) = temp();
    let (record, _data) = record_one(&base, "x.txt", "x");
    let src = record.entries[0].source.clone();
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    d.undeploy(&record, &NullSink);
    assert!(src.exists(), "staging source survives deploy+undeploy");
}

#[test]
fn deploy_writes_winning_content_on_conflict() {
    let (_tmp, base) = temp();
    let a = base.join("mods/A");
    let b = base.join("mods/B");
    write(&a.join("f.txt"), "from-a");
    write(&b.join("f.txt"), "from-b");
    let data = base.join("Data");
    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &a), ModSource::new("B", &b)])
        .expect("plan");
    let record =
        DeployRecord::from_plan(&plan, base.join(".overseer-backup"), DeployerKind::HardLink)
            .expect("record");
    HardlinkDeployer::new()
        .deploy(&record, &NullSink)
        .expect("deploy");
    assert_eq!(fs::read_to_string(data.join("f.txt")).unwrap(), "from-b");
}

#[test]
fn deploy_backs_up_preexisting_file() {
    let (_tmp, base) = temp();
    let (record, data) = record_one(&base, "sub/x.txt", "ours");
    // A real, pre-existing file occupies the destination before deploy
    let dest = data.join("sub/x.txt");
    write(&dest, "preexisting");
    HardlinkDeployer::new()
        .deploy(&record, &NullSink)
        .expect("deploy");
    // The deployed link wins at the destination...
    assert_eq!(fs::read_to_string(&dest).unwrap(), "ours");
    // ...and the original is preserved verbatim under the backup root
    let backup = record.backup_root.join("sub/x.txt");
    assert_eq!(fs::read_to_string(&backup).unwrap(), "preexisting");
}

#[test]
fn undeploy_restores_a_clobbered_preexisting_file() {
    let (_tmp, base) = temp();
    let (record, data) = record_one(&base, "sub/x.txt", "ours");
    let dest = data.join("sub/x.txt");
    write(&dest, "preexisting");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "ours");
    let report = d.undeploy(&record, &NullSink);
    assert!(report.is_fully_resolved());
    // The user's original file is back, byte for byte...
    assert_eq!(fs::read_to_string(&dest).unwrap(), "preexisting");
    // ...and the backup root is swept clean
    assert!(
        !record.backup_root.exists(),
        "backup root removed when empty"
    );
}

#[test]
fn undeploy_is_re_runnable_without_deleting_restored_originals() {
    let (_tmp, base) = temp();
    // Two entries: one with a pre-existing original, one we create ourselves
    let m = base.join("mods/M");
    write(&m.join("kept.txt"), "ours");
    write(&m.join("made.txt"), "made");
    let data = base.join("Data");
    write(&data.join("kept.txt"), "preexisting");
    let plan = DeployPlan::from_mods(&data, &[ModSource::new("M", &m)]).expect("plan");
    let record =
        DeployRecord::from_plan(&plan, base.join(".overseer-backup"), DeployerKind::HardLink)
            .expect("record");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    // First reversal restores the original and removes the created file
    assert!(d.undeploy(&record, &NullSink).is_fully_resolved());
    assert_eq!(
        fs::read_to_string(data.join("kept.txt")).unwrap(),
        "preexisting"
    );
    assert!(!data.join("made.txt").exists());
    // Second reversal (a recovery retry) must be a no-op for the restored; original — it must NOT fall through and delete it
    assert!(d.undeploy(&record, &NullSink).is_fully_resolved());
    assert_eq!(
        fs::read_to_string(data.join("kept.txt")).unwrap(),
        "preexisting",
        "restored original survives a re-run"
    );
    assert!(!data.join("made.txt").exists());
}

#[test]
fn is_our_link_holds_only_for_a_live_link() {
    let (_tmp, base) = temp();
    let source = base.join("mods/M/x.txt");
    write(&source, "content");
    let dest = base.join("Data/x.txt");
    fs::create_dir_all(dest.parent().expect("parent")).expect("mk Data");
    fs::hard_link(&source, &dest).expect("hard link");

    // An intact hard link resolves to the same underlying file as its source
    assert!(is_our_link(&dest, &source));

    // Replacing dest in place (remove + recreate) breaks the link: new file, new identity
    fs::remove_file(&dest).expect("remove");
    write(&dest, "replaced");
    assert!(!is_our_link(&dest, &source));

    // A source that no longer exists can't be proven ours
    assert!(!is_our_link(&dest, &base.join("mods/M/gone.txt")));
}

#[test]
fn undeploy_leaves_a_file_replaced_in_place_after_deploy() {
    let (_tmp, base) = temp();
    let (record, data) = record_one(&base, "sub/x.txt", "ours");
    let dest = data.join("sub/x.txt");
    let d = HardlinkDeployer::new();
    d.deploy(&record, &NullSink).expect("deploy");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "ours");

    // A tool replaces the deployed file in place, breaking our hard link
    fs::remove_file(&dest).expect("remove our link");
    write(&dest, "tool output");

    let report = d.undeploy(&record, &NullSink);
    assert!(report.is_fully_resolved());
    // The replacement is no longer our link, so it is left alone, not deleted
    assert!(dest.exists(), "a file replaced after deploy is preserved");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "tool output");
}

#[test]
fn undeploy_reports_a_residual_backup_file() {
    let (_tmp, base) = temp();
    let (record, _data) = record_one(&base, "x.txt", "ours");
    // A stray file under the backup root that no entry will restore
    write(&record.backup_root.join("stray.txt"), "orphan");
    let report = HardlinkDeployer::new().undeploy(&record, &NullSink);
    assert!(!report.is_fully_resolved());
    assert!(
        report
            .unresolved
            .iter()
            .any(|e| matches!(e, DeployError::ResidualBackup { .. }))
    );
}
