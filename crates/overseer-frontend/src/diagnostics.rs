//! Shared presentation for diagnostic findings

use overseer_diagnostics::Severity;

use crate::style::Role;

/// Visual presentation for one diagnostic severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeverityPresentation {
    pub role: Role,
    pub glyph: &'static str,
}

/// Shared visual presentation for a diagnostic severity
pub const fn severity_presentation(severity: Severity) -> SeverityPresentation {
    match severity {
        Severity::Info => SeverityPresentation {
            role: Role::Success,
            glyph: "✓",
        },
        Severity::Warning => SeverityPresentation {
            role: Role::Warning,
            glyph: "!",
        },
        Severity::Error => SeverityPresentation {
            role: Role::Failure,
            glyph: "✗",
        },
    }
}

#[cfg(test)]
#[path = "tests/diagnostics.rs"]
mod tests;
