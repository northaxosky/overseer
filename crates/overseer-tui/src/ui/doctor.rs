//! The diagnostics popup: a findings list with a detail pane for the selection.

use overseer_diagnostics::{Finding, Report, Severity};
use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, ListItem, Paragraph, Wrap},
};

use super::render_overlay_list;
use crate::app::App;
use crate::theme;

/// The doctor body: a severity summary, the findings list, and a detail pane
pub(super) fn render_doctor_body(app: &mut App, frame: &mut Frame, area: Rect) {
    let report = app.report.as_ref();
    let rows = Layout::vertical([
        Constraint::Length(1), // summary
        Constraint::Fill(1),   // findings
        Constraint::Length(7), // detail
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(doctor_summary_line(report, &app.session.profile.name)),
        rows[0],
    );

    let items: Vec<ListItem<'static>> = report
        .map(|r| r.findings.iter().map(finding_item).collect())
        .unwrap_or_default();
    render_overlay_list(frame, rows[1], items, &mut app.doctor_state);

    let detail = selected_detail(report, app.doctor_state.selected());
    let detail_pane = Paragraph::new(detail)
        .wrap(Wrap { trim: true })
        .block(Block::new().borders(Borders::TOP).title(" details "));
    frame.render_widget(detail_pane, rows[2]);
}

/// A one line severity summary for the doctor body's header
fn doctor_summary_line(report: Option<&Report>, profile: &str) -> Line<'static> {
    let Some(report) = report else {
        return Line::raw(format!(" {profile}"));
    };
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

/// One finding as a styled list row: a severity coloured glyph and the title
fn finding_item(finding: &Finding) -> ListItem<'static> {
    let (role, glyph) = severity_style(finding.severity);
    let line = Line::from(vec![
        Span::styled(format!(" {glyph} "), theme::style(role)),
        Span::raw(finding.title.clone()),
    ]);
    ListItem::new(line)
}

/// The detail text for the selected finding
fn selected_detail(report: Option<&Report>, selected: Option<usize>) -> String {
    let Some(report) = report else {
        return String::new();
    };
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

/// The role and glpyh for a severity, matches `overseer doctor`
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
