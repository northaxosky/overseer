//! The Doctor workspace: an `r`-gated diagnostics scan with a findings list and
//! a detail pane, mirroring the Conflicts workspace's stale/error/ready states.

use overseer_diagnostics::{Finding, Report, Severity};
use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, ListItem, Paragraph, Wrap},
};

use super::{render_overlay_list, render_workspace_message};
use crate::app::{App, DoctorStatus, Focus};
use crate::theme;

/// The shared title for the Doctor workspace pane, message states alike.
const DOCTOR_TITLE: &str = " Doctor — setup health ";

/// The doctor workspace: diagnostics run on `r`, or a prompt in every other state.
pub(super) fn render_doctor(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    let report = match &app.doctor.status {
        DoctorStatus::Stale => {
            return render_workspace_message(
                frame,
                area,
                DOCTOR_TITLE,
                "Diagnostics stale — press r to run.",
                focused,
            );
        }
        DoctorStatus::Error(msg) => {
            let text = format!("Diagnostics failed: {msg} — press r to retry.");
            return render_workspace_message(frame, area, DOCTOR_TITLE, &text, focused);
        }
        DoctorStatus::Ready(report) => report,
    };

    // Frame the ready view like the other workspace panes; the stale/error states
    // are already framed by render_workspace_message.
    let block = Block::bordered()
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .title(DOCTOR_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // summary
        Constraint::Fill(1),   // findings
        Constraint::Length(7), // detail
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(doctor_summary_line(report, &app.session.profile.name)),
        rows[0],
    );

    // Wrap long titles to the findings pane so nothing clips horizontally
    // (`render_overlay_list` reserves 2 cols for the selection marker).
    let text_width = (rows[1].width as usize).saturating_sub(2);
    let items: Vec<ListItem<'static>> = report
        .findings
        .iter()
        .map(|f| finding_item(f, text_width))
        .collect();
    render_overlay_list(frame, rows[1], items, &mut app.doctor.list);

    let detail = selected_detail(report, app.doctor.list.selected());
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

/// One finding as a styled, width-wrapped list row: a severity-coloured glyph and
/// the title, wrapped across lines so a long title stays fully readable.
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
                // Align continuation lines under the title.
                Line::from(format!("{}{chunk}", " ".repeat(indent)))
            }
        })
        .collect();
    ListItem::new(lines)
}

/// Greedily wrap `text` to `width` columns, hard-splitting any word too long to fit.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if word.chars().count() > width {
            // A word longer than the line: flush what we have, then hard-split it.
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            for ch in word.chars() {
                if current.chars().count() == width {
                    lines.push(std::mem::take(&mut current));
                }
                current.push(ch);
            }
            continue;
        }
        let sep = usize::from(!current.is_empty());
        if current.chars().count() + sep + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        } else if sep == 1 {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
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
