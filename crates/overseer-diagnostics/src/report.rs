//! The aggregated result of a diagnostics run

use crate::finding::{Finding, Severity};

/// Finding totals grouped by severity
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SeverityCounts {
    pub info: usize,
    pub warnings: usize,
    pub errors: usize,
}

impl SeverityCounts {
    /// Whether the report has no warnings or errors
    pub const fn is_clear(self) -> bool {
        self.warnings == 0 && self.errors == 0
    }
}

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

    /// Count findings by severity
    pub fn counts(&self) -> SeverityCounts {
        let mut counts = SeverityCounts::default();

        for finding in &self.findings {
            match finding.severity {
                Severity::Info => counts.info += 1,
                Severity::Warning => counts.warnings += 1,
                Severity::Error => counts.errors += 1,
            }
        }
        counts
    }
}

#[cfg(test)]
#[path = "tests/report.rs"]
mod tests;
