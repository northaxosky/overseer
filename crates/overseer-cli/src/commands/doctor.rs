//! `overseer doctor`: run setup health checks and print a report.

use anyhow::{Context, Result};
use overseer_diagnostics::{Finding, Report, diagnose};
use overseer_frontend::diagnostics::severity_presentation;

use crate::cli::ProfileArgs;
use crate::ui::{Role, heading, styled};

pub fn run(target: &ProfileArgs) -> Result<()> {
    let (instance, profile) = target.load_profile()?;
    let report = diagnose(&instance, &profile.name)
        .with_context(|| format!("running diagnostics for profile `{}`", profile.name))?;

    heading(format!("Diagnostics: {}", profile.name));
    for finding in &report.findings {
        print_finding(finding);
    }
    println!();
    print_summary(&report);
    Ok(())
}

/// A finding: a severity-coloured marker, the title, and the detail (warnings/errors only)
fn print_finding(finding: &Finding) {
    let presentation = severity_presentation(finding.severity);
    let marker = styled(presentation.role, presentation.glyph);
    match &finding.detail {
        Some(detail) => println!("  {marker}  {} — {}", finding.title, detail),
        None => println!("  {marker}  {}", finding.title),
    }
}

/// A one-line summary: `No problems found.` or `N warnings, M errors.`
fn print_summary(report: &Report) {
    let counts = report.counts();

    if counts.is_clear() {
        println!("{}", styled(Role::Success, "No problems found."));
        return;
    }
    let role = if counts.errors > 0 {
        Role::Failure
    } else {
        Role::Warning
    };
    let summary = format!(
        "{}, {}.",
        plural(counts.warnings, "warning"),
        plural(counts.errors, "error")
    );
    println!("{}", styled(role, summary));
}

fn plural(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}
