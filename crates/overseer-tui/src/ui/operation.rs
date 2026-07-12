//! Background operation status rendering

use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

use crate::app::{App, OperationProgress, OperationState};
use crate::theme;

#[derive(Clone, Copy)]
enum EllipsisPosition {
    Leading,
    Trailing,
}

/// Return the rows required by the current operation state
pub(super) fn height(app: &App) -> u16 {
    match &app.operation {
        OperationState::Idle => 0,
        OperationState::Running(_) => 1,
        OperationState::Completed(_) => 2,
    }
}

/// Render the running or completed operation state
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let (text, role) = match &app.operation {
        OperationState::Idle => return,
        OperationState::Running(running) => {
            let text = match &running.view.progress {
                Some(progress) => {
                    progress_line(progress, running.view.phase.label(), area.width as usize)
                }
                None => {
                    const SPINNER: [&str; 4] = ["/", "-", "\\", "|"];
                    let glyph = SPINNER[running.view.spinner % SPINNER.len()];
                    format!(
                        " {glyph} {} · {}…",
                        running.view.kind.label(),
                        running.view.phase.label()
                    )
                }
            };
            (text, Role::Heading)
        }
        OperationState::Completed(completed) => {
            let (prefix, role) = if completed.succeeded {
                ("✓ COMPLETE", Role::Success)
            } else {
                ("✕ FAILED", Role::Failure)
            };

            (
                format!(
                    " {prefix} · {} · {}",
                    completed.kind.label(),
                    completed.message
                ),
                role,
            )
        }
    };

    frame.render_widget(
        Paragraph::new(text)
            .style(theme::style(role))
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// Build a one-row determinate line that gives long paths tail priority
fn progress_line(progress: &OperationProgress, phase: &str, width: usize) -> String {
    let count = format!("{}/{}", progress.completed, progress.total);
    let path = progress.current.as_ref().map(|path| path.as_str());
    let overhead = if path.is_some() { 10 } else { 7 } + count.chars().count();
    let available = width.saturating_sub(overhead);
    let path_width = path
        .map(|path| path.chars().count().min(available / 2))
        .unwrap_or(0);
    let remaining = available.saturating_sub(path_width);
    let bar_width = remaining.div_ceil(3).clamp(1, 12);
    let phase_width = remaining.saturating_sub(bar_width);
    let bar = progress_bar(progress.fraction(), bar_width);
    let phase = ellipsize(phase, phase_width, EllipsisPosition::Trailing);
    match path {
        Some(path) => format!(
            " [{bar}] {count} · {phase} · {}",
            ellipsize(path, path_width, EllipsisPosition::Leading)
        ),
        None => format!(" [{bar}] {count} · {phase}"),
    }
}

/// Render a fixed-width ASCII progress bar from a clamped fraction
fn progress_bar(fraction: f64, width: usize) -> String {
    let filled = (fraction.clamp(0.0, 1.0) * width as f64).round() as usize;
    let filled = filled.min(width);

    format!("{}{}", "#".repeat(filled), "-".repeat(width - filled))
}

/// Truncate text with an explicit leading or trailing ellipsis
fn ellipsize(text: &str, width: usize, position: EllipsisPosition) -> String {
    let len = text.chars().count();
    if len <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_owned();
    }

    match position {
        EllipsisPosition::Leading => format!(
            "…{}",
            text.chars().skip(len - width + 1).collect::<String>()
        ),
        EllipsisPosition::Trailing => {
            format!("{}…", text.chars().take(width - 1).collect::<String>())
        }
    }
}

#[cfg(test)]
#[path = "tests/operation.rs"]
mod tests;
