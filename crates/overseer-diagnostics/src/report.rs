//! The aggregated result of a diagnostics run

use crate::finding::{Finding, Severity};

/// Every finding from a diagnostics run
#[derive(Debug, Clone)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn new(findings: Vec<Finding>) -> Self {
        Self { findings }
    }

    /// The most severe level present, or `None` if there are no findings
    pub fn worst(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    /// Whether any finding is an error
    pub fn has_errors(&self) -> bool {
        self.worst() == Some(Severity::Error)
    }
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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
}
