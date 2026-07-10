//! The Doctor modal: a centered pop-up over the main view showing a diagnostics run
//! as a severity summary, a selectable findings list, and a live detail pane.

use overseer_diagnostics::{Finding, Report, Severity};
use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, ListItem, Paragraph, Wrap},
};

use super::{centered_rect, modal_block, render_overlay_list, wrap_text};
use crate::app::DoctorReport;
use crate::theme;

/// The title on the Doctor modal's frame
const DOCTOR_TITLE: &str = "  Doctor — setup health  ";

/// Draw the Doctor modal as a larger centered box with severity summary, findings list, and live detail pane
pub(super) fn render_doctor_modal(doctor: &mut DoctorReport, profile: &str, frame: &mut Frame) {
    let area = centered_rect(75, 75, frame.area());
    frame.render_widget(Clear, area);
    let block = modal_block(DOCTOR_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // summary
        Constraint::Fill(1),   // findings
        Constraint::Length(7), // detail
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(doctor_summary_line(&doctor.report, profile)),
        rows[0],
    );

    // Wrap long titles to the findings pane so nothing clips horizontally (`render_overlay_list` reserves 2 cols for the selection marker)
    let text_width = (rows[1].width as usize).saturating_sub(2);
    let items: Vec<ListItem<'static>> = doctor
        .report
        .findings
        .iter()
        .map(|f| finding_item(f, text_width))
        .collect();
    render_overlay_list(frame, rows[1], items, doctor.list.state_mut());

    let detail = selected_detail(&doctor.report, doctor.list.index());
    let detail_pane = Paragraph::new(detail)
        .wrap(Wrap { trim: true })
        .block(Block::new().borders(Borders::TOP).title(" details "));
    frame.render_widget(detail_pane, rows[2]);
}

/// A one line severity summary for the doctor body's header
fn doctor_summary_line(report: &Report, profile: &str) -> Line<'static> {
    let count = |s| report.findings.iter().filter(|f| f.severity == s).count();
    let (warnings, errors) = (count(Severity::Warning), count(Severity::Error));
    if warnings == 0 && errors == 0 {
        return Line::styled(
            format!(" {profile} · all clear"),
            theme::style(Role::Success),
        );
    }
    let role = if errors > 0 {
        Role::Failure
    } else {
        Role::Warning
    };
    Line::styled(
        format!(
            " {profile} · {}, {}",
            plural(warnings, "warning"),
            plural(errors, "error")
        ),
        theme::style(role),
    )
}

/// One finding as a styled, width-wrapped row with a severity-coloured glyph and full title
fn finding_item(finding: &Finding, width: usize) -> ListItem<'static> {
    let (role, glyph) = severity_style(finding.severity);
    let prefix = format!(" {glyph} ");
    let indent = prefix.chars().count();
    let lines: Vec<Line<'static>> = wrap_text(&finding.title, width.saturating_sub(indent).max(1))
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            if i == 0 {
                Line::from(vec![
                    Span::styled(prefix.clone(), theme::style(role)),
                    Span::raw(chunk),
                ])
            } else {
                // Align continuation lines under the title
                Line::from(format!("{}{chunk}", " ".repeat(indent)))
            }
        })
        .collect();
    ListItem::new(lines)
}

/// The detail text for the selected finding
fn selected_detail(report: &Report, selected: Option<usize>) -> String {
    if report.findings.is_empty() {
        return "No problems found.".to_owned();
    }
    match selected.and_then(|i| report.findings.get(i)) {
        Some(f) => f
            .detail
            .clone()
            .unwrap_or_else(|| "No further detail.".to_owned()),
        None => String::new(),
    }
}

/// The role and glyph for a severity, matches `overseer doctor`
fn severity_style(severity: Severity) -> (Role, &'static str) {
    match severity {
        Severity::Info => (Role::Success, "✓"),
        Severity::Warning => (Role::Warning, "!"),
        Severity::Error => (Role::Failure, "✗"),
    }
}

/// `N noun(s)` with naive pluralisation
fn plural(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
#[path = "tests/doctor.rs"]
mod tests;
