//! The launcher view popup: Targets to launch

use crate::app::App;
use crate::theme;
use overseer_core::launch;
use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{List, ListItem, Paragraph},
};

/// The launcher view popup: Targets to launch
pub(super) fn render_launcher_body(app: &mut App, frame: &mut Frame, area: Rect) {
    let targets = launch::targets(&app.session.instance);
    if targets.is_empty() {
        let msg = Paragraph::new("No launch targets. Add with `overseer exe add`.")
            .style(theme::style(Role::Warning));
        frame.render_widget(msg, area);
        return;
    }
    let items: Vec<ListItem> = targets.into_iter().map(ListItem::new).collect();
    let list = List::new(items).highlight_style(theme::style(Role::Heading));
    frame.render_stateful_widget(list, area, &mut app.launch_state);
}
