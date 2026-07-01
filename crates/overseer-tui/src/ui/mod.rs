//! Rendering: turn the [`App`] state into ratatui widgets.
//!
//! View layer only. It reads [`App`] state and mutates just the UI selection
//! state (`ListState`); it never touches domain data.

mod doctor;
mod help;
mod modal;
mod overlay;
mod settings;

use overseer_core::apply::DeploymentStatus;
use overseer_core::plugins::PluginMeta;
use overseer_frontend::style::Role;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, ConflictsStatus, Focus, Workspace};
use crate::theme;

/// The shared title for the Conflicts workspace pane, scan or message alike.
const CONFLICTS_TITLE: &str = " Conflicts — all enabled mods ";

/// The shared title for the Downloads workspace pane, list or message alike.
const DOWNLOADS_TITLE: &str = " Downloads — archives in downloads/ ";

/// Draw the main view, plus any popup floating on top
pub(crate) fn draw(app: &mut App, frame: &mut Frame) {
    draw_main(app, frame);
    if let Some(tab) = app.popup {
        overlay::render_overlay(app, tab, frame);
    }
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

    // Workspace switcher spans the full width above both panes so the two bordered
    // panes line up (it used to sit inside the right column, offsetting it by a row).
    frame.render_widget(
        workspace_header(app.workspace, &app.session.profile.name),
        rows[1],
    );

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
    let status = match &app.message {
        Some(n) => Paragraph::new(n.text.clone()).style(theme::style(n.role)),
        None => Paragraph::new(status_summary(app.session.status.as_ref())),
    };
    frame.render_widget(status, rows[3]);
    frame.render_widget(
        Paragraph::new("1/2/3/4 workspace · s settings · ? help · q quit ")
            .alignment(Alignment::Right),
        rows[3],
    );
}

/// Draw the right pane: the active workspace's body. The switcher line is drawn
/// full-width by `draw_main` so both panes align.
fn render_workspace(app: &mut App, frame: &mut Frame, area: Rect) {
    match app.workspace {
        Workspace::Plugins => render_plugins(app, frame, area),
        Workspace::Conflicts => render_conflicts(app, frame, area),
        Workspace::Downloads => render_downloads(app, frame, area),
        Workspace::Saves => render_saves(app, frame, area),
    }
}

/// The switcher line: every workspace name with the active one emphasised, plus its scope.
fn workspace_header(active: Workspace, profile: &str) -> Paragraph<'static> {
    let role = |on: bool| if on { Role::Heading } else { Role::Muted };
    let scope = match active {
        Workspace::Plugins => "load order".to_owned(),
        Workspace::Conflicts => "all enabled mods".to_owned(),
        Workspace::Downloads => "archives in downloads/".to_owned(),
        Workspace::Saves => format!("{profile}'s saves"),
    };
    let line = Line::from(vec![
        Span::styled(" Workspace  ", theme::style(Role::Muted)),
        Span::styled(
            "1 Plugins",
            theme::style(role(active == Workspace::Plugins)),
        ),
        Span::raw("  "),
        Span::styled(
            "2 Conflicts",
            theme::style(role(active == Workspace::Conflicts)),
        ),
        Span::raw("  "),
        Span::styled(
            "3 Downloads",
            theme::style(role(active == Workspace::Downloads)),
        ),
        Span::raw("  "),
        Span::styled("4 Saves", theme::style(role(active == Workspace::Saves))),
        Span::styled(format!("  · {scope}"), theme::style(Role::Muted)),
    ]);
    Paragraph::new(line)
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
    let rows: Vec<ListItem<'static>> = match &app.conflicts.status {
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
        ConflictsStatus::Ready(found) => found
            .iter()
            .map(|c| ListItem::new(format!("{} · {}", c.relative, c.providers.join(" < "))))
            .collect(),
    };
    render_pane(
        frame,
        area,
        CONFLICTS_TITLE.to_owned(),
        rows,
        &mut app.conflicts.list,
        focused,
    );
}

/// The downloads workspace: installable archives, or a hint to drop files in.
fn render_downloads(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    if app.downloads.entries.is_empty() {
        let text = format!(
            "No archives. Drop .7z/.zip files in {}.",
            app.session.instance.downloads_dir()
        );
        return render_workspace_message(frame, area, DOWNLOADS_TITLE, &text, focused);
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
    render_pane(
        frame,
        area,
        DOWNLOADS_TITLE.to_owned(),
        rows,
        &mut app.downloads.list,
        focused,
    );
}

/// The saves workspace: the profile's saves newest-first, or an empty-folder note.
fn render_saves(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Workspace;
    let title = format!(" Saves — {} ", app.session.profile.name);
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Popup;
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
        let out = render(&mut app, 80, 12);
        assert!(out.contains("No live deployment"), "status");
        assert!(out.contains("help"), "footer offers help");
        assert!(out.contains("quit"), "footer offers quit");
    }

    #[test]
    fn help_popup_lists_keybinds_when_open() {
        let mut app = App::sample();
        app.popup = Some(Popup::Help);
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Help"), "active tab label");
        assert!(out.contains("reorder"), "popup lists bindings");
        assert!(out.contains("Doctor"), "tab bar shows the other tabs");
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
    }

    #[test]
    fn conflicts_workspace_stale_prompts_to_scan() {
        use crate::app::Workspace;
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Press r"), "a stale scan prompts for r");
    }

    #[test]
    fn conflicts_workspace_ready_row_shows_the_priority_chain() {
        use crate::app::{ConflictsStatus, Workspace};
        use camino::Utf8PathBuf;
        use overseer_core::deploy::FileConflict;
        let mut app = App::sample();
        app.workspace = Workspace::Conflicts;
        app.conflicts.status = ConflictsStatus::Ready(vec![FileConflict {
            relative: Utf8PathBuf::from("shared.dds"),
            providers: vec!["Low".to_owned(), "High".to_owned()],
        }]);
        let out = render(&mut app, 80, 24);
        assert!(out.contains("shared.dds"), "the conflicting path is shown");
        assert!(out.contains("Low < High"), "providers render winner-last");
    }

    #[test]
    fn downloads_workspace_lists_archives_and_marks_installed() {
        use crate::app::Workspace;
        use camino::Utf8PathBuf;
        use overseer_core::install::DownloadEntry;
        let mut app = App::sample();
        app.workspace = Workspace::Downloads;
        app.downloads.entries = vec![
            DownloadEntry {
                name: "Alpha.zip".to_owned(),
                path: Utf8PathBuf::from("downloads/Alpha.zip"),
                installed: false,
            },
            DownloadEntry {
                name: "Beta.7z".to_owned(),
                path: Utf8PathBuf::from("downloads/Beta.7z"),
                installed: true,
            },
        ];
        app.downloads.list.select(Some(0));
        let out = render(&mut app, 80, 24);
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
    fn settings_popup_lists_recent_instances() {
        let mut app = App::sample();
        app.popup = Some(Popup::Settings);
        app.settings_state.select(Some(0));
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Settings"), "popup title");
        assert!(out.contains("alpha"), "lists a recent instance");
    }

    #[test]
    fn doctor_popup_shows_findings_and_summary() {
        use overseer_diagnostics::{Finding, Report, Severity};
        let mut app = App::sample();
        app.report = Some(Report::new(vec![Finding {
            check: "x",
            severity: Severity::Error,
            title: "Broken thing".to_owned(),
            detail: Some("Fix it like so.".to_owned()),
        }]));
        app.doctor_state.select(Some(0));
        app.popup = Some(Popup::Doctor);
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Doctor"), "active tab label");
        assert!(out.contains("1 error"), "title summarises severity counts");
        assert!(out.contains("Broken thing"), "lists the finding");
        assert!(
            out.contains("Fix it like so."),
            "detail pane shows the selected finding's detail"
        );
    }

    #[test]
    fn doctor_popup_reports_all_clear_when_empty() {
        use overseer_diagnostics::Report;
        let mut app = App::sample();
        app.report = Some(Report::new(vec![]));
        app.popup = Some(Popup::Doctor);
        let out = render(&mut app, 80, 24);
        assert!(out.contains("all clear"), "title says all clear");
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
