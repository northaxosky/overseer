//! Tests for the BA2 archive breakdown check

use super::*;
use crate::context::{ArchiveInfo, LoadedArchiveCounts};
use crate::finding::Severity;
use overseer_core::archive::{Ba2Header, Ba2Kind};

fn info(name: &str, scan: ArchiveScan) -> ArchiveInfo {
    ArchiveInfo {
        name: name.to_owned(),
        mod_name: "ModA".to_owned(),
        relative: camino::Utf8Path::new("Data").join(name),
        scan,
    }
}

fn header(version: u32, kind: Ba2Kind) -> ArchiveScan {
    ArchiveScan::Header(Ba2Header {
        version,
        kind,
        file_count: 0,
    })
}

fn run(archives: Vec<ArchiveInfo>) -> Vec<Finding> {
    let ctx = GameContext {
        archives,
        ..GameContext::default()
    };
    super::run(&ctx)
}

fn run_counts(loaded_archive_counts: LoadedArchiveCounts) -> Vec<Finding> {
    let ctx = GameContext {
        loaded_archive_counts,
        ..GameContext::default()
    };
    super::run(&ctx)
}

fn limit_findings(findings: &[Finding], label: &str) -> Vec<Finding> {
    findings
        .iter()
        .filter(|f| f.title.starts_with(label))
        .cloned()
        .collect()
}

#[test]
fn no_loaded_archives_warns() {
    let findings = run(vec![]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("No BA2 archives are loaded"));
}

#[test]
fn reports_loaded_counts_and_version_split_as_one_info_line() {
    let findings = run_counts(LoadedArchiveCounts {
        gnrl: 1,
        dx10: 2,
        v1: 2,
        vng: 1,
    });
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
    assert_eq!(
        findings[0].title,
        "1/256 general + 2/255 texture BA2 loaded · versions: 2 v1, 1 v7/8"
    );
}

#[test]
fn over_limit_general_and_texture_are_errors() {
    let general = run_counts(LoadedArchiveCounts {
        gnrl: 257,
        ..LoadedArchiveCounts::default()
    });
    let texture = run_counts(LoadedArchiveCounts {
        dx10: 256,
        ..LoadedArchiveCounts::default()
    });

    let general_limit = limit_findings(&general, "General BA2 archives");
    let texture_limit = limit_findings(&texture, "Texture BA2 archives");

    assert_eq!(general_limit.len(), 1);
    assert_eq!(general_limit[0].severity, Severity::Error);
    assert!(general_limit[0].title.contains("257 / 256"));
    assert_eq!(
        general_limit[0].detail.as_deref(),
        Some(
            "Unpack or merge archives to reduce the total (don't mix texture and non-texture when merging)."
        )
    );

    assert_eq!(texture_limit.len(), 1);
    assert_eq!(texture_limit[0].severity, Severity::Error);
    assert!(texture_limit[0].title.contains("256 / 255"));
}

#[test]
fn exactly_at_limit_and_warn_floor_are_warnings_but_just_under_is_clear() {
    let at_limit = run_counts(LoadedArchiveCounts {
        gnrl: 256,
        ..LoadedArchiveCounts::default()
    });
    let at_floor = run_counts(LoadedArchiveCounts {
        gnrl: 230,
        ..LoadedArchiveCounts::default()
    });
    let just_under = run_counts(LoadedArchiveCounts {
        gnrl: 229,
        ..LoadedArchiveCounts::default()
    });

    let at_limit = limit_findings(&at_limit, "General BA2 archives");
    let at_floor = limit_findings(&at_floor, "General BA2 archives");
    let just_under = limit_findings(&just_under, "General BA2 archives");

    assert_eq!(at_limit.len(), 1);
    assert_eq!(at_limit[0].severity, Severity::Warning);
    assert!(at_limit[0].title.contains("256 / 256"));

    assert_eq!(at_floor.len(), 1);
    assert_eq!(at_floor[0].severity, Severity::Warning);
    assert!(at_floor[0].title.contains("230 / 256"));

    assert!(just_under.is_empty());
}

#[test]
fn an_invalid_archive_warns() {
    let findings = run(vec![info("Bad.ba2", ArchiveScan::Invalid)]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("not a valid BA2"));
}

#[test]
fn an_unreadable_archive_warns_with_the_reason() {
    let findings = run(vec![info(
        "Locked.ba2",
        ArchiveScan::Unreadable("permission denied".to_owned()),
    )]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert_eq!(findings[0].detail.as_deref(), Some("permission denied"));
}

#[test]
fn an_unsupported_version_warns() {
    let findings = run(vec![info("Weird.ba2", header(99, Ba2Kind::General))]);
    assert!(findings.iter().any(
        |f| f.severity == Severity::Warning && f.title.contains("unsupported BA2 version (99)")
    ));
}

#[test]
fn supported_next_gen_versions_are_not_flagged() {
    let findings = run(vec![
        info("v7.ba2", header(7, Ba2Kind::Texture)),
        info("v8.ba2", header(8, Ba2Kind::General)),
    ]);
    assert!(
        !findings
            .iter()
            .any(|f| f.title.contains("unsupported BA2 version"))
    );
}

#[test]
fn starfield_ba2_versions_are_unsupported_for_fallout_4() {
    // v2/v3 are Starfield BA2 versions; Fallout 4 cannot read them, so they must be flagged
    for version in [2, 3] {
        let findings = run(vec![info("sf.ba2", header(version, Ba2Kind::General))]);
        assert!(
            findings
                .iter()
                .any(|f| f.severity == Severity::Warning
                    && f.title.contains("unsupported BA2 version")),
            "version {version} should be unsupported for FO4"
        );
    }
}

#[test]
fn an_other_tag_is_not_counted_in_the_buckets() {
    // A console GNMF archive parses as `Other` — neither general nor texture, and v1 is supported, so it is never flagged
    let findings = run(vec![info(
        "Console.ba2",
        header(1, Ba2Kind::Other(*b"GNMF")),
    )]);
    assert!(!findings.iter().any(|f| f.title.contains("Console.ba2")));
}
