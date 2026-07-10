//! Tests for rendering App state into ratatui widgets

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
    let out = render(&mut app, 80, 32);
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
fn separator_header_fills_to_the_inner_width() {
    let line = separator_header("Mid", 20, false, 0);
    assert!(line.starts_with("▼ Mid "));
    assert!(line.ends_with('─'));
    assert_eq!(line.chars().count(), 18); // width 20 minus the 2 border columns
}

#[test]
fn a_collapsed_separator_shows_a_glyph_and_member_count() {
    let line = separator_header("Mid", 30, true, 5);
    assert!(
        line.starts_with("▶ Mid (5) "),
        "collapsed shows ▶ and the count"
    );
    assert!(line.ends_with('─'));
}

#[test]
fn a_separator_renders_as_a_header_rule_not_a_checkbox_row() {
    use overseer_core::instance::{ModKind, ModListEntry};

    let mut app = App::sample();
    app.session.profile.mods.insert(
        0,
        ModListEntry {
            name: "Gameplay_separator".to_owned(),
            enabled: false,
            kind: ModKind::Separator,
        },
    );
    app.mods.reset(&app.session.profile.mods);
    let out = render(&mut app, 60, 10);
    assert!(
        out.contains("Gameplay"),
        "shows the separator's display name"
    );
    assert!(out.contains("──"), "rendered as a rule");
    assert!(
        !out.contains("Gameplay_separator"),
        "the `_separator` suffix is stripped for display"
    );
    assert!(
        !out.contains("[ ] Gameplay"),
        "a separator has no checkbox marker"
    );
}

#[test]
fn a_plugin_separator_renders_as_a_header_in_the_plugins_pane() {
    use overseer_core::plugins::Separator;
    let mut app = App::sample();
    app.session.plugin_separators.items.push(Separator {
        name: "Endgame".to_owned(),
        anchor: Some("Cool.esp".to_owned()),
    });
    app.plugins
        .reset(&app.session.order.plugins, &app.session.plugin_separators);
    let out = render(&mut app, 80, 12);
    assert!(out.contains("Endgame"), "the plugin separator's name shows");
    assert!(out.contains("──"), "it renders as a header rule");
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
    // App::sample() defaults to the Plugins workspace
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
    use crate::test_support::download_entry;
    let mut app = App::sample();
    app.workspace = Workspace::Downloads;
    app.downloads.entries = vec![
        download_entry("Alpha.zip", 0, 0, false),
        download_entry("Beta.7z", 0, 0, true),
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
    assert!(
        out.contains("> /alpha"),
        "the selected row uses the shared selection cursor"
    );
}

#[test]
fn doctor_modal_shows_findings_and_summary() {
    use crate::app::{DoctorReport, ListCursor, Modal};
    use overseer_diagnostics::{Finding, Report, Severity};
    let mut app = App::sample();
    app.modal = Some(Modal::Doctor(DoctorReport {
        report: Report::new(vec![Finding {
            check: "x",
            severity: Severity::Error,
            title: "Broken thing".to_owned(),
            detail: Some("Fix it like so.".to_owned()),
        }]),
        list: ListCursor::first(1),
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
    use crate::app::{DoctorReport, ListCursor, Modal};
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
        list: ListCursor::first(1),
    }));
    // The trailing word only survives if the title wrapped instead of clipping at the findings pane's edge
    let out = render(&mut app, 80, 24);
    assert!(
        out.contains("row"),
        "a long finding title wraps to stay readable"
    );
}

#[test]
fn doctor_modal_reports_all_clear_when_empty() {
    use crate::app::{DoctorReport, ListCursor, Modal};
    use overseer_diagnostics::Report;
    let mut app = App::sample();
    app.modal = Some(Modal::Doctor(DoctorReport {
        report: Report::new(vec![]),
        list: ListCursor::first(0),
    }));
    let out = render(&mut app, 80, 24);
    assert!(out.contains("all clear"), "summary says all clear");
    assert!(
        out.contains("No problems found."),
        "detail pane shows the clean bill"
    );
}

#[test]
fn doctor_modal_reports_all_clear_with_only_info_findings() {
    use crate::app::{DoctorReport, ListCursor, Modal};
    use overseer_diagnostics::{Finding, Report};
    let mut app = App::sample();
    app.modal = Some(Modal::Doctor(DoctorReport {
        report: Report::new(vec![Finding::info("Healthy")]),
        list: ListCursor::first(1),
    }));

    let out = render(&mut app, 80, 24);
    assert!(out.contains("all clear"), "info findings are not problems");
    assert!(out.contains("Healthy"), "the info finding remains visible");
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

    // Clearing the input and submitting surfaces the inline validation error
    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let out = render(&mut app, 80, 24);
    assert!(out.contains("empty"), "the inline validation error renders");
}
