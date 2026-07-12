//! Plugins pane selection, separator collapse state, and display projection

use overseer_core::plugins::{PluginEntry, PluginRow, PluginSeparators, merge_rows};
use ratatui::widgets::ListState;

use super::SeparatorUiState;
use crate::app::ListCursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PluginPaneRow {
    Plugin {
        plugin_index: usize,
    },
    Separator {
        separator_index: usize,
        collapsed: bool,
        member_count: usize,
    },
}

#[derive(Debug, Default)]
pub(crate) struct PluginsPane {
    selection: ListCursor,
    separators: SeparatorUiState,
}

impl PluginsPane {
    /// Create pane state for the current plugin order and separator sidecar
    pub(crate) fn new(plugins: &[PluginEntry], separators: &PluginSeparators) -> Self {
        let mut pane = Self::default();
        pane.reset(plugins, separators);
        pane
    }

    /// Reset selection and collapse state for replacement plugin data
    pub(crate) fn reset(&mut self, plugins: &[PluginEntry], separators: &PluginSeparators) {
        self.separators.reset(separators.items.len());
        let len = self.project(plugins, separators).len();
        self.selection.reset_first(len);
    }

    /// Preserve compatible view state while accepting replacement plugin models
    pub(crate) fn reconcile_model(
        &mut self,
        plugins: &[PluginEntry],
        separators: &PluginSeparators,
    ) {
        if self.separators.len() != separators.items.len() {
            self.separators.reset(separators.items.len());
        }
        let len = self.project(plugins, separators).len();
        self.selection.clamp(len);
    }

    /// Return the selected display-row index
    pub(crate) fn index(&self) -> Option<usize> {
        self.selection.index()
    }

    /// Select a display-row index or clear the selection
    pub(crate) fn select(&mut self, index: Option<usize>) {
        self.selection.select(index);
    }

    /// Move the display selection by `delta` without wrapping
    pub(crate) fn move_by(&mut self, len: usize, delta: isize) {
        self.selection.move_by(len, delta);
    }

    /// Clamp the display selection to `len` rows
    pub(crate) fn clamp(&mut self, len: usize) {
        self.selection.clamp(len);
    }

    /// Expose mutable Ratatui state for stateful rendering
    pub(crate) fn state_mut(&mut self) -> &mut ListState {
        self.selection.state_mut()
    }

    /// Toggle collapse state by sidecar index
    pub(crate) fn toggle_separator(&mut self, separator_index: usize) {
        self.separators.toggle(separator_index);
    }

    /// Insert expanded collapse state by sidecar index
    pub(crate) fn insert_separator(&mut self, separator_index: usize) {
        self.separators.insert(separator_index);
    }

    /// Remove collapse state by sidecar index
    pub(crate) fn remove_separator(&mut self, separator_index: usize) {
        self.separators.remove(separator_index);
    }

    /// Project plugins and sidecar separators into complete visible semantic rows
    pub(crate) fn project(
        &self,
        plugins: &[PluginEntry],
        separators: &PluginSeparators,
    ) -> Vec<PluginPaneRow> {
        assert_eq!(
            self.separators.len(),
            separators.items.len(),
            "plugin separator collapse state must align with sidecar order"
        );

        let merged = merge_rows(plugins, &separators.items);
        let mut member_counts = vec![0; separators.items.len()];
        let mut owner = None;
        for row in &merged {
            match *row {
                PluginRow::Separator(separator_index) => owner = Some(separator_index),
                PluginRow::Plugin(_) => {
                    if let Some(separator_index) = owner {
                        member_counts[separator_index] += 1;
                    }
                }
            }
        }

        let mut rows = Vec::with_capacity(merged.len());
        let mut hidden = false;
        for row in merged {
            match row {
                PluginRow::Separator(separator_index) => {
                    let collapsed = self.separators.is_collapsed(separator_index);
                    hidden = collapsed;
                    rows.push(PluginPaneRow::Separator {
                        separator_index,
                        collapsed,
                        member_count: member_counts[separator_index],
                    });
                }
                PluginRow::Plugin(plugin_index) if !hidden => {
                    rows.push(PluginPaneRow::Plugin { plugin_index });
                }
                PluginRow::Plugin(_) => {}
            }
        }
        rows
    }
}

#[cfg(test)]
#[path = "tests/plugins.rs"]
mod tests;
