//! The TUI's theme: maps the shared `Role`s to concrete `ratatui::Style`s

use overseer_frontend::style::Role;
use ratatui::style::{Color, Modifier, Style};

/// The ratatui style for a semantic role
pub(crate) fn style(role: Role) -> Style {
    match role {
        Role::Heading => Style::new().add_modifier(Modifier::BOLD),
        Role::Success | Role::Added => Style::new().fg(Color::Green),
        Role::Failure => Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        Role::Warning | Role::Removed => Style::new().fg(Color::Yellow),
        Role::Muted => Style::new().add_modifier(Modifier::DIM),
    }
}

/// Highlight for the selected row in a list
pub(crate) fn selection_style() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_map_to_distinct_styles() {
        assert_eq!(style(Role::Success), style(Role::Added));
        assert_ne!(style(Role::Success), style(Role::Muted));
    }
}
