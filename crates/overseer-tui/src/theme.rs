//! The TUI's theme: maps the shared `Role`s to concrete `ratatui::Style`s

use overseer_frontend::style::{Color, Role};
use ratatui::style::{Color as TuiColor, Modifier, Style};

/// The ratatui style for a semantic role, derived from the shared palette
pub(crate) fn style(role: Role) -> Style {
    let p = role.palette();
    let mut style = Style::new();
    if let Some(color) = p.color {
        style = style.fg(match color {
            Color::Green => TuiColor::Green,
            Color::Red => TuiColor::Red,
            Color::Yellow => TuiColor::Yellow,
        });
    }
    if p.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if p.dim {
        style = style.add_modifier(Modifier::DIM);
    }
    style
}

/// Highlight for the selected row in a list
pub(crate) fn selection_style() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

#[cfg(test)]
#[path = "tests/theme.rs"]
mod tests;
