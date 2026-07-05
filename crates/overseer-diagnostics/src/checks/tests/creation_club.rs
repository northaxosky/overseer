//! Tests for the Creation Club manifest check

use super::*;
use crate::finding::Severity;

fn ctx(ccc: CccStatus) -> GameContext {
    GameContext {
        ccc,
        ..GameContext::default()
    }
}

fn present(entries: &[&str]) -> CccStatus {
    CccStatus::Present {
        file: "Fallout4.ccc",
        entries: entries.iter().map(|e| (*e).to_owned()).collect(),
    }
}

#[test]
fn a_game_without_a_manifest_is_silent() {
    assert!(super::run(&ctx(CccStatus::NotApplicable)).is_empty());
}

#[test]
fn a_missing_manifest_warns() {
    let findings = super::run(&ctx(CccStatus::Missing {
        file: "Fallout4.ccc",
    }));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("Fallout4.ccc"));
    assert!(findings[0].title.contains("missing"));
}

#[test]
fn an_unreadable_manifest_warns() {
    let findings = super::run(&ctx(CccStatus::Unreadable {
        file: "Fallout4.ccc",
        error: "access denied".to_owned(),
    }));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("could not be read"));
    assert!(
        findings[0]
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("access denied"))
    );
}

#[test]
fn a_present_manifest_reports_its_count() {
    let findings = super::run(&ctx(present(&["ccA.esl", "ccB.esl"])));
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(findings[0].title.contains("2 Creation Club plugins"));
}

#[test]
fn a_single_entry_is_singular() {
    let findings = super::run(&ctx(present(&["ccA.esl"])));
    assert!(
        findings[0].title.ends_with("Creation Club plugin"),
        "got: {}",
        findings[0].title
    );
}

#[test]
fn an_empty_manifest_reports_zero() {
    let findings = super::run(&ctx(present(&[])));
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(findings[0].title.contains("0 Creation Club plugins"));
}
