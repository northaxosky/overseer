//! The tabbed overlay frame: the bordered box and the tab bar.

use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, BorderType, Clear, Padding, Paragraph, Tabs},
};

use super::{centered_rect, doctor, help, settings};
use crate::app::{App, Popup};
use crate::theme;

/// Draw the tabbed overlay: a bordered frame with a tab bar and the active tab's body
pub(super) fn render_overlay(app: &mut App, tab: Popup, frame: &mut Frame) {
    let area = centered_rect(80, 70, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title("  Overseer  ")
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // tab bar
        Constraint::Length(1), // spacer
        Constraint::Fill(1),   // body
        Constraint::Length(1), // hint
    ])
    .split(inner);

    match tab {
        Popup::Help => help::render_help_body(app, frame, rows[2]),
        Popup::Settings => settings::render_settings_body(app, frame, rows[2]),
        Popup::Doctor => doctor::render_doctor_body(app, frame, rows[2]),
    }
    render_tab_bar(tab, frame, rows[0]);

    let hint =
        Paragraph::new(" Tab / Shift+Tab  switch · Esc  close ").style(theme::style(Role::Muted));
    frame.render_widget(hint, rows[3]);
}

/// The tab bar across the top of the overlay, highlighting the active tab
fn render_tab_bar(active: Popup, frame: &mut Frame, area: Rect) {
    let labels: Vec<&'static str> = Popup::TABS.iter().map(|t| t.label()).collect();
    let tabs = Tabs::new(labels)
        .select(active.index())
        .style(theme::style(Role::Muted))
        .highlight_style(theme::style(Role::Heading))
        .divider("·");
    frame.render_widget(tabs, area);
}
