//! The help popup body: a selectable list of keybindings.

use ratatui::{Frame, layout::Rect, widgets::ListItem};

use super::render_overlay_list;
use crate::app::App;

/// The help body: a selectable list of keybindings
pub(super) fn render_help_body(app: &mut App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem<'static>> = crate::app::HELP_ENTRIES
        .iter()
        .map(|(keys, desc)| ListItem::new(format!("  {keys:<16}{desc}")))
        .collect();
    render_overlay_list(frame, area, items, &mut app.help_state);
}
