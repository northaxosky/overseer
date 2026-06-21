//! Rendering: turn the [`App`] state into ratatui widgets.
//!
//! View layer only. It reads [`App`] state and mutates just the UI selection
//! state (`ListState`); it never touches domain data.

use overseer_core::apply::DeploymentStatus;
use overseer_core::plugins::PluginMeta;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style, Stylize},
    text::Line,
    widgets::{Block, BorderType, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, Focus};

/// Draw the whole UI: header, the two panes, and the status footer.
pub(crate) fn draw(app: &mut App, frame: &mut Frame) {
    let rows = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Fill(1),   // body
        Constraint::Length(1), // footer
    ])
    .split(frame.area());

    let header = Line::from(vec![
        " Overseer ".bold(),
        format!(" · {} ", app.profile.name).dim(),
    ]);
    frame.render_widget(Paragraph::new(header), rows[0]);

    let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(rows[1]);

    let mods_focused = app.focus == Focus::Mods;
    let mods_title = format!(" mods — {} ({}) ", app.profile.name, app.profile.mods.len());
    let mods_items: Vec<ListItem<'static>> = app
        .profile
        .mods
        .iter()
        .map(|m| ListItem::new(format!("{} {}", marker(m.enabled), m.name)))
        .collect();
    render_pane(
        frame,
        cols[0],
        mods_title,
        mods_items,
        &mut app.mods_state,
        mods_focused,
    );

    let plugins_focused = app.focus == Focus::Plugins;
    let plugins_title = format!(" plugins — {} ", app.order.plugins.len());
    let plugins_items: Vec<ListItem<'static>> = app
        .order
        .plugins
        .iter()
        .map(|p| {
            let tag = if is_master(&app.discovered, &p.name) {
                " (master)"
            } else {
                ""
            };
            ListItem::new(format!("{} {}{}", marker(p.active), p.name, tag))
        })
        .collect();
    render_pane(
        frame,
        cols[1],
        plugins_title,
        plugins_items,
        &mut app.plugins_state,
        plugins_focused,
    );

    let foot = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(rows[2]);
    let left = app
        .message
        .clone()
        .unwrap_or_else(|| status_summary(&app.status));
    frame.render_widget(Paragraph::new(left), foot[0]);
    frame.render_widget(
        Paragraph::new(" Space toggle · Tab switch · q quit ").alignment(Alignment::Right),
        foot[1],
    );
}

/// The enabled/active checkbox marker.
fn marker(on: bool) -> &'static str {
    if on { "[x]" } else { "[ ]" }
}

/// Whether a plugin name is a master, per discovered metadata.
fn is_master(discovered: &[PluginMeta], name: &str) -> bool {
    discovered
        .iter()
        .any(|m| m.is_master && m.name.eq_ignore_ascii_case(name))
}

/// One-line summary of the instance's live deployment, for the footer.
fn status_summary(status: &Option<DeploymentStatus>) -> String {
    match status {
        None => "No live deployment".to_owned(),
        Some(s) => {
            let files = s.deployment.record.entries.len();
            let health = if s.verified.is_ok() {
                "verified".to_owned()
            } else {
                format!("{} missing", s.verified.missing.len())
            };
            format!(
                "Deployed: {} · {} files · {}",
                s.deployment.profile, files, health
            )
        }
    }
}

/// Render one selectable list pane. Takes the data and its selection state as
/// separate (disjoint) borrows so the borrow checker is happy.
fn render_pane(
    frame: &mut Frame,
    area: Rect,
    title: String,
    items: Vec<ListItem<'static>>,
    state: &mut ListState,
    focused: bool,
) {
    let block = Block::bordered()
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .title(title);
    let mut list = List::new(items).block(block);
    if focused {
        list = list
            .highlight_symbol("> ")
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    }
    frame.render_stateful_widget(list, area, state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test backend");
        terminal.draw(|f| draw(app, f)).expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn footer_shows_deployment_status_and_hints() {
        let mut app = App::sample();
        let out = render(&mut app, 80, 12);
        assert!(
            out.contains("No live deployment"),
            "footer shows deployment status"
        );
        assert!(out.contains("quit"), "footer shows key hints");
    }

    #[test]
    fn footer_prefers_a_message_over_status() {
        let mut app = App::sample();
        app.message = Some("Saved".to_owned());
        let out = render(&mut app, 80, 12);
        assert!(out.contains("Saved"), "footer shows the message");
    }

    #[test]
    fn both_panes_render_their_contents() {
        let mut app = App::sample();
        let out = render(&mut app, 60, 10);
        assert!(out.contains("CoolMod"), "mods pane lists mods");
        assert!(out.contains("Cool.esp"), "plugins pane lists plugins");
        assert!(out.contains("(master)"), "master plugins are tagged");
    }
}
