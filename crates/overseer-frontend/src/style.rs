//! Styling roles shared by all of Overseer's front ends

/// A semantic styling role
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Section heading
    Heading,
    /// Completed action, good result, or enabled item
    Success,
    /// Error or failed check
    Failure,
    /// Caution: something missing or removed the user should notice
    Warning,
    /// Secondary info: counts, hints, disables...
    Muted,
    /// Deployed / added entry
    Added,
    /// Removed Entry
    Removed,
}

/// A backend neutral terminal color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Positive: success and additions
    Green,
    /// Negative: errors and failures
    Red,
    /// Caution: warnings and removals
    Yellow,
}

/// The canonical styling for a role: an optional colour plus emphasis flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RoleStyle {
    /// Optional colour; `None` inherits the terminal default
    pub color: Option<Color>,
    /// Bold emphasis
    pub bold: bool,
    /// Dim emphasis
    pub dim: bool,
}

impl Role {
    /// This role's canonical, backend neutral styling
    pub fn palette(self) -> RoleStyle {
        let colored = |c| RoleStyle {
            color: Some(c),
            bold: true,
            dim: false,
        };
        match self {
            Role::Heading => RoleStyle {
                bold: true,
                ..RoleStyle::default()
            },
            Role::Success | Role::Added => colored(Color::Green),
            Role::Failure => colored(Color::Red),
            Role::Warning | Role::Removed => colored(Color::Yellow),
            Role::Muted => RoleStyle {
                dim: true,
                ..RoleStyle::default()
            },
        }
    }
}

#[cfg(test)]
#[path = "tests/style.rs"]
mod tests;
