//! Overseer TUI: owns the terminal and drives the draw → input → update loop.
//! State lives in [`app`], rendering in [`ui`], argument parsing in [`cli`].

mod app;
mod cli;
mod ui;

use anyhow::{Context, Result};
use overseer_core::settings::Settings;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use app::App;

fn main() -> Result<()> {
    // A TUI owns the terminal, so logging is silent on failure (stderr would corrupt the display).
    overseer_frontend::logging::init(overseer_frontend::logging::Config {
        default_filter: "warn,overseer_tui=info,overseer_core=info",
        warn_on_error: false,
    });
    tracing::info!("overseer-tui starting");

    let (explicit, profile) = cli::parse_args()?;
    let settings = Settings::load();
    let resolved = settings
        .resolve_instance(explicit)
        .context("no instance to open, pass `overseer-tui <instance-dir>` once to get started")?;
    let instance_dir = overseer_frontend::absolutize(&resolved)?;

    let mut app = App::load(&instance_dir, &profile, settings)
        .with_context(|| format!("loading instance at {instance_dir}"))?;

    let mut terminal = ratatui::init();
    let result = run(&mut app, &mut terminal);
    ratatui::restore();

    match &result {
        Ok(()) => tracing::info!("overseer-tui exited"),
        Err(e) => tracing::error!(error = %e, "overseer-tui exited with error"),
    }
    result
}

/// The draw → input → update loop, running until the user quits.
fn run(app: &mut App, terminal: &mut DefaultTerminal) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(app, frame))?;

        // Windows reports key release events too; act on presses only.
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }
    Ok(())
}
