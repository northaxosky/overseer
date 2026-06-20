//! Overseer TUI: Owns the terminal (currently read-only)

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::apply::{self, DeploymentStatus};
use overseer_core::instance::{Instance, ModListEntry, Profile};
use overseer_core::plugins::{PluginEntry, PluginLoadOrder, PluginMeta, discover_plugins};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style, Stylize},
    text::Line,
    widgets::{Block, BorderType, List, ListItem, ListState, Paragraph},
};

fn main() -> Result<()> {
    // A TUI owns the terminal, so logging is silent on failure (stderr would corrupt display)
    overseer_frontend::logging::init(overseer_frontend::logging::Config {
        default_filter: "warn,overseer_tui=info,overseer_core=info",
        warn_on_error: false,
    });
    tracing::info!("overseer-tui starting");

    let (instance_dir, profile) = parse_args()?;

    // Load before entering alt scree so a load error prints normally
    let mut app = App::load(&instance_dir, &profile)
        .with_context(|| format!("loading instance at {instance_dir}"))?;

    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();

    match &result {
        Ok(()) => tracing::info!("overseer-tui exited"),
        Err(e) => tracing::error!(error = %e, "overseer-tui exited with error"),
    }
    result
}

/// Parse `overseer-tui <instance-dir> [--profile NAME]`
fn parse_args() -> Result<(Utf8PathBuf, String)> {
    let mut instance: Option<Utf8PathBuf> = None;
    let mut profile = String::from("Default");
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" => profile = args.next().context("--profile needs a value")?,
            _ if instance.is_none() => instance = Some(Utf8PathBuf::from(arg)),
            _ => bail!("unexpected argument: {arg}"),
        }
    }
    let instance = instance.context("usage: overseer-tui <instance-dir> [--profile NAME]")?;
    Ok((instance, profile))
}

/// Which pane has keyboard focus
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Focus {
    #[default]
    Mods,
    Plugins,
}

/// The loaded snapshot the UI renders
#[derive(Debug, Default)]
struct App {
    should_quit: bool,
    focus: Focus,
    profile_name: String,
    mods: Vec<ModListEntry>,
    plugins: Vec<PluginEntry>,
    discovered: Vec<PluginMeta>,
    status: Option<DeploymentStatus>,
    mods_state: ListState,
    plugins_state: ListState,
}

impl App {
    /// Load an instance snapshot
    fn load(instance_dir: &Utf8Path, profile_name: &str) -> Result<Self> {
        let instance = Instance::load(instance_dir.to_owned())?;

        let mut profile = Profile::load(&instance, profile_name)?;
        profile.reconcile(&instance)?;

        let discovered = discover_plugins(&instance, &profile)?;
        let mut order = PluginLoadOrder::load(&instance, profile_name)?;
        order.reconcile(&discovered);

        let status = apply::status(&instance)?;
        let mods = profile.mods;
        let plugins = order.plugins;

        Ok(Self {
            profile_name: profile_name.to_owned(),
            mods_state: initial_selection(mods.len()),
            plugins_state: initial_selection(plugins.len()),
            mods,
            plugins,
            discovered,
            status,
            ..Self::default()
        })
    }

    /// Run the draw/event loop until user quits
    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let rows = Layout::vertical([
            Constraint::Length(1), // header
            Constraint::Fill(1),   // body
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

        let header = Line::from(vec![
            " Overseer ".bold(),
            format!(" · {} ", self.profile_name).dim(),
        ]);
        frame.render_widget(Paragraph::new(header), rows[0]);

        let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(rows[1]);

        let mods_focused = self.focus == Focus::Mods;
        let mods_title = format!(" mods — {} ({}) ", self.profile_name, self.mods.len());
        let mods_items: Vec<ListItem<'static>> = self
            .mods
            .iter()
            .map(|m| ListItem::new(format!("{} {}", marker(m.enabled), m.name)))
            .collect();
        render_pane(
            frame,
            cols[0],
            mods_title,
            mods_items,
            &mut self.mods_state,
            mods_focused,
        );

        let plugins_focused = self.focus == Focus::Plugins;
        let plugins_title = format!(" plugins — {} ", self.plugins.len());
        let plugins_items: Vec<ListItem<'static>> = self
            .plugins
            .iter()
            .map(|p| {
                let tag = if is_master(&self.discovered, &p.name) {
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
            &mut self.plugins_state,
            plugins_focused,
        );

        let foot = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(rows[2]);
        frame.render_widget(Paragraph::new(self.status_summary()), foot[0]);
        frame.render_widget(
            Paragraph::new(" Tab: switch · j/k: move · q: quit ").alignment(Alignment::Right),
            foot[1],
        );
    }

    fn handle_events(&mut self) -> Result<()> {
        let Event::Key(key) = event::read()? else {
            return Ok(());
        };

        // Windows reports key release events too
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        if is_quit(key) {
            self.should_quit = true;
            return Ok(());
        }

        match key.code {
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            _ => {}
        }
        Ok(())
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Mods => Focus::Plugins,
            Focus::Plugins => Focus::Mods,
        };
    }

    /// Move the selection within the focused pane
    fn move_selection(&mut self, delta: isize) {
        let (state, len) = match self.focus {
            Focus::Mods => (&mut self.mods_state, self.mods.len()),
            Focus::Plugins => (&mut self.plugins_state, self.plugins.len()),
        };
        if len == 0 {
            return;
        }
        let current = state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        state.select(Some(next));
    }

    /// One line summary of the instance's deployment status
    fn status_summary(&self) -> String {
        match &self.status {
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
}

/// The enabled/active checkbox marker
fn marker(on: bool) -> &'static str {
    if on { "[x]" } else { "[ ]" }
}

/// Whether a plugin name is a master
fn is_master(discovered: &[PluginMeta], name: &str) -> bool {
    discovered
        .iter()
        .any(|m| m.is_master && m.name.eq_ignore_ascii_case(name))
}

/// A `ListState` selecting the first row when the list is not empty
fn initial_selection(len: usize) -> ListState {
    let mut state = ListState::default();
    if len > 0 {
        state.select(Some(0));
    }
    state
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
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    }
    frame.render_stateful_widget(list, area, state);
}

/// Whether a key event should quit the app: `q`, `Esc`, or `Ctrl-C`
fn is_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn sample_app() -> App {
        App {
            profile_name: "Default".to_owned(),
            mods: vec![
                ModListEntry {
                    name: "CoolMod".to_owned(),
                    enabled: true,
                    foreign: false,
                },
                ModListEntry {
                    name: "OffMod".to_owned(),
                    enabled: false,
                    foreign: false,
                },
            ],
            plugins: vec![
                PluginEntry {
                    name: "Cool.esm".to_owned(),
                    active: true,
                },
                PluginEntry {
                    name: "Cool.esp".to_owned(),
                    active: false,
                },
            ],
            discovered: vec![PluginMeta {
                name: "Cool.esm".to_owned(),
                is_master: true,
                is_light: false,
                masters: Vec::new(),
            }],
            mods_state: initial_selection(2),
            plugins_state: initial_selection(2),
            ..App::default()
        }
    }

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test backend");
        terminal.draw(|f| app.draw(f)).expect("draw");
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
        let mut app = sample_app(); // no live deployment
        let out = render(&mut app, 80, 12);
        assert!(
            out.contains("No live deployment"),
            "footer shows deployment status"
        );
        assert!(out.contains("q: quit"), "footer shows key hints");
    }

    #[test]
    fn both_panes_render_their_contents() {
        let mut app = sample_app();
        let out = render(&mut app, 60, 10);
        assert!(out.contains("CoolMod"), "mods pane lists mods");
        assert!(out.contains("Cool.esp"), "plugins pane lists plugins");
        assert!(out.contains("(master)"), "master plugins are tagged");
    }

    #[test]
    fn tab_toggles_focus() {
        let mut app = sample_app();
        assert_eq!(app.focus, Focus::Mods);
        app.toggle_focus();
        assert_eq!(app.focus, Focus::Plugins);
        app.toggle_focus();
        assert_eq!(app.focus, Focus::Mods);
    }

    #[test]
    fn selection_moves_and_clamps_within_the_focused_pane() {
        let mut app = sample_app();
        assert_eq!(app.mods_state.selected(), Some(0));
        app.move_selection(-1); // already at top → clamps
        assert_eq!(app.mods_state.selected(), Some(0));
        app.move_selection(1);
        assert_eq!(app.mods_state.selected(), Some(1));
        app.move_selection(1); // at bottom (len 2) → clamps
        assert_eq!(app.mods_state.selected(), Some(1));
        // The plugins pane is independent and untouched while Mods is focused.
        assert_eq!(app.plugins_state.selected(), Some(0));
    }

    #[test]
    fn quit_keys_are_recognised() {
        assert!(is_quit(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE
        )));
        assert!(is_quit(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(is_quit(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_quit(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        )));
    }
}
