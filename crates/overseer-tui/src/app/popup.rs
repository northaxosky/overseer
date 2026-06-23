//! The popup overlay: which view is showing and the help popup's contents.

/// A popup floating over the main view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Popup {
    Help,
    Settings,
    Doctor,
    // ModActions, etc... later
}

/// Key bindings shown (and selectable) in the help popup: (keys, description).
pub(crate) const HELP_ENTRIES: &[(&str, &str)] = &[
    ("j / k   ↓ / ↑", "move selection"),
    ("Tab", "switch pane"),
    ("Space / Enter", "toggle enabled / active"),
    ("J / K", "reorder mod (priority)"),
    ("s", "open settings"),
    ("d", "run diagnostics"),
    ("?", "toggle this help"),
    ("q / Esc", "quit"),
];
