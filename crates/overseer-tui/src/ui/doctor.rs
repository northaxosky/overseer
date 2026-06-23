//! The diagnostics popup: a findings list with a detail pane for the selection.

use overseer_diagnostics::{Finding, Report, Severity};
use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use super::centered_rect;
use crate::app::App;
use crate::theme;

/// The diagnostics popup: a scrollable findings list with a detail pane for the selection
pub(super) fn render_doctor(app: &mut App, frame: &mut Frame) {
    let area = centered_rect(80, 70, frame.area());
    frame.render_widget(Clear, area);

    let report = app.report.as_ref();
    let block = Block::bordered().title(format!(
        "  {}  ",
        doctor_title(report, &app.session.profile.name)
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Fill(1), Constraint::Length(7)]).split(inner);

    let items: Vec<ListItem<'static>> = report
        .map(|r| r.findings.iter().map(finding_item).collect())
        .unwrap_or_default();
    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(theme::selection_style());
    frame.render_stateful_widget(list, rows[0], &mut app.doctor_state);

    let detail = selected_detail(report, app.doctor_state.selected());
    let detail_pane = Paragraph::new(detail)
        .wrap(Wrap { trim: true })
        .block(Block::new().borders(Borders::TOP).title(" detail "));
    frame.render_widget(detail_pane, rows[1]);
}

/// The doctor popup's border title: the profile plus a one-line severity summary
fn doctor_title(report: Option<&Report>, profile: &str) -> String {
    let Some(report) = report else {
        return format!("Diagnostics — {profile}");
    };
    let count = |s| report.findings.iter().filter(|f| f.severity == s).count();
    let (warnings, errors) = (count(Severity::Warning), count(Severity::Error));
    if warnings == 0 && errors == 0 {
        return format!("Diagnostics — {profile} · all clear");
    }
    format!(
        "Diagnostics — {profile} · {}, {}",
        plural(warnings, "warning"),
        plural(errors, "error")
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
