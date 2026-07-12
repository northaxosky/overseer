//! Background operation status rendering

use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

use crate::app::{App, OperationState};
use crate::theme;

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
            const SPINNER: [&str; 4] = ["/", "-", "\\", "|"];

            let glyph = SPINNER[running.view.spinner % SPINNER.len()];

            (
                format!(
                    " {glyph} {} · {}…",
                    running.view.kind.label(),
                    running.view.phase.label()
                ),
                Role::Heading,
            )
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
