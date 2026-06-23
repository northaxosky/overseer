//! The popup overlays: the help and settings lists.

use ratatui::{
    Frame,
    widgets::{Block, Clear, List, ListItem, ListState, Padding},
};

use super::centered_rect;
use crate::app::App;
use crate::theme;

/// A centered, bordered popup wrapping a selectable list
fn render_list_popup(
    frame: &mut Frame,
    title: &str,
    items: Vec<ListItem<'static>>,
    state: &mut ListState,
    pct_x: u16,
    pct_y: u16,
) {
    let block = Block::bordered()
        .title(format!("  {title}  "))
        .padding(Padding::uniform(1));
    let list = List::new(items)
        .block(block)
        .highlight_symbol("> ")
        .highlight_style(theme::selection_style());
    let area = centered_rect(pct_x, pct_y, frame.area());
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(list, area, state);
}

/// The help popup: a selectable list of keybindings
pub(super) fn render_help(app: &mut App, frame: &mut Frame) {
    let items: Vec<ListItem<'static>> = crate::app::HELP_ENTRIES
        .iter()
        .map(|(keys, desc)| ListItem::new(format!("  {keys:<16}{desc}")))
        .collect();
    render_list_popup(
        frame,
        "Help (Esc: close)",
        items,
        &mut app.help_state,
        70,
        60,
    );
}

/// The settings popup: A selectable list of recent instances to switch to
pub(super) fn render_settings(app: &mut App, frame: &mut Frame) {
    let items: Vec<ListItem<'static>> = app
        .settings
        .recent_instances
        .iter()
        .map(|p| ListItem::new(p.to_string()))
        .collect();
    render_list_popup(
        frame,
        "Settings — recent instances (Enter: switch · Esc: close)",
        items,
        &mut app.settings_state,
        70,
        60,
    );
}
