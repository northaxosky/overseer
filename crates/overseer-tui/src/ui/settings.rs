//! The settings popup body: recent instances to switch to.

use ratatui::{Frame, layout::Rect, widgets::ListItem};

use super::render_overlay_list;
use crate::app::App;

/// The settings body: recent instances to switch to
pub(super) fn render_settings_body(app: &mut App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem<'static>> = app
        .settings
        .recent_instances
        .iter()
        .map(|p| ListItem::new(p.to_string()))
        .collect();
    render_overlay_list(frame, area, items, &mut app.settings_state);
}
