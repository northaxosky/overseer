//! Modal surfaces: views that block the main view and end in submit or cancel

use ratatui::widgets::ListState;

/// A surface that blocks the main view and ends in submit or cancel
#[derive(Debug)]
pub(crate) enum Modal {
    Select(Select),
}

/// Pick one item from a list and act on it
#[derive(Debug)]
pub(crate) struct Select {
    pub(crate) kind: SelectKind,
    pub(crate) items: Vec<String>,
    pub(crate) state: ListState,
}

/// Which selection a [`Select`] drives; its items and what submitting does
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectKind {
    Launch,
    Profile,
}

impl SelectKind {
    /// Accelerator that opens this selection from the main view
    pub(crate) fn toggle_key(self) -> char {
        match self {
            SelectKind::Launch => 'l',
            SelectKind::Profile => 'p',
        }
    }

    /// Message shown when the list has no items to choose from
    pub(crate) fn empty_message(self) -> &'static str {
        match self {
            SelectKind::Launch => "No launch targets. Add with `overseer exe add`.",
            SelectKind::Profile => "No profiles.",
        }
    }

    /// Verb naming what submitting does, shown in the hint line
    pub(crate) fn action_verb(self) -> &'static str {
        match self {
            SelectKind::Launch => "launch",
            SelectKind::Profile => "switch",
        }
    }
}
