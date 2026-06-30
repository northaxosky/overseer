//! Rendering for [`Modal`](crate::app::Modal) surfaces: a centered, blocking overlay

use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    widgets::{Block, BorderType, Clear, List, ListItem, Padding, Paragraph, Wrap},
};

use super::centered_rect;
use crate::app::{App, Modal};
use crate::theme;

/// Draw the active modal centered over the main view
pub(super) fn render_modal(app: &mut App, frame: &mut Frame) {
    let select = match app.modal.as_mut() {
        Some(Modal::Select(select)) => select,
        None => return,
    };
    let kind = select.kind;

    let area = centered_rect(60, 40, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title("  Overseer  ")
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Fill(1),   // body
        Constraint::Length(1), // hint
    ])
    .split(inner);

    if select.items.is_empty() {
        let msg = Paragraph::new(kind.empty_message())
            .style(theme::style(Role::Warning))
            .wrap(Wrap { trim: true });
        frame.render_widget(msg, rows[0]);
    } else {
        let list_items: Vec<ListItem> = select.items.iter().cloned().map(ListItem::new).collect();
        let list = List::new(list_items).highlight_style(theme::style(Role::Heading));
        frame.render_stateful_widget(list, rows[0], &mut select.state);
    }

    let hint = Paragraph::new(format!(" Enter {} · Esc close ", kind.action_verb()))
        .style(theme::style(Role::Muted));
    frame.render_widget(hint, rows[1]);
}
