//! Overseer TUI: owns the terminal and drives the draw → input → update loop.
//! State lives in [`app`], rendering in [`ui`], argument parsing in [`cli`].

mod app;
mod cli;
mod theme;
mod ui;

#[cfg(test)]
mod test_support;

use anyhow::{Context, Result};
use overseer_core::settings::Settings;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::time::{Duration, Instant};

use app::App;

const OPERATION_TICK: Duration = Duration::from_millis(100);

fn main() -> Result<()> {
    // A TUI owns the terminal, so logging is silent on failure (stderr would corrupt the display)
    let _ = overseer_frontend::logging::init("warn,overseer_tui=info,overseer_core=info");
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
    app.finish_operation_after_terminal();

    match &result {
        Ok(()) => tracing::info!("overseer-tui exited"),
        Err(e) => tracing::error!(error = %e, "overseer-tui exited with error"),
    }
    result
}

/// The draw → input → update loop, running until the user quits
fn run(app: &mut App, terminal: &mut DefaultTerminal) -> Result<()> {
    let mut dirty = true;
    let mut deadline = Instant::now() + OPERATION_TICK;

    while !app.should_quit {
        if app.operation_running() {
            dirty |= app.poll_operation();

            if !app.operation_running() {
                dirty = true;
                continue;
            }

            let now = Instant::now();

            if now >= deadline {
                app.tick_operation();

                while deadline <= now {
                    deadline += OPERATION_TICK;
                }

                dirty = true;
            }

            if dirty {
                terminal.draw(|frame| ui::draw(app, frame))?;
                dirty = false;
            }

            if event::poll(deadline.saturating_duration_since(Instant::now()))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                app.handle_key(key);
                dirty = true;
            }
        } else {
            if dirty {
                terminal.draw(|frame| ui::draw(app, frame))?;
            }

            let event = event::read()?;
            dirty = true;

            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
            {
                app.handle_key(key);
            }

            deadline = Instant::now() + OPERATION_TICK;
        }
    }

    Ok(())
}
