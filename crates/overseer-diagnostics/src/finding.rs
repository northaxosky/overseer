//! What a check reports: a finding with a severity

/// How serious a finding is. Ordered so `Error` is worst
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational: nothing wrong
    Info,
    /// Warning: a potential problem worth attention
    Warning,
    /// Error: a real problem that might brick the setup
    Error,
}

/// One thing a check noticed about the setup
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The id of the check that produced this (e.g. `"plugin-count"`)
    pub check: &'static str,
    /// Severity of the finding
    pub severity: Severity,
    /// One-line summary of the finding
    pub title: String,
    /// Longer explanation / guidance, shown by the CLI for warnings and errors
    pub detail: Option<String>,
}

impl Finding {
    /// A finding without a check id; `diagnose` stamps id
    pub fn new(severity: Severity, title: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            check: "",
            severity,
            title: title.into(),
            detail,
        }
    }
}
