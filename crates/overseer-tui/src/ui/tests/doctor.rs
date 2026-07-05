//! Tests for the Doctor modal's detail selection

use super::*;
use overseer_diagnostics::{Finding, Report, Severity};

fn finding(detail: Option<&str>) -> Finding {
    Finding {
        check: "x",
        severity: Severity::Warning,
        title: "Something".to_owned(),
        detail: detail.map(str::to_owned),
    }
}

#[test]
fn detail_falls_back_when_a_finding_has_no_detail() {
    let report = Report::new(vec![finding(None)]);
    assert_eq!(selected_detail(&report, Some(0)), "No further detail.");
}

#[test]
fn detail_prefers_the_findings_own_text() {
    let report = Report::new(vec![finding(Some("Fix it"))]);
    assert_eq!(selected_detail(&report, Some(0)), "Fix it");
}
