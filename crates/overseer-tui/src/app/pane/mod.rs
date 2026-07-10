//! State and semantic projections for grouped main-view panes

mod mods;

pub(crate) use mods::{ModPaneRow, ModsPane};

#[derive(Debug, Default)]
pub(crate) struct SeparatorUiState {
    collapsed: Vec<bool>,
}

impl SeparatorUiState {
    /// Reset collapse state to `count` expanded separators
    pub(super) fn reset(&mut self, count: usize) {
        self.collapsed.clear();
        self.collapsed.resize(count, false);
    }

    /// Insert an expanded separator at `separator_index`
    pub(super) fn insert(&mut self, separator_index: usize) {
        self.collapsed.insert(separator_index, false);
    }

    /// Remove collapse state at `separator_index`
    pub(super) fn remove(&mut self, separator_index: usize) {
        self.collapsed.remove(separator_index);
    }

    /// Toggle collapse state at `separator_index`
    pub(super) fn toggle(&mut self, separator_index: usize) {
        self.collapsed[separator_index] = !self.collapsed[separator_index];
    }

    /// Report whether `separator_index` is collapsed
    pub(super) fn is_collapsed(&self, separator_index: usize) -> bool {
        self.collapsed[separator_index]
    }

    /// Return the number of tracked separators
    fn len(&self) -> usize {
        self.collapsed.len()
    }
}
