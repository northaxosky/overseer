//! Tests for the shared, crash-safe conversion engine

use super::*;
use crate::patch::delta::{DeltaError, RustDeltaDecoder};
use crate::test_support::temp;

const TEST_TARGET: TargetSpec = TargetSpec {
    rel_path: "check.bin",
    expected: ExpectedFingerprint {
        size: 9,
        crc32: 0xCBF4_3926,
        sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
    },
};

const PARTIAL_THEN_TRUNCATED: &[u8] = &[
    0xD6, 0xC3, 0xC4, 0x00, 0x00, 0x00, 0x09, 0x03, 0x00, 0x03, 0x01, 0x00, 0x61, 0x62, 0x63, 0x04,
    0x00,
];

fn item() -> ConvertItem {
    ConvertItem {
        rel_path: "check.bin",
        target: TEST_TARGET,
        group: "test",
    }
}

static SENTINEL_ESM: TargetSpec = TargetSpec {
    rel_path: "Data/sentinel.esm",
    expected: ExpectedFingerprint {
        size: 9,
        crc32: 0xCBF4_3926,
        sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
    },
};

static TEST_GROUPS: &[GroupSpec] = &[
    GroupSpec {
        name: "core",
        ownership: Ownership::Mandatory,
        files: &["check.bin"],
    },
    GroupSpec {
        name: "sentinel",
        ownership: Ownership::Sentinel("Data/sentinel.esm"),
        files: &["Data/sentinel.esm"],
    },
];

fn test_target(rel: &str) -> Option<TargetSpec> {
    match rel {
        "check.bin" => Some(TEST_TARGET),
        "Data/sentinel.esm" => Some(SENTINEL_ESM),
        _ => None,
    }
}

fn test_any_known_size(_: &str, _: u64) -> bool {
    false
}

fn test_known_source(_: &str, _: &FileFingerprint) -> Option<String> {
    None
}

fn test_policy() -> Policy<'static> {
    Policy {
        groups: TEST_GROUPS,
        target_for: &test_target,
        any_known_size: &test_any_known_size,
        known_source: &test_known_source,
    }
}

enum FakeDecoder {
    Writes(Vec<u8>),
    Fails,
}

impl DeltaDecoder for FakeDecoder {
    fn apply(
        &self,
        _source: &Utf8Path,
        _delta: &Utf8Path,
        dest: &Utf8Path,
    ) -> Result<(), DeltaError> {
        match self {
            FakeDecoder::Writes(bytes) => {
                std::fs::write(dest, bytes).unwrap();
                Ok(())
            }
            FakeDecoder::Fails => Err(DeltaError::CreateDestination {
                path: dest.to_owned(),
                source: std::io::Error::other("simulated decode failure"),
            }),
        }
    }
}

fn seed_source(bytes: &[u8]) -> (tempfile::TempDir, Utf8PathBuf) {
    let (tmp, root) = temp();
    std::fs::write(root.join("check.bin"), bytes).unwrap();
    (tmp, root)
}

#[test]
fn classify_recognizes_the_clean_target() {
    let (_tmp, root) = temp();
    std::fs::write(root.join("check.bin"), b"123456789").unwrap();
    assert_eq!(
        classify(&root, item(), &test_policy()).unwrap().state,
        ItemState::AlreadyTarget
    );
}

#[test]
fn classify_flags_different_file() {
    let (_tmp, root) = temp();
    std::fs::write(root.join("check.bin"), b"987654321").unwrap();
    assert_eq!(
        classify(&root, item(), &test_policy()).unwrap().state,
        ItemState::NeedsConversion
    );
}

#[test]
fn classify_reports_a_missing_file() {
    let (_tmp, root) = temp();
    assert_eq!(
        classify(&root, item(), &test_policy()).unwrap().state,
        ItemState::Missing
    );
}

#[test]
fn plan_includes_owned_groups_and_skips_unowned() {
    let (_tmp, root) = temp();
    std::fs::write(root.join("check.bin"), b"987654321").unwrap();
    let plans = plan(&root, &test_policy()).unwrap();
    let has = |g: &str| plans.iter().any(|p| p.item.group == g);
    assert!(has("core"), "mandatory group is always planned");
    assert!(!has("sentinel"), "unowned sentinel group is skipped");
}

#[test]
fn plan_defers_hashing_when_size_rules_out_all_fingerprints() {
    let (_tmp, root) = temp();
    std::fs::create_dir_all(root.join("Data")).unwrap();
    std::fs::write(root.join("Data/sentinel.esm"), b"not the real size").unwrap();
    let plans = plan(&root, &test_policy()).unwrap();
    let esm = plans
        .iter()
        .find(|p| p.item.rel_path == "Data/sentinel.esm")
        .unwrap();
    assert_eq!(esm.state, ItemState::NeedsConversion);
    assert!(esm.current.is_none(), "size-gated preview should not hash");
    assert!(esm.known_source.is_none());
}

#[test]
fn converts_verifies_and_backs_up() {
    let (_tmp, root) = seed_source(b"AE-version");
    let outcome = convert_item(
        &root,
        item(),
        Utf8Path::new("d.vcdiff"),
        &FakeDecoder::Writes(b"123456789".to_vec()),
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Converted);
    assert_eq!(std::fs::read(root.join("check.bin")).unwrap(), b"123456789");
    assert_eq!(
        std::fs::read(root.join("check.bin.overseer-bak")).unwrap(),
        b"AE-version"
    );
}

#[test]
fn wrong_target_hash_leaves_real_file_untouched() {
    let (_tmp, root) = seed_source(b"AE-version");
    let err = convert_item(
        &root,
        item(),
        Utf8Path::new("d.vcdiff"),
        &FakeDecoder::Writes(b"987654321".to_vec()),
    )
    .expect_err("mismatch");
    assert!(matches!(err, ConvertError::TargetMismatch { .. }));
    assert_eq!(
        std::fs::read(root.join("check.bin")).unwrap(),
        b"AE-version"
    );
    assert!(!root.join("check.bin.overseer-tmp").exists());
    assert!(!root.join("check.bin.overseer-bak").exists());
}

#[test]
fn corrupted_delta_cleans_up() {
    let (_tmp, root) = seed_source(b"AE-version");
    let err = convert_item(
        &root,
        item(),
        Utf8Path::new("d.vcdiff"),
        &FakeDecoder::Fails,
    )
    .expect_err("decoder failed");
    assert!(matches!(err, ConvertError::Delta { .. }));
    assert_eq!(
        std::fs::read(root.join("check.bin")).unwrap(),
        b"AE-version"
    );
    assert!(!root.join("check.bin.overseer-tmp").exists());
}

/// Remove real adapter output after a later malformed window fails
#[test]
fn real_decoder_mid_stream_failure_cleans_up() {
    let (_tmp, root) = seed_source(b"AE-version");
    let source = root.join("check.bin");
    let delta = root.join("partial.vcdiff");
    let probe = root.join("partial.bin");
    std::fs::write(&delta, PARTIAL_THEN_TRUNCATED).unwrap();
    let decoder = RustDeltaDecoder::new(TEST_TARGET.expected.size);

    let err = decoder.apply(&source, &delta, &probe).unwrap_err();
    assert!(matches!(err, DeltaError::Decode { .. }));
    assert_eq!(std::fs::read(&probe).unwrap(), b"abc");
    std::fs::remove_file(&probe).unwrap();

    let err = convert_item(&root, item(), &delta, &decoder).unwrap_err();
    assert!(matches!(err, ConvertError::Delta { .. }));
    assert_eq!(std::fs::read(source).unwrap(), b"AE-version");
    assert!(!root.join("check.bin.overseer-tmp").exists());
}

#[test]
fn already_target_is_idempotent() {
    let (_tmp, root) = temp();
    std::fs::write(root.join("check.bin"), b"123456789").unwrap();
    let outcome = convert_item(
        &root,
        item(),
        Utf8Path::new("d.vcdiff"),
        &FakeDecoder::Writes(b"corrupted".to_vec()),
    )
    .unwrap();
    assert_eq!(outcome, Outcome::AlreadyTarget);
    assert!(!root.join("check.bin.overseer-bak").exists());
}

#[test]
fn backup_conflict_refuses_to_clobber() {
    let (_tmp, root) = seed_source(b"AE-version");
    std::fs::write(root.join("check.bin.overseer-bak"), b"other").unwrap();
    let err = convert_item(
        &root,
        item(),
        Utf8Path::new("d.vcdiff"),
        &FakeDecoder::Writes(b"123456789".to_vec()),
    )
    .expect_err("backup conflict");
    assert!(matches!(err, ConvertError::BackupConflict { .. }));
    assert_eq!(
        std::fs::read(root.join("check.bin")).unwrap(),
        b"AE-version"
    );
}

#[test]
fn a_backup_conflict_cleans_up_the_prepared_temp() {
    // Regression: prepare() writes check.bin.overseer-tmp; a guard-phase BackupConflict must not leak it
    let (_tmp, root) = seed_source(b"AE-version");
    std::fs::write(root.join("check.bin.overseer-bak"), b"other").unwrap();
    let err = convert_item(
        &root,
        item(),
        Utf8Path::new("d.vcdiff"),
        &FakeDecoder::Writes(b"123456789".to_vec()),
    )
    .expect_err("backup conflict");
    assert!(matches!(err, ConvertError::BackupConflict { .. }));
    assert!(
        !root.join("check.bin.overseer-tmp").exists(),
        "the prepared temp must be cleaned up when the backup guard fails"
    );
}

struct PerPathDecoder;

impl DeltaDecoder for PerPathDecoder {
    fn apply(
        &self,
        _source: &Utf8Path,
        _delta: &Utf8Path,
        dest: &Utf8Path,
    ) -> Result<(), DeltaError> {
        let bytes: &[u8] = if dest.as_str().contains("good") {
            b"123456789"
        } else {
            b"XXXXXXXXX"
        };
        std::fs::write(dest, bytes).unwrap();
        Ok(())
    }
}

#[test]
fn bad_delta_in_batch_converts_nothing() {
    const GOOD_TARGET: TargetSpec = TargetSpec {
        rel_path: "good.bin",
        expected: ExpectedFingerprint {
            size: 9,
            crc32: 0xCBF4_3926,
            sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
        },
    };
    const BAD_TARGET: TargetSpec = TargetSpec {
        rel_path: "bad.bin",
        expected: ExpectedFingerprint {
            size: 9,
            crc32: 0xCBF4_3926,
            sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
        },
    };
    let (_tmp, root) = temp();
    std::fs::write(root.join("good.bin"), b"OLD-good").unwrap();
    std::fs::write(root.join("bad.bin"), b"OLD-bad").unwrap();
    let jobs = [
        ConvertJob {
            item: ConvertItem {
                rel_path: "good.bin",
                target: GOOD_TARGET,
                group: "test",
            },
            delta: Utf8PathBuf::from("d.vcdiff"),
        },
        ConvertJob {
            item: ConvertItem {
                rel_path: "bad.bin",
                target: BAD_TARGET,
                group: "test",
            },
            delta: Utf8PathBuf::from("d.vcdiff"),
        },
    ];
    let err = convert(&root, &jobs, &PerPathDecoder).expect_err("batch fails");
    assert!(matches!(err, ConvertError::TargetMismatch { .. }));
    assert_eq!(std::fs::read(root.join("good.bin")).unwrap(), b"OLD-good");
    assert_eq!(std::fs::read(root.join("bad.bin")).unwrap(), b"OLD-bad");
}

struct FailInstallOf {
    tmp_suffix: String,
}

impl RenameOp for FailInstallOf {
    fn rename(&mut self, from: &Utf8Path, to: &Utf8Path) -> Result<(), IoError> {
        if from.as_str().ends_with(&self.tmp_suffix) {
            return Err(io_err(to, std::io::Error::other("injected rename failure")));
        }
        rename(from, to)
    }
}

#[test]
fn commit_failure_mid_batch_rolls_back_every_prior_swap() {
    const ONE_TARGET: TargetSpec = TargetSpec {
        rel_path: "one.bin",
        expected: ExpectedFingerprint {
            size: 9,
            crc32: 0xCBF4_3926,
            sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
        },
    };
    const TWO_TARGET: TargetSpec = TargetSpec {
        rel_path: "two.bin",
        expected: ExpectedFingerprint {
            size: 9,
            crc32: 0xCBF4_3926,
            sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
        },
    };
    const THREE_TARGET: TargetSpec = TargetSpec {
        rel_path: "three.bin",
        expected: ExpectedFingerprint {
            size: 9,
            crc32: 0xCBF4_3926,
            sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
        },
    };
    let (_tmp, root) = temp();
    std::fs::write(root.join("one.bin"), b"original1").unwrap();
    std::fs::write(root.join("two.bin"), b"original2").unwrap();
    std::fs::write(root.join("three.bin"), b"original3").unwrap();
    let jobs = [
        ConvertJob {
            item: ConvertItem {
                rel_path: "one.bin",
                target: ONE_TARGET,
                group: "test",
            },
            delta: Utf8PathBuf::from("d.vcdiff"),
        },
        ConvertJob {
            item: ConvertItem {
                rel_path: "two.bin",
                target: TWO_TARGET,
                group: "test",
            },
            delta: Utf8PathBuf::from("d.vcdiff"),
        },
        ConvertJob {
            item: ConvertItem {
                rel_path: "three.bin",
                target: THREE_TARGET,
                group: "test",
            },
            delta: Utf8PathBuf::from("d.vcdiff"),
        },
    ];
    let mut ready = Vec::new();
    for job in &jobs {
        match prepare(&root, job, &FakeDecoder::Writes(b"123456789".to_vec())).unwrap() {
            Prepared::Ready {
                item,
                tmp,
                source,
                verified_by,
            } => ready.push(PreparedReady {
                item,
                tmp,
                source,
                verified_by,
            }),
            _ => panic!("all three jobs should prepare"),
        }
    }
    let mut renamer = FailInstallOf {
        tmp_suffix: "two.bin.overseer-tmp".to_owned(),
    };
    let err = commit_batch_with_renamer(&root, &ready, &mut renamer).expect_err("commit fails");
    assert!(matches!(err, ConvertError::CommitFailed { item, .. } if item == "two.bin"));
    assert_eq!(std::fs::read(root.join("one.bin")).unwrap(), b"original1");
    assert_eq!(std::fs::read(root.join("two.bin")).unwrap(), b"original2");
    assert_eq!(std::fs::read(root.join("three.bin")).unwrap(), b"original3");
    for name in ["one.bin", "two.bin", "three.bin"] {
        assert!(!root.join(format!("{name}.overseer-tmp")).exists());
        assert!(!root.join(format!("{name}.overseer-bak")).exists());
    }
}

/// A source rewritten between prepare and commit (antivirus, Steam, another tool) aborts the swap and rolls back
#[test]
fn a_source_changing_before_commit_rolls_back_and_refuses() {
    const ONE: TargetSpec = TargetSpec {
        rel_path: "one.bin",
        expected: ExpectedFingerprint {
            size: 9,
            crc32: 0xCBF4_3926,
            sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
        },
    };
    const TWO: TargetSpec = TargetSpec {
        rel_path: "two.bin",
        expected: ExpectedFingerprint {
            size: 9,
            crc32: 0xCBF4_3926,
            sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
        },
    };
    let (_tmp, root) = temp();
    std::fs::write(root.join("one.bin"), b"original1").unwrap();
    std::fs::write(root.join("two.bin"), b"original2").unwrap();
    let jobs = [
        ConvertJob {
            item: ConvertItem {
                rel_path: "one.bin",
                target: ONE,
                group: "test",
            },
            delta: Utf8PathBuf::from("d.vcdiff"),
        },
        ConvertJob {
            item: ConvertItem {
                rel_path: "two.bin",
                target: TWO,
                group: "test",
            },
            delta: Utf8PathBuf::from("d.vcdiff"),
        },
    ];
    let mut ready = Vec::new();
    for job in &jobs {
        match prepare(&root, job, &FakeDecoder::Writes(b"123456789".to_vec())).unwrap() {
            Prepared::Ready {
                item,
                tmp,
                source,
                verified_by,
            } => ready.push(PreparedReady {
                item,
                tmp,
                source,
                verified_by,
            }),
            _ => panic!("both jobs should prepare"),
        }
    }
    // A concurrent writer rewrites the second source after it was fingerprinted
    std::fs::write(root.join("two.bin"), b"changed!!").unwrap();

    let err = commit_batch(&root, &ready).expect_err("source changed");
    assert!(matches!(err, ConvertError::SourceChanged { item } if item == "two.bin"));
    // The first swap is rolled back; both originals stand and no sidecars leak
    assert_eq!(std::fs::read(root.join("one.bin")).unwrap(), b"original1");
    assert_eq!(std::fs::read(root.join("two.bin")).unwrap(), b"changed!!");
    for name in ["one.bin", "two.bin"] {
        assert!(!root.join(format!("{name}.overseer-tmp")).exists());
        assert!(!root.join(format!("{name}.overseer-bak")).exists());
    }
}

/// A group with any unknown-target file is skipped whole, so an incomplete edition (e.g. NextGen) plans nothing
#[test]
fn items_skips_a_group_with_an_unrecorded_target() {
    static GROUPS: &[GroupSpec] = &[
        GroupSpec {
            name: "known",
            ownership: Ownership::Mandatory,
            files: &["check.bin"],
        },
        GroupSpec {
            name: "partial",
            ownership: Ownership::Mandatory,
            files: &["check.bin", "unrecorded.bin"],
        },
    ];
    let policy = Policy {
        groups: GROUPS,
        target_for: &test_target,
        any_known_size: &test_any_known_size,
        known_source: &test_known_source,
    };
    let (_tmp, root) = temp();
    std::fs::write(root.join("check.bin"), b"123456789").unwrap();

    let resolved = items(&root, &policy).unwrap();
    let groups: Vec<&str> = resolved.iter().map(|i| i.group).collect();
    assert_eq!(groups, ["known"]);
}

#[test]
fn leftover_temp_is_removed_before_retry() {
    let (_tmp, root) = seed_source(b"AE-version");
    std::fs::write(root.join("check.bin.overseer-tmp"), b"stale").unwrap();
    convert_item(
        &root,
        item(),
        Utf8Path::new("d.vcdiff"),
        &FakeDecoder::Writes(b"123456789".to_vec()),
    )
    .unwrap();
    assert_eq!(std::fs::read(root.join("check.bin")).unwrap(), b"123456789");
}

#[test]
fn recovers_when_real_is_missing_but_backup_survives() {
    let (_tmp, root) = temp();
    std::fs::write(root.join("check.bin.overseer-bak"), b"AE-version").unwrap();
    assert!(!root.join("check.bin").exists());
    let outcome = convert_item(
        &root,
        item(),
        Utf8Path::new("d.vcdiff"),
        &FakeDecoder::Writes(b"123456789".to_vec()),
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Converted);
    assert_eq!(std::fs::read(root.join("check.bin")).unwrap(), b"123456789");
    assert_eq!(
        std::fs::read(root.join("check.bin.overseer-bak")).unwrap(),
        b"AE-version"
    );
}

#[test]
fn recover_install_restores_a_crashed_mandatory_file_before_planning() {
    let (_tmp, root) = temp();
    std::fs::write(root.join("check.bin.overseer-bak"), b"og-bytes").unwrap();
    assert!(!root.join("check.bin").exists());
    recover_install(&root, &test_policy()).unwrap();
    assert_eq!(std::fs::read(root.join("check.bin")).unwrap(), b"og-bytes");
    assert!(!root.join("check.bin.overseer-bak").exists());
}

#[test]
fn recover_install_restores_a_crashed_sentinel_before_planning() {
    // A crashed sentinel in its backup slot makes the group look unowned; recovery must ignore ownership
    let (_tmp, root) = temp();
    std::fs::create_dir_all(root.join("Data")).unwrap();
    std::fs::write(root.join("Data/sentinel.esm.overseer-bak"), b"og-esm-bytes").unwrap();
    assert!(!root.join("Data/sentinel.esm").exists());
    recover_install(&root, &test_policy()).unwrap();
    assert_eq!(
        std::fs::read(root.join("Data/sentinel.esm")).unwrap(),
        b"og-esm-bytes"
    );
    assert!(!root.join("Data/sentinel.esm.overseer-bak").exists());
}
