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

impl From<overseer_core::plugins::Severity> for Severity {
    fn from(value: overseer_core::plugins::Severity) -> Self {
        match value {
            overseer_core::plugins::Severity::Info => Self::Info,
            overseer_core::plugins::Severity::Warning => Self::Warning,
            overseer_core::plugins::Severity::Error => Self::Error,
        }
    }
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
    /// A finding with an explicit severity and no detail; `diagnose` stamps the check id
    pub fn new(severity: Severity, title: impl Into<String>) -> Self {
        Self {
            check: "",
            severity,
            title: title.into(),
            detail: None,
        }
    }

    /// Info finding: nothing wrong was identified
    pub fn info(title: impl Into<String>) -> Self {
        Self::new(Severity::Info, title)
    }

    /// Warning finding: a potential problem worth attention
    pub fn warning(title: impl Into<String>) -> Self {
        Self::new(Severity::Warning, title)
    }

    /// Error finding: a real problem that might brick the setup
    pub fn error(title: impl Into<String>) -> Self {
        Self::new(Severity::Error, title)
    }

    /// Attach the guidance the UI shows for warnings and errors
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}
