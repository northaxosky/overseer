//! Overseer TUI: Owns the terminal

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
    // A TUI owns the terminal, so logging is silent on failure (stderr would corrupt display)
    overseer_frontend::logging::init(overseer_frontend::logging::Config {
        default_filter: "warn,overseer_tui=info,overseer_core=info",
        warn_on_error: false,
    });
    tracing::info!("overseer-tui starting");

    let mut terminal = ratatui::init();
    let result = App::default().run(&mut terminal);
    ratatui::restore();

    match &result {
        Ok(()) => tracing::info!("overseer-tui exited"),
        Err(e) => tracing::error!(error = %e, "overseer-tui exited with error"),
    }
    result
}

/// Minimal application state
#[derive(Debug, Default)]
struct App {
    should_quit: bool,
}

impl App {
    /// Run the draw/event loop until user quits
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

    /// the placeholder view, pulled out so it can be rendered to a `TestBackend`
    fn view(&self) -> Paragraph<'static> {
        let block = Block::bordered()
            .title(" Overseer ")
            .title_alignment(Alignment::Center)
            .padding(Padding::uniform(1));
        let text = Text::from(vec![
            Line::from("TUI Skeletion - nothing here yet.".bold()),
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

/// Whether a key event should quit the app: `q`, `Esc`, or `Ctrl-C`
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
