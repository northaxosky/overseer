//! `overseer doctor`: run setup health checks and print a report.

use anyhow::{Context, Result};
use overseer_diagnostics::{Finding, Report, Severity, diagnose};

use crate::cli::ProfileArgs;
use crate::context::open_instance;
use crate::ui::{Role, heading, styled};

pub fn run(target: &ProfileArgs) -> Result<()> {
    let instance = open_instance(&target.instance)?;
    let report = diagnose(&instance, &target.profile)
        .with_context(|| format!("running diagnostics for profile `{}`", target.profile))?;

    heading(format!("Diagnostics: {}", target.profile));
    for finding in &report.findings {
        print_finding(finding);
    }
    println!();
    print_summary(&report);
    Ok(())
}

/// A finding: a severity-coloured marker, the title, and the detail (warnings/errors only).
fn print_finding(finding: &Finding) {
    let (role, glyph) = match finding.severity {
        Severity::Info => (Role::Success, "✓"),
        Severity::Warning => (Role::Warning, "!"),
        Severity::Error => (Role::Failure, "✗"),
    };
    let marker = styled(role, glyph);
    match &finding.detail {
        Some(detail) => println!("  {marker}  {} — {}", finding.title, detail),
        None => println!("  {marker}  {}", finding.title),
    }
}

/// A one-line summary: `No problems found.` or `N warnings, M errors.`
fn print_summary(report: &Report) {
    let count = |s| report.findings.iter().filter(|f| f.severity == s).count();
    let (warnings, errors) = (count(Severity::Warning), count(Severity::Error));

    if warnings == 0 && errors == 0 {
        println!("{}", styled(Role::Success, "No problems found."));
        return;
    }
    let role = if errors > 0 {
        Role::Failure
    } else {
        Role::Warning
    };
    let summary = format!(
        "{}, {}.",
        plural(warnings, "warning"),
        plural(errors, "error")
    );
    println!("{}", styled(role, summary));
}

fn plural(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}
