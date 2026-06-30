//! Rendering for [`Modal`](crate::app::Modal) surfaces: a centered, blocking overlay

use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    widgets::{Block, BorderType, Clear, List, ListItem, Padding, Paragraph, Wrap},
};

use super::centered_rect;
use crate::app::{App, Confirm, Modal, Prompt, Select};
use crate::theme;

/// Draw the active modal centered over the main view
pub(super) fn render_modal(app: &mut App, frame: &mut Frame) {
    match app.modal.as_mut() {
        Some(Modal::Select(select)) => render_select(select, frame),
        Some(Modal::Prompt(prompt)) => render_prompt(prompt, frame),
        Some(Modal::Confirm(confirm)) => render_confirm(confirm, frame),
        None => {}
    }
}

fn render_select(select: &mut Select, frame: &mut Frame) {
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
        let msg = Paragraph::new(select.kind.empty_message())
            .style(theme::style(Role::Warning))
            .wrap(Wrap { trim: true });
        frame.render_widget(msg, rows[0]);
    } else {
        let list_items: Vec<ListItem> = select.items.iter().cloned().map(ListItem::new).collect();
        let list = List::new(list_items).highlight_style(theme::style(Role::Heading));
        frame.render_stateful_widget(list, rows[0], &mut select.state);
    }

    let hint = Paragraph::new(format!(
        " Enter {} · Esc close{}",
        select.kind.action_verb(),
        select.kind.extra_hint()
    ))
    .style(theme::style(Role::Muted));
    frame.render_widget(hint, rows[1]);
}

fn render_prompt(prompt: &Prompt, frame: &mut Frame) {
    let area = centered_rect(60, 30, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title(format!("  {}  ", prompt.kind.title()))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // input line
        Constraint::Length(1), // inline error
        Constraint::Fill(1),   // spacer
        Constraint::Length(1), // hint
    ])
    .split(inner);

    let room = (inner.width as usize).saturating_sub(1);
    let line = format!("{}| ", tail(&prompt.input, room));
    frame.render_widget(
        Paragraph::new(line).style(theme::style(Role::Heading)),
        rows[0],
    );

    if let Some(err) = &prompt.error {
        let msg = Paragraph::new(err.clone())
            .style(theme::style(Role::Failure))
            .wrap(Wrap { trim: true });
        frame.render_widget(msg, rows[1]);
    }

    let hint = Paragraph::new(" Enter confirm · Esc cancel ").style(theme::style(Role::Muted));
    frame.render_widget(hint, rows[3]);
}

fn render_confirm(confirm: &Confirm, frame: &mut Frame) {
    let area = centered_rect(60, 30, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title("  Confirm  ")
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Fill(1),   // message
        Constraint::Length(1), // hint
    ])
    .split(inner);

    let msg = Paragraph::new(confirm.message.clone())
        .style(theme::style(Role::Heading))
        .wrap(Wrap { trim: true });
    frame.render_widget(msg, rows[0]);

    let hint = Paragraph::new(" y / N ").style(theme::style(Role::Muted));
    frame.render_widget(hint, rows[1]);
}

/// The last `max` characters of `s` for a tail-window text field
fn tail(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_owned()
    } else {
        s.chars().skip(count - max).collect()
    }
}
