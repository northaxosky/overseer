//! Overseer TUI — skeleton.
//!
//! Owns the terminal, so all diagnostics go to a log file (never stdout/stderr;
//! see [`logging`]). This is intentionally minimal: it opens the alternate
//! screen, renders a placeholder, and quits on `q` / `Esc` / `Ctrl-C`, restoring
//! the terminal on exit and on panic. Real screens are built on top of this.

mod logging;

use anyhow::Result;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    layout::Alignment,
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, Padding, Paragraph},
};

fn main() -> Result<()> {
    logging::init();
    tracing::info!("overseer-tui starting");

    // `ratatui::init` enters raw mode + the alternate screen and installs a
    // panic hook that restores the terminal before unwinding.
    let mut terminal = ratatui::init();
    let result = App::default().run(&mut terminal);
    ratatui::restore();

    match &result {
        Ok(()) => tracing::info!("overseer-tui exited"),
        Err(e) => tracing::error!(error = %e, "overseer-tui exited with error"),
    }
    result
}

/// Minimal application state.
#[derive(Debug, Default)]
struct App {
    should_quit: bool,
}

impl App {
    /// Run the draw/event loop until the user quits.
    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self.view(), frame.area());
    }

    /// The placeholder view, pulled out so it can be rendered to a `TestBackend`.
    fn view(&self) -> Paragraph<'static> {
        let block = Block::bordered()
            .title(" Overseer ")
            .title_alignment(Alignment::Center)
            .padding(Padding::uniform(1));
        let text = Text::from(vec![
            Line::from("TUI skeleton — nothing here yet.".bold()),
            Line::from(""),
            Line::from("press q to quit".dim()),
        ]);
        Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Center)
    }

    fn handle_events(&mut self) -> Result<()> {
        if let Event::Key(key) = event::read()?
            && is_quit(key)
        {
            self.should_quit = true;
        }
        Ok(())
    }
}

/// Whether a key event should quit the app: `q`, `Esc`, or `Ctrl-C`.
fn is_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn view_renders_the_placeholder_without_panicking() {
        let app = App::default();
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).expect("test backend");
        terminal.draw(|f| app.draw(f)).expect("draw");

        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            rendered.contains("Overseer"),
            "placeholder title is rendered"
        );
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
