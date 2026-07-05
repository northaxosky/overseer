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

#[cfg(test)]
#[path = "tests/report.rs"]
mod tests;
