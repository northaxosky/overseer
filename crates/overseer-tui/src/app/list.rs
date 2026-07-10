//! Shared cursor state for Ratatui lists

use ratatui::widgets::ListState;

#[derive(Debug, Default)]
pub(crate) struct ListCursor {
    state: ListState,
}

impl ListCursor {
    /// Create a selection on the first row when the list is non-empty
    pub(crate) fn first(len: usize) -> Self {
        let mut selection = Self::default();
        selection.reset_first(len);
        selection
    }

    /// The selected row index
    pub(crate) fn index(&self) -> Option<usize> {
        self.state.selected()
    }

    /// Select an index or clear the selection
    pub(crate) fn select(&mut self, index: Option<usize>) {
        self.state.select(index);
    }

    /// Select the first row when present without replacing scroll state
    pub(crate) fn select_first(&mut self, len: usize) {
        self.select((len > 0).then_some(0));
    }

    /// Replace the list state and select the first row when present
    pub(crate) fn reset_first(&mut self, len: usize) {
        self.state = ListState::default();
        self.select_first(len);
    }

    /// Clamp an existing selection to the current list bounds
    pub(crate) fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.select(None);
        } else if let Some(index) = self.index() {
            self.select(Some(index.min(len - 1)));
        }
    }

    /// Move the selection by `delta` without wrapping
    pub(crate) fn move_by(&mut self, len: usize, delta: isize) {
        if len == 0 {
            return;
        }
        let current = self.index().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        self.select(Some(next));
        self.clamp(len);
    }

    /// Mutable Ratatui state used by stateful list rendering
    pub(crate) fn state_mut(&mut self) -> &mut ListState {
        &mut self.state
    }
}

#[cfg(test)]
#[path = "tests/list.rs"]
mod tests;
