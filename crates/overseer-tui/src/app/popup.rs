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
    ("1 / 2", "switch workspace"),
    ("r", "scan conflicts (Conflicts workspace)"),
    ("D / P", "deploy / purge"),
    ("l", "launch a target"),
    ("p", "switch profile"),
    ("n", "new profile"),
    ("s", "open settings"),
    ("d", "run diagnostics"),
    ("?", "toggle this help"),
    ("q / Esc", "quit"),
];

impl Popup {
    /// The tabs in the order they appear in the overlay's tab bar
    pub(crate) const TABS: [Popup; 3] = [Popup::Doctor, Popup::Settings, Popup::Help];

    /// This popups position in [`Popup::TABS`]
    pub(crate) fn index(self) -> usize {
        Self::TABS.iter().position(|&t| t == self).unwrap_or(0)
    }

    /// The label shown on this popup's tab
    pub(crate) fn label(self) -> &'static str {
        match self {
            Popup::Doctor => "Doctor",
            Popup::Settings => "Settings",
            Popup::Help => "Help",
        }
    }

    /// The tab `delta` steps away, wrapping around the ends
    pub(crate) fn cycle(self, delta: isize) -> Popup {
        let count = Self::TABS.len() as isize;
        let next = (self.index() as isize + delta).rem_euclid(count) as usize;
        Self::TABS[next]
    }
}
