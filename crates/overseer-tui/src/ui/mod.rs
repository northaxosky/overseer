//! Rendering: turn the [`App`] state into ratatui widgets.
//!
//! View layer only. It reads [`App`] state and mutates just the UI selection
//! state (`ListState`); it never touches domain data.

mod doctor;
mod modal;
mod operation;

use overseer_core::apply::DeploymentStatus;
use overseer_core::deploy::FileConflict;
use overseer_core::instance::ModListEntry;
use overseer_core::plugins::is_master;
use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use strum::IntoEnumIterator;

use crate::app::{
    App, ConflictsStatus, Focus, ModPaneRow, OperationKind, PluginPaneRow, Workspace,
    downloads_sort_label, saves_sort_label, separator_display,
};
use crate::theme;

/// The shared title for the Conflicts workspace pane, scan or message alike
const CONFLICTS_TITLE: &str = " Conflicts — all enabled mods ";

/// Draw the main view, plus any modal floating on top
pub(crate) fn draw(app: &mut App, frame: &mut Frame) {
    draw_main(app, frame);
    if app.modal.is_some() {
        modal::render_modal(app, frame);
    }
}

/// Draw the main UI: header, the two panes, and the status footer
pub(crate) fn draw_main(app: &mut App, frame: &mut Frame) {
    let rows = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Length(1), // workspace switcher
        Constraint::Fill(1),   // body
        Constraint::Length(operation::height(app)),
        Constraint::Length(1), // footer
    ])
    .split(frame.area());

    let header = Line::from(vec![
        Span::styled(" Overseer ", theme::style(Role::Heading)),
        Span::styled(
            format!(" · {} ", app.session.profile.name),
            theme::style(Role::Muted),
        ),
    ]);
    frame.render_widget(Paragraph::new(header), rows[0]);

    // Workspace switcher spans the full width above both panes so the two bordered panes line up
    frame.render_widget(
        workspace_header(
            app.workspace,
            &app.session.profile.name,
            rows[1].width as usize,
        ),
        rows[1],
    );

    // Two columns: the mods pane on the left, the active workspace on the right
    let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(rows[2]);

    let mods_focused = app.focus == Focus::Mods;
    let mods_title = format!(
        " mods — {} ({}) ",
        app.session.profile.name,
        app.session.profile.mods.len()
    );

    let mods_items: Vec<ListItem<'static>> = app
        .mods
        .project(&app.session.profile.mods)
        .into_iter()
        .map(|row| match row {
            ModPaneRow::Separator {
                model_index,
                collapsed,
                member_count,
                ..
            } => {
                let entry = &app.session.profile.mods[model_index];
                let header = separator_header(
                    separator_display(&entry.name),
                    cols[0].width,
                    collapsed,
                    member_count,
                );
                ListItem::new(header).style(theme::style(Role::Heading))
            }
            ModPaneRow::Mod { model_index } => mod_row(&app.session.profile.mods[model_index]),
        })
        .collect();

    render_pane(
        frame,
        cols[0],
        mods_title,
        mods_items,
        app.mods.state_mut(),
        mods_focused,
    );

    // Right pane: draw the active workspace body
    let ws = app.workspace;
    ws.render(app, frame, cols[1]);
    operation::render(app, frame, rows[3]);

    // Status/message on the left, key hints on the right, sharing the footer row
    let (status_text, status_role) = match &app.message {
        Some(n) => (n.text.clone(), n.role),
        None => (status_summary(app.session.status.as_ref()), Role::Muted),
    };
    let status_width = status_text.chars().count();
    let status = Paragraph::new(status_text).style(theme::style(status_role));
    frame.render_widget(status, rows[4]);
    let full_hint = "1–4 workspace · o sort · s instance · d doctor · ? help · q quit ";
    let compact_hint = "1–4 workspace · s instance · d doctor · ? help · q quit ";
    let hint = if status_width + full_hint.chars().count() < rows[4].width as usize {
        full_hint
    } else {
        compact_hint
    };
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Right), rows[4]);
}

impl Workspace {
    /// Draw this workspace's body into `area`
    fn render(self, app: &mut App, frame: &mut Frame, area: Rect) {
        match self {
            Workspace::Plugins => render_plugins(app, frame, area),
            Workspace::Conflicts => render_conflicts(app, frame, area),
            Workspace::Downloads => render_downloads(app, frame, area),
            Workspace::Saves => render_saves(app, frame, area),
        }
    }

    /// The header's scope tag: what this workspace shows (Saves is per-profile)
    fn scope(self, profile: &str) -> String {
        match self {
            Workspace::Plugins => "load order".to_owned(),
            Workspace::Conflicts => "all enabled mods".to_owned(),
            Workspace::Downloads => "archives in downloads/".to_owned(),
            Workspace::Saves => format!("{profile}'s saves"),
        }
    }
}

/// The switcher line compacted to `width`: full labels when they fit, else numbers with the active label kept
fn workspace_header(active: Workspace, profile: &str, width: usize) -> Paragraph<'static> {
    let full = switcher_line(active, profile, true);
    let line = if full.width() <= width {
        full
    } else {
        switcher_line(active, profile, false)
    };
    Paragraph::new(line)
}

/// Build the switcher line; `verbose` shows labels/prefix/scope, compact labels only the active workspace
fn switcher_line(active: Workspace, profile: &str, verbose: bool) -> Line<'static> {
    let role = |on: bool| if on { Role::Heading } else { Role::Muted };
    let mut spans = if verbose {
        vec![
            Span::styled(" Workspace ", theme::style(Role::Heading)),
            Span::styled("· ", theme::style(Role::Muted)),
        ]
    } else {
        vec![Span::raw(" ")]
    };
    for (i, w) in Workspace::iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("| ", theme::style(Role::Muted)));
        }
        // Compact mode keeps only the active workspace's label
        let text = if verbose || w == active {
            format!("{} {} ", w.key(), w.label())
        } else {
            format!("{} ", w.key())
        };
        spans.push(Span::styled(text, theme::style(role(w == active))));
    }
    if verbose {
        spans.push(Span::styled(
            format!("· {}", active.scope(profile)),
            theme::style(Role::Muted),
        ));
    }
    Line::from(spans)
}

/// The plugins workspace: the load order, highlighted when the right pane has focus
fn render_plugins(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    let title = format!(" plugins — {} ", app.session.order.plugins.len());
    let items: Vec<ListItem<'static>> = app
        .plugins
        .project(&app.session.order.plugins, &app.session.plugin_separators)
        .into_iter()
        .map(|row| match row {
            PluginPaneRow::Separator {
                separator_index,
                collapsed,
                member_count,
            } => {
                let sep = &app.session.plugin_separators.items[separator_index];
                let header = separator_header(&sep.name, area.width, collapsed, member_count);
                ListItem::new(header).style(theme::style(Role::Heading))
            }
            PluginPaneRow::Plugin { plugin_index } => {
                let p = &app.session.order.plugins[plugin_index];
                let tag = if is_master(&p.name, &app.session.discovered) {
                    " (master)"
                } else {
                    ""
                };
                let role = if p.active { Role::Success } else { Role::Muted };
                ListItem::new(format!("{} {}{}", marker(p.active), p.name, tag))
                    .style(theme::style(role))
            }
        })
        .collect();
    render_pane(frame, area, title, items, app.plugins.state_mut(), focused);
}

/// The conflicts workspace: a scan result, or a short prompt in every other state
fn render_conflicts(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    let found = match &app.conflicts.status {
        ConflictsStatus::Stale => {
            return render_workspace_message(
                frame,
                area,
                CONFLICTS_TITLE,
                "Press r to scan for conflicts.",
                focused,
            );
        }
        ConflictsStatus::Error(msg) => {
            let text = format!("Conflict scan failed: {msg} — press r to retry.");
            return render_workspace_message(frame, area, CONFLICTS_TITLE, &text, focused);
        }
        ConflictsStatus::Ready(found) if found.is_empty() => {
            return render_workspace_message(
                frame,
                area,
                CONFLICTS_TITLE,
                "No file conflicts among enabled mods.",
                focused,
            );
        }
        // Each row is a priority chain; providers are winner-last, so the rightmost wins
        ConflictsStatus::Ready(found) => found,
    };

    let rows: Vec<ListItem<'static>> = found
        .iter()
        .map(|c| ListItem::new(format!(" {}  ×{}", c.relative, c.providers.len())))
        .collect();
    let selected = app
        .conflicts
        .list
        .index()
        .and_then(|i| found.get(i))
        .unwrap_or(&found[0]);

    let block = pane_block(CONFLICTS_TITLE, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let panes = Layout::vertical([Constraint::Fill(1), Constraint::Length(8)]).split(inner);
    let mut list = List::new(rows);
    if focused {
        list = highlighted(list);
    }
    frame.render_stateful_widget(list, panes[0], app.conflicts.list.state_mut());

    let detail = Paragraph::new(conflict_detail_lines(selected, panes[1].width as usize))
        .wrap(Wrap { trim: false })
        .block(Block::new().borders(Borders::TOP).title(" detail "));
    frame.render_widget(detail, panes[1]);
}

/// The selected conflict's full path, winner, overridden mods, and the winner's staged path
fn conflict_detail_lines(conflict: &FileConflict, width: usize) -> Vec<Line<'static>> {
    let label = "File: ";
    let indent = label.chars().count();
    let text_width = width.saturating_sub(indent).max(1);
    let mut lines: Vec<Line<'static>> = wrap_text(conflict.relative.as_str(), text_width)
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            if i == 0 {
                Line::from(vec![
                    Span::styled(label, theme::style(Role::Heading)),
                    Span::raw(chunk),
                ])
            } else {
                Line::from(format!("{}{chunk}", " ".repeat(indent)))
            }
        })
        .collect();

    // providers are low->high priority: winner last, everyone else loses
    let Some((winner, losers)) = conflict.providers.split_last() else {
        return lines;
    };

    lines.push(Line::from(vec![
        Span::styled("Winner: ", theme::style(Role::Heading)),
        Span::styled(winner.clone(), theme::style(Role::Success)),
    ]));

    lines.push(Line::styled("Overridden:", theme::style(Role::Heading)));
    for loser in losers.iter().rev() {
        lines.push(Line::styled(
            format!("  · {loser}"),
            theme::style(Role::Muted),
        ));
    }

    lines.push(Line::from(vec![
        Span::styled("Staged: ", theme::style(Role::Heading)),
        Span::styled(
            format!("mods/{winner}/{}", conflict.relative),
            theme::style(Role::Muted),
        ),
    ]));
    lines
}

/// The downloads workspace: installable archives, or a hint to drop files in
fn render_downloads(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    let title = downloads_title(app);

    if app.downloads.entries.is_empty() {
        let text = if app
            .operation
            .is_running_kind(OperationKind::RefreshDownloads)
        {
            "Refreshing downloads…".to_owned()
        } else {
            format!(
                "No archives. Drop .7z/.zip files in {}.",
                app.session.instance.downloads_dir()
            )
        };

        return render_workspace_message(frame, area, &title, &text, focused);
    }
    let rows: Vec<ListItem<'static>> = app
        .downloads
        .entries
        .iter()
        .map(|e| {
            // Installed archives are muted with a suffix, like inactive rows elsewhere
            if e.installed {
                ListItem::new(format!("{} (installed)", e.name)).style(theme::style(Role::Muted))
            } else {
                ListItem::new(e.name.clone())
            }
        })
        .collect();
    render_pane(
        frame,
        area,
        title,
        rows,
        app.downloads.list.state_mut(),
        focused,
    );
}

/// The saves workspace: the profile's saves in the active sort order, or an empty-folder note
fn render_saves(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    let title = saves_title(app);
    if app.saves.entries.is_empty() {
        return render_workspace_message(
            frame,
            area,
            &title,
            "No saves in this profile's folder yet.",
            focused,
        );
    }
    let rows: Vec<ListItem<'static>> = app
        .saves
        .entries
        .iter()
        .map(|s| match &s.meta {
            // A parsed save reads as its character/level/location/in-game date
            Some(m) => ListItem::new(format!(
                "{}  ·  L{}  ·  {}  ·  {}",
                m.character, m.level, m.location, m.game_date
            )),
            // An unparsable save still lists, muted, as its bare file name
            None => ListItem::new(s.file_name.clone()).style(theme::style(Role::Muted)),
        })
        .collect();
    render_pane(
        frame,
        area,
        title,
        rows,
        app.saves.list.state_mut(),
        focused,
    );
}

fn downloads_title(app: &App) -> String {
    format!(
        " Downloads — downloads/ · {} ",
        downloads_sort_label(app.settings.downloads_sort)
    )
}

fn saves_title(app: &App) -> String {
    format!(
        " Saves — {} · {} ",
        app.session.profile.name,
        saves_sort_label(app.settings.saves_sort)
    )
}

/// A short, centered message inside a workspace pane frame (stale / error / empty)
fn render_workspace_message(frame: &mut Frame, area: Rect, title: &str, msg: &str, focused: bool) {
    let block = pane_block(title.to_owned(), focused);
    frame.render_widget(
        Paragraph::new(msg.to_owned())
            .block(block)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center),
        area,
    );
}

/// A `Rect` centered in `area`, `pct_x`% wide and `pct_y`% tall
pub(super) fn centered_rect(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let rows = Layout::vertical([
        Constraint::Percentage((100 - pct_y) / 2),
        Constraint::Percentage(pct_y),
        Constraint::Percentage((100 - pct_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - pct_x) / 2),
        Constraint::Percentage(pct_x),
        Constraint::Percentage((100 - pct_x) / 2),
    ])
    .split(rows[1])[1]
}

/// A `Rect` centered in `area`, `pct_x`% wide and a fixed `lines` tall (clamped to `area`)
pub(super) fn centered_rect_lines(pct_x: u16, lines: u16, area: Rect) -> Rect {
    let lines = lines.min(area.height);
    let top = area.height.saturating_sub(lines) / 2;
    let rows = Layout::vertical([
        Constraint::Length(top),
        Constraint::Length(lines),
        Constraint::Min(0),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - pct_x) / 2),
        Constraint::Percentage(pct_x),
        Constraint::Percentage((100 - pct_x) / 2),
    ])
    .split(rows[1])[1]
}

/// The enabled/active checkbox marker
fn marker(on: bool) -> &'static str {
    if on { "[x]" } else { "[ ]" }
}

/// A mod-list row: a separator renders as a header rule, every other kind as a checkbox + name
fn mod_row(m: &ModListEntry) -> ListItem<'static> {
    let role = if m.enabled {
        Role::Success
    } else {
        Role::Muted
    };
    ListItem::new(format!("{} {}", marker(m.enabled), m.name)).style(theme::style(role))
}

/// A separator header filling the pane's inner width: `▼ Name ───` expanded, `▶ Name (n) ───` collapsed
fn separator_header(display: &str, width: u16, collapsed: bool, members: usize) -> String {
    let head = if collapsed {
        format!("▶ {display} ({members}) ")
    } else {
        format!("▼ {display} ")
    };
    let inner = (width as usize).saturating_sub(2);
    let fill = inner.saturating_sub(head.chars().count());
    format!("{head}{}", "─".repeat(fill))
}

/// One-line summary of the instance's live deployment, for the footer
fn status_summary(status: Option<&DeploymentStatus>) -> String {
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

/// A modal frame: a double-bordered box titled `title`; callers add padding as needed
pub(super) fn modal_block(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Double)
        .title(title)
}

/// Main-view pane frame: a thick border when focused, plain otherwise
pub(super) fn pane_block(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .title(title)
}

/// Apply the shared selection cursor (`> ` marker + selection style) to a list
pub(super) fn highlighted(list: List<'_>) -> List<'_> {
    list.highlight_symbol("> ")
        .highlight_style(theme::selection_style())
}

/// Render one selectable list pane
fn render_pane(
    frame: &mut Frame,
    area: Rect,
    title: String,
    items: Vec<ListItem<'static>>,
    state: &mut ListState,
    focused: bool,
) {
    let block = pane_block(title, focused);
    let mut list = List::new(items).block(block);
    if focused {
        list = highlighted(list);
    }
    frame.render_stateful_widget(list, area, state);
}

/// Render a selectable list filling `area`, highlighting the current row
fn render_overlay_list(
    frame: &mut Frame,
    area: Rect,
    items: Vec<ListItem<'static>>,
    state: &mut ListState,
) {
    let list = highlighted(List::new(items));
    frame.render_stateful_widget(list, area, state);
}

/// Greedily wrap `text` to `width` columns, hard-splitting any word too long to fit
pub(super) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if word.chars().count() > width {
            // A word longer than the line: flush what we have, then hard-split it
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            for ch in word.chars() {
                if current.chars().count() == width {
                    lines.push(std::mem::take(&mut current));
                }
                current.push(ch);
            }
            continue;
        }
        let sep = usize::from(!current.is_empty());
        if current.chars().count() + sep + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        } else if sep == 1 {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
#[path = "tests/ui.rs"]
mod tests;
