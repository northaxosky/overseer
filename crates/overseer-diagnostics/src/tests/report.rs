//! Tests for the aggregated diagnostics report

use super::*;

/// worst() is the max severity and has_errors() only trips on an Error finding
#[test]
fn worst_is_the_max_severity_and_has_errors_detects_error() {
    // No findings: nothing to report
    assert_eq!(Report::new(vec![]).worst(), None);

    // A warning outranks an info but isn't an error
    let warned = Report::new(vec![Finding::info("ok"), Finding::warning("hmm")]);
    assert_eq!(warned.worst(), Some(Severity::Warning));
    assert!(!warned.has_errors());

    // An error is the worst and trips has_errors
    let errored = Report::new(vec![Finding::warning("hmm"), Finding::error("boom")]);
    assert_eq!(errored.worst(), Some(Severity::Error));
    assert!(errored.has_errors());
}

#[test]
fn counts_groups_findings_by_severity() {
    let report = Report::new(vec![
        Finding::info("one"),
        Finding::info("two"),
        Finding::warning("three"),
        Finding::error("four"),
    ]);

    assert_eq!(
        report.counts(),
        SeverityCounts {
            info: 2,
            warnings: 1,
            errors: 1,
        }
    );
}

#[test]
fn info_only_reports_are_clear() {
    let counts = Report::new(vec![Finding::info("healthy")]).counts();
    assert!(counts.is_clear());
}
