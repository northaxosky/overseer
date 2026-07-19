//! Tests for the plan-derived deployment record

use super::*;
use crate::deploy::{ModSource, TargetOwnership, deployer_for};

use crate::test_support::{temp, write};

#[test]
fn vfs_stub_launch_is_unsupported() {
    use crate::deploy::LaunchTarget;
    let target = LaunchTarget {
        program: Utf8PathBuf::from("x.exe"),
        args: vec![],
        working_dir: Utf8PathBuf::from("."),
    };
    for kind in [DeployerKind::Usvfs, DeployerKind::ProjFs] {
        let Err(err) = deployer_for(kind).launch(&target) else {
            panic!("stub launch must be unsupported");
        };
        assert!(
            matches!(err, DeployError::Unsupported { deployer } if deployer == kind),
            "{kind:?} should report its own kind"
        );
    }
}

#[test]
fn verify_report_is_complete_only_when_nothing_missing() {
    let ok = VerifyReport {
        expected: 3,
        missing: vec![],
    };
    assert!(ok.is_complete());
    let bad = VerifyReport {
        expected: 3,
        missing: vec![Utf8PathBuf::from("x.txt")],
    };
    assert!(!bad.is_complete());
}

#[test]
fn reversal_report_is_resolved_only_when_empty() {
    assert!(ReversalReport::default().is_fully_resolved());
    let bad = ReversalReport {
        unresolved: vec![ReversalIssue::new("x", "test")],
        ..ReversalReport::default()
    };
    assert!(!bad.is_fully_resolved());
}

#[test]
fn record_survives_json_round_trip() {
    let record = DeployRecord {
        deployer: DeployerKind::HardLink,
        target_root: Utf8PathBuf::from("C:/Game/Data"),
        backup_root: Utf8PathBuf::from("C:/Game/.overseer-backup"),
        entries: vec![DeployEntry {
            relative: Utf8PathBuf::from("Textures/x.dds"),
            source: Utf8PathBuf::from("C:/mods/M/Textures/x.dds"),
        }],
        created_dirs: vec![Utf8PathBuf::from("Textures")],
    };
    let json = serde_json::to_string(&record).expect("serialize");
    let back: DeployRecord = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.deployer, DeployerKind::HardLink);
    assert_eq!(back.entries, record.entries);
    assert_eq!(back.created_dirs, record.created_dirs);
    assert_eq!(back.backup_root, record.backup_root);
}

#[test]
fn deployer_kind_serializes_as_its_variant_name() {
    let json = serde_json::to_string(&DeployerKind::HardLink).expect("serialize");
    assert_eq!(json, "\"HardLink\"");
}

#[test]
fn factory_builds_a_backend_for_each_kind() {
    assert_eq!(
        deployer_for(DeployerKind::HardLink).kind(),
        DeployerKind::HardLink
    );
    assert_eq!(
        deployer_for(DeployerKind::Usvfs).kind(),
        DeployerKind::Usvfs
    );
}

#[test]
fn stub_classifier_returns_unknown_unsupported() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("a.txt"), "a");
    let plan = DeployPlan::from_mods(base.join("Data"), &[ModSource::new("A", &m)]).expect("plan");
    let record =
        DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::Usvfs).expect("record");

    assert!(matches!(
        deployer_for(DeployerKind::Usvfs).classify(&record, &record.entries[0]),
        TargetOwnership::Unknown(DeployError::Unsupported {
            deployer: DeployerKind::Usvfs
        })
    ));
}

#[test]
fn deployer_kind_display_is_human_readable() {
    assert_eq!(DeployerKind::HardLink.to_string(), "HardLink Deployer");
    assert_eq!(DeployerKind::Usvfs.to_string(), "USVFS Deployer");
}

#[test]
fn from_plan_copies_every_planned_file_as_an_entry() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("a.txt"), "a");
    write(&m.join("sub/b.txt"), "b");
    let data = base.join("Data");
    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    let record = DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::HardLink)
        .expect("record");
    assert_eq!(record.target_root, data);
    assert_eq!(record.entries.len(), 2);
    for entry in &record.entries {
        assert!(entry.source.starts_with(&m));
    }
}

#[test]
fn from_plan_records_only_dirs_that_do_not_yet_exist() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("a/b/c.txt"), "deep");
    let data = base.join("Data");
    std::fs::create_dir_all(data.join("a")).expect("pre-existing dir");
    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    let record = DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::HardLink)
        .expect("record");
    assert_eq!(record.created_dirs, vec![Utf8PathBuf::from("a/b")]);
}

#[test]
fn from_plan_orders_created_dirs_outermost_first_without_duplicates() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("a/b/one.txt"), "1");
    write(&m.join("a/b/two.txt"), "2");
    let data = base.join("Data");
    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    let record = DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::HardLink)
        .expect("record");
    assert_eq!(
        record.created_dirs,
        vec![Utf8PathBuf::from("a"), Utf8Path::new("a").join("b")]
    );
}

#[test]
fn from_plan_records_no_dirs_for_top_level_files() {
    let (_tmp, base) = temp();
    let m = base.join("mods/A");
    write(&m.join("root.txt"), "r");
    let data = base.join("Data");
    let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
    let record = DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::HardLink)
        .expect("record");
    assert!(record.created_dirs.is_empty());
}
