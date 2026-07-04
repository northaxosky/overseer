//! Rendering: turn the [`App`] state into ratatui widgets.
//!
//! View layer only. It reads [`App`] state and mutates just the UI selection
//! state (`ListState`); it never touches domain data.

mod doctor;
mod modal;

use overseer_core::apply::DeploymentStatus;
use overseer_core::deploy::FileConflict;
use overseer_core::plugins::PluginMeta;
use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use strum::IntoEnumIterator;

use crate::app::{App, ConflictsStatus, Focus, Workspace, downloads_sort_label, saves_sort_label};
use crate::theme;

/// The shared title for the Conflicts workspace pane, scan or message alike.
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

    // Two columns: the mods pane on the left, the active workspace on the right.
    let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(rows[2]);

    let mods_focused = app.focus == Focus::Mods;
    let mods_title = format!(
        " mods — {} ({}) ",
        app.session.profile.name,
        app.session.profile.mods.len()
    );
    let mods_items: Vec<ListItem<'static>> = app
        .session
        .profile
        .mods
        .iter()
        .map(|m| {
            let role = if m.enabled {
                Role::Success
            } else {
                Role::Muted
            };
            ListItem::new(format!("{} {}", marker(m.enabled), m.name)).style(theme::style(role))
        })
        .collect();
    render_pane(
        frame,
        cols[0],
        mods_title,
        mods_items,
        &mut app.mods_state,
        mods_focused,
    );

    render_workspace(app, frame, cols[1]);

    // Status/message on the left, key hints on the right, sharing the footer row.
    let (status_text, status_role) = match &app.message {
        Some(n) => (n.text.clone(), n.role),
        None => (status_summary(app.session.status.as_ref()), Role::Muted),
    };
    let status_width = status_text.chars().count();
    let status = Paragraph::new(status_text).style(theme::style(status_role));
    frame.render_widget(status, rows[3]);
    let full_hint = "1–4 workspace · o sort · s instance · d doctor · ? help · q quit ";
    let compact_hint = "1–4 workspace · s instance · d doctor · ? help · q quit ";
    let hint = if status_width + full_hint.chars().count() < rows[3].width as usize {
        full_hint
    } else {
        compact_hint
    };
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Right), rows[3]);
}

/// Draw the right pane body; `draw_main` draws the switcher full-width so both panes align.
fn render_workspace(app: &mut App, frame: &mut Frame, area: Rect) {
    let ws = app.workspace;
    ws.render(app, frame, area);
}

impl Workspace {
    /// Draw this workspace's body into `area`.
    fn render(self, app: &mut App, frame: &mut Frame, area: Rect) {
        match self {
            Workspace::Plugins => render_plugins(app, frame, area),
            Workspace::Conflicts => render_conflicts(app, frame, area),
            Workspace::Downloads => render_downloads(app, frame, area),
            Workspace::Saves => render_saves(app, frame, area),
        }
    }

    /// The header's scope tag: what this workspace shows (Saves is per-profile).
    fn scope(self, profile: &str) -> String {
        match self {
            Workspace::Plugins => "load order".to_owned(),
            Workspace::Conflicts => "all enabled mods".to_owned(),
            Workspace::Downloads => "archives in downloads/".to_owned(),
            Workspace::Saves => format!("{profile}'s saves"),
        }
    }
}

/// The switcher line compacted to `width`: full labels when they fit, else numbers with the active label kept.
fn workspace_header(active: Workspace, profile: &str, width: usize) -> Paragraph<'static> {
    let full = switcher_line(active, profile, true);
    let line = if full.width() <= width {
        full
    } else {
        switcher_line(active, profile, false)
    };
    Paragraph::new(line)
}

/// Build the switcher line; `verbose` shows labels/prefix/scope, compact labels only the active workspace.
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
        // Compact mode keeps only the active workspace's label.
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

/// The plugins workspace: the load order, highlighted when the right pane has focus.
fn render_plugins(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    let title = format!(" plugins — {} ", app.session.order.plugins.len());
    let items: Vec<ListItem<'static>> = app
        .session
        .order
        .plugins
        .iter()
        .map(|p| {
            let tag = if is_master(&app.session.discovered, &p.name) {
                " (master)"
            } else {
                ""
            };
            let role = if p.active { Role::Success } else { Role::Muted };
            ListItem::new(format!("{} {}{}", marker(p.active), p.name, tag))
                .style(theme::style(role))
        })
        .collect();
    render_pane(frame, area, title, items, &mut app.plugins_state, focused);
}

/// The conflicts workspace: a scan result, or a short prompt in every other state.
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
        // Each row is a priority chain; providers are winner-last, so the rightmost wins.
        ConflictsStatus::Ready(found) => found,
    };

    let rows: Vec<ListItem<'static>> = found
        .iter()
        .map(|c| ListItem::new(format!(" {}  ×{}", c.relative, c.providers.len())))
        .collect();
    let selected = app
        .conflicts
        .list
        .selected()
        .and_then(|i| found.get(i))
        .unwrap_or(&found[0]);

    let block = Block::bordered()
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .title(CONFLICTS_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let panes = Layout::vertical([Constraint::Fill(1), Constraint::Length(8)]).split(inner);
    let mut list = List::new(rows);
    if focused {
        list = list
            .highlight_symbol("> ")
            .highlight_style(theme::selection_style());
    }
    frame.render_stateful_widget(list, panes[0], &mut app.conflicts.list);

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

/// The downloads workspace: installable archives, or a hint to drop files in.
fn render_downloads(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    let title = downloads_title(app);
    if app.downloads.entries.is_empty() {
        let text = format!(
            "No archives. Drop .7z/.zip files in {}.",
            app.session.instance.downloads_dir()
        );
        return render_workspace_message(frame, area, &title, &text, focused);
    }
    let rows: Vec<ListItem<'static>> = app
        .downloads
        .entries
        .iter()
        .map(|e| {
            // Installed archives are muted with a suffix, like inactive rows elsewhere.
            if e.installed {
                ListItem::new(format!("{} (installed)", e.name)).style(theme::style(Role::Muted))
            } else {
                ListItem::new(e.name.clone())
            }
        })
        .collect();
    render_pane(frame, area, title, rows, &mut app.downloads.list, focused);
}

/// The saves workspace: the profile's saves in the active sort order, or an empty-folder note.
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
            // A parsed save reads as its character/level/location/in-game date.
            Some(m) => ListItem::new(format!(
                "{}  ·  L{}  ·  {}  ·  {}",
                m.character, m.level, m.location, m.game_date
            )),
            // An unparsable save still lists, muted, as its bare file name.
            None => ListItem::new(s.file_name.clone()).style(theme::style(Role::Muted)),
        })
        .collect();
    render_pane(frame, area, title, rows, &mut app.saves.list, focused);
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

/// A short, centered message inside a workspace pane frame (stale / error / empty).
fn render_workspace_message(frame: &mut Frame, area: Rect, title: &str, msg: &str, focused: bool) {
    let block = Block::bordered()
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .title(title.to_owned());
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

/// A `Rect` centered in `area`, `pct_x`% wide and a fixed `lines` tall (clamped to `area`).
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

/// Whether a plugin name is a master, per discovered metadata
fn is_master(discovered: &[PluginMeta], name: &str) -> bool {
    discovered
        .iter()
        .any(|m| m.is_master && m.name.eq_ignore_ascii_case(name))
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

/// Render one selectable list pane
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
            .highlight_style(theme::selection_style());
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
    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(theme::selection_style());
    frame.render_stateful_widget(list, area, state);
}

/// Greedily wrap `text` to `width` columns, hard-splitting any word too long to fit.
pub(super) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if word.chars().count() > width {
            // A word longer than the line: flush what we have, then hard-split it.
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

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────
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
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn footer_shows_status_and_help_hint() {
        let mut app = App::sample();
        let out = render(&mut app, 100, 12);
        assert!(out.contains("No live deployment"), "status");
        assert!(out.contains("sort"), "footer offers sorting");
        assert!(out.contains("help"), "footer offers help");
        assert!(out.contains("quit"), "footer offers quit");
    }

    #[test]
    fn help_modal_lists_keybinds_when_open() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Help"), "the modal is titled Help");
        assert!(out.contains("sort"), "the modal lists sort bindings");
        assert!(out.contains("reorder"), "the modal lists bindings");
        assert!(out.contains("Tab"), "the modal shows key columns");
    }

    #[test]
    fn footer_prefers_a_message_over_status() {
        let mut app = App::sample();
        app.ok("Saved");
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

    #[test]
    fn workspace_header_names_every_workspace() {
        let mut app = App::sample();
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("1 Plugins"),
            "header names the plugins workspace"
        );
        assert!(
            out.contains("2 Conflicts"),
            "header names the conflicts workspace"
        );
        assert!(
            out.contains("3 Downloads"),
            "header names the downloads workspace"
        );
        assert!(out.contains("4 Saves"), "header names the saves workspace");
        assert!(
            out.contains('|'),
            "workspaces are separated by a pipe in the switcher"
        );
    }

    #[test]
    fn workspace_header_compacts_on_a_narrow_terminal() {
        // App::sample() defaults to the Plugins workspace.
        let mut app = App::sample();
        let out = render(&mut app, 30, 24);
        assert!(
            out.contains("1 Plugins"),
            "the active workspace keeps its label when compact"
        );
        assert!(
            !out.contains("2 Conflicts"),
            "inactive labels are dropped to fit a narrow terminal"
        );
    }

    #[test]
    fn conflicts_workspace_stale_prompts_to_scan() {
        use crate::app::Workspace;
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Press r"), "a stale scan prompts for r");
        assert!(!out.contains(" detail "), "no split before a scan");
    }

    fn conflict(relative: &str, providers: &[&str]) -> overseer_core::deploy::FileConflict {
        overseer_core::deploy::FileConflict {
            relative: camino::Utf8PathBuf::from(relative),
            providers: providers.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    #[test]
    fn conflicts_workspace_ready_shows_list_row_and_detail() {
        use crate::app::{ConflictsStatus, Workspace};
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        app.conflicts.status =
            ConflictsStatus::Ready(vec![conflict("Textures/shared.dds", &["Low", "High"])]);
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("Textures/shared.dds"),
            "the file path is shown"
        );
        assert!(out.contains("×2"), "the list row shows the provider count");
        assert!(out.contains("Winner: High"), "the detail names the winner");
        assert!(
            out.contains("Staged: mods/High/Textures/shared.dds"),
            "the detail shows the winner's staged path"
        );
        assert!(out.contains("Low"), "the detail names a loser");
        assert!(
            !out.contains("mods/Low/"),
            "a loser's path is never fabricated (core only keeps the winner's casing)"
        );
    }

    #[test]
    fn conflicts_workspace_detail_lists_losers_high_to_low() {
        use crate::app::{ConflictsStatus, Workspace};
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        app.conflicts.status = ConflictsStatus::Ready(vec![conflict(
            "Meshes/shared.nif",
            &["BaseLayer", "MiddleLayer", "TopLayer"],
        )]);
        app.conflicts.list.select(Some(0));
        let out = render(&mut app, 80, 24);
        let middle = out.find("MiddleLayer").expect("nearest loser");
        let base = out.find("BaseLayer").expect("lowest-priority loser");
        assert!(middle < base, "losers render nearest challenger first");
    }

    #[test]
    fn conflicts_workspace_detail_follows_the_selection() {
        use crate::app::{ConflictsStatus, Workspace};
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        app.conflicts.status = ConflictsStatus::Ready(vec![
            conflict("a.dds", &["Lo", "AlphaWinner"]),
            conflict("b.dds", &["Lo", "BetaWinner"]),
        ]);
        app.conflicts.list.select(Some(1));
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("Winner: BetaWinner"),
            "the detail tracks the selected (second) conflict"
        );
        assert!(
            !out.contains("Winner: AlphaWinner"),
            "the unselected conflict's detail is not shown"
        );
    }

    #[test]
    fn conflicts_workspace_detail_wraps_a_narrow_path() {
        use crate::app::{ConflictsStatus, Workspace};
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        app.conflicts.status = ConflictsStatus::Ready(vec![conflict(
            "Textures/VeryLongConflictPathWithUniqueWrapTail.dds",
            &["Low", "High"],
        )]);
        let out = render(&mut app, 50, 24);
        assert!(
            out.contains("niqueWrapTail.dds"),
            "the detail wraps a long path instead of clipping the suffix"
        );
    }

    #[test]
    fn conflicts_workspace_empty_state_stays_a_message() {
        use crate::app::{ConflictsStatus, Workspace};
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        app.conflicts.status = ConflictsStatus::Ready(Vec::new());
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("No file conflicts"),
            "an empty scan still renders its message"
        );
        assert!(
            !out.contains(" detail "),
            "the list/detail split is only used when conflicts exist"
        );
    }

    #[test]
    fn conflicts_workspace_error_state_stays_a_message() {
        use crate::app::{ConflictsStatus, Workspace};
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        app.conflicts.status = ConflictsStatus::Error("boom".to_owned());
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Conflict scan failed: boom"), "error message");
        assert!(
            !out.contains(" detail "),
            "the list/detail split is only used when conflicts exist"
        );
    }

    #[test]
    fn downloads_workspace_lists_archives_and_marks_installed() {
        use crate::app::Workspace;
        use camino::Utf8PathBuf;
        use overseer_core::install::DownloadEntry;
        use std::time::SystemTime;
        let mut app = App::sample();
        app.workspace = Workspace::Downloads;
        app.downloads.entries = vec![
            DownloadEntry {
                name: "Alpha.zip".to_owned(),
                path: Utf8PathBuf::from("downloads/Alpha.zip"),
                installed: false,
                size: 0,
                modified: SystemTime::UNIX_EPOCH,
            },
            DownloadEntry {
                name: "Beta.7z".to_owned(),
                path: Utf8PathBuf::from("downloads/Beta.7z"),
                installed: true,
                size: 0,
                modified: SystemTime::UNIX_EPOCH,
            },
        ];
        app.downloads.list.select(Some(0));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("name ↑"), "the title shows the active sort");
        assert!(
            out.contains("Alpha.zip"),
            "an installable archive is listed"
        );
        assert!(out.contains("Beta.7z"), "every archive is listed");
        assert!(
            out.contains("(installed)"),
            "an installed archive is tagged"
        );
    }

    #[test]
    fn downloads_workspace_empty_state_points_at_the_folder() {
        use crate::app::Workspace;
        let mut app = App::sample();
        app.workspace = Workspace::Downloads;
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("archives"),
            "the empty state explains the pane"
        );
        assert!(out.contains("Drop"), "it tells the user to drop files in");
    }

    #[test]
    fn saves_workspace_lists_parsed_metadata() {
        use crate::app::Workspace;
        use camino::Utf8PathBuf;
        use overseer_core::saves::{SaveInfo, SaveMeta};
        use std::time::SystemTime;
        let mut app = App::sample();
        app.workspace = Workspace::Saves;
        app.saves.entries = vec![SaveInfo {
            path: Utf8PathBuf::from("Saves/Default/Save1.fos"),
            file_name: "Save1.fos".to_owned(),
            modified: SystemTime::UNIX_EPOCH,
            meta: Some(SaveMeta {
                save_number: 1,
                character: "Nora".to_owned(),
                level: 12,
                location: "Sanctuary".to_owned(),
                game_date: "Day 3".to_owned(),
            }),
        }];
        app.saves.list.select(Some(0));
        let out = render(&mut app, 120, 24);
        assert!(out.contains("date ↓"), "the title shows the active sort");
        assert!(out.contains("Nora"), "the character is shown");
        assert!(out.contains("L12"), "the level is shown");
        assert!(out.contains("Sanctuary"), "the location is shown");
        assert!(out.contains("Day 3"), "the in-game date is shown");
    }

    #[test]
    fn saves_workspace_empty_state_explains_the_pane() {
        use crate::app::Workspace;
        let mut app = App::sample();
        app.workspace = Workspace::Saves;
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("No saves"),
            "the empty state explains the pane"
        );
    }

    #[test]
    fn an_unparsed_save_renders_as_its_file_name() {
        use crate::app::Workspace;
        use camino::Utf8PathBuf;
        use overseer_core::saves::SaveInfo;
        use std::time::SystemTime;
        let mut app = App::sample();
        app.workspace = Workspace::Saves;
        app.saves.entries = vec![SaveInfo {
            path: Utf8PathBuf::from("Saves/Default/Broken.fos"),
            file_name: "Broken.fos".to_owned(),
            modified: SystemTime::UNIX_EPOCH,
            meta: None,
        }];
        app.saves.list.select(Some(0));
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("Broken.fos"),
            "an unparsed save shows its file name"
        );
    }

    #[test]
    fn confirm_modal_shows_its_message_and_choices() {
        use crate::app::{Confirm, ConfirmAction, Modal};
        use camino::Utf8PathBuf;
        let mut app = App::sample();
        app.modal = Some(Modal::Confirm(Confirm {
            message: "Install Mod.zip? Creates mods/Mod.".to_owned(),
            action: ConfirmAction::InstallDownload(Utf8PathBuf::from("downloads/Mod.zip")),
        }));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Confirm"), "the modal is titled");
        assert!(out.contains("Install Mod.zip"), "it shows the message");
        assert!(out.contains("y / N"), "it offers the yes/no choice");
    }

    #[test]
    fn instance_picker_lists_recent_instances() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("alpha"), "lists a recent instance");
        assert!(out.contains("switch"), "the hint names the switch action");
    }

    #[test]
    fn doctor_modal_shows_findings_and_summary() {
        use crate::app::{DoctorReport, Modal, initial_selection};
        use overseer_diagnostics::{Finding, Report, Severity};
        let mut app = App::sample();
        app.modal = Some(Modal::Doctor(DoctorReport {
            report: Report::new(vec![Finding {
                check: "x",
                severity: Severity::Error,
                title: "Broken thing".to_owned(),
                detail: Some("Fix it like so.".to_owned()),
            }]),
            list: initial_selection(1),
        }));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Doctor"), "the modal is titled Doctor");
        assert!(
            out.contains("1 error"),
            "summary summarises severity counts"
        );
        assert!(out.contains("Broken thing"), "lists the finding");
        assert!(
            out.contains("Fix it like so."),
            "detail pane shows the selected finding's detail"
        );
        assert!(
            out.contains('║'),
            "the doctor modal is framed with a double border"
        );
    }

    #[test]
    fn doctor_modal_wraps_long_finding_titles() {
        use crate::app::{DoctorReport, Modal, initial_selection};
        use overseer_diagnostics::{Finding, Report, Severity};
        let mut app = App::sample();
        app.modal = Some(Modal::Doctor(DoctorReport {
            report: Report::new(vec![Finding {
                check: "x",
                severity: Severity::Warning,
                title: "This is an exceptionally long finding title that will not fit on one row"
                    .to_owned(),
                detail: None,
            }]),
            list: initial_selection(1),
        }));
        // The trailing word only survives if the title wrapped instead of; clipping at the findings pane's edge.
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("row"),
            "a long finding title wraps to stay readable"
        );
    }

    #[test]
    fn doctor_modal_reports_all_clear_when_empty() {
        use crate::app::{DoctorReport, Modal, initial_selection};
        use overseer_diagnostics::Report;
        let mut app = App::sample();
        app.modal = Some(Modal::Doctor(DoctorReport {
            report: Report::new(vec![]),
            list: initial_selection(0),
        }));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("all clear"), "summary says all clear");
        assert!(
            out.contains("No problems found."),
            "detail pane shows the clean bill"
        );
    }

    #[test]
    fn launch_modal_lists_targets_when_open() {
        use overseer_core::instance::Executable;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::sample();
        app.session.instance.config.executables = vec![Executable {
            name: "FO4Edit".to_owned(),
            path: camino::Utf8PathBuf::from("FO4Edit.exe"),
            args: Vec::new(),
        }];
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("FO4Edit"), "modal lists the launch target");
        assert!(out.contains("Enter launch"), "modal shows the submit hint");
    }

    #[test]
    fn launch_modal_shows_empty_state_with_no_targets() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::sample(); // sample instance configures no exes
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains("No launch targets"),
            "modal shows the empty state"
        );
    }

    #[test]
    fn new_profile_prompt_renders_title_input_and_error() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::sample();
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        for c in ['A', 'b'] {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let out = render(&mut app, 80, 24);
        assert!(out.contains("New profile"), "prompt shows its title");
        assert!(out.contains("Ab"), "prompt echoes the typed input");
        assert!(
            out.contains("Enter confirm"),
            "prompt shows the submit hint"
        );

        // Clearing the input and submitting surfaces the inline validation error.
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("empty"), "the inline validation error renders");
    }
}
