//! Mods pane selection, separator collapse state, and display projection

use overseer_core::instance::{ModKind, ModListEntry};
use ratatui::widgets::ListState;

use super::SeparatorUiState;
use crate::app::ListCursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModPaneRow {
    Mod {
        model_index: usize,
    },
    Separator {
        model_index: usize,
        separator_index: usize,
        collapsed: bool,
        member_count: usize,
    },
}

impl ModPaneRow {
    /// Return the profile model index represented by this row
    pub(crate) fn model_index(self) -> usize {
        match self {
            Self::Mod { model_index } | Self::Separator { model_index, .. } => model_index,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ModsPane {
    selection: ListCursor,
    separators: SeparatorUiState,
}

impl ModsPane {
    /// Create pane state for the current profile mod list
    pub(crate) fn new(mods: &[ModListEntry]) -> Self {
        let mut pane = Self::default();
        pane.reset(mods);
        pane
    }

    /// Reset selection and collapse state for a replacement mod list
    pub(crate) fn reset(&mut self, mods: &[ModListEntry]) {
        self.separators.reset(separator_count(mods));
        let len = self.project(mods).len();
        self.selection.reset_first(len);
    }

    /// Preserve compatible view state while accepting a replacement mod model
    pub(crate) fn reconcile_model(&mut self, mods: &[ModListEntry]) {
        let count = separator_count(mods);

        if self.separators.len() != count {
            self.separators.reset(count);
        }

        let len = self.project(mods).len();
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

    /// Toggle collapse state by separator-only index
    pub(crate) fn toggle_separator(&mut self, separator_index: usize) {
        self.separators.toggle(separator_index);
    }

    /// Insert expanded collapse state by separator-only index
    pub(crate) fn insert_separator(&mut self, separator_index: usize) {
        self.separators.insert(separator_index);
    }

    /// Remove collapse state by separator-only index
    pub(crate) fn remove_separator(&mut self, separator_index: usize) {
        self.separators.remove(separator_index);
    }

    /// Project profile entries into complete visible semantic rows
    pub(crate) fn project(&self, mods: &[ModListEntry]) -> Vec<ModPaneRow> {
        let separator_count = separator_count(mods);
        assert_eq!(
            self.separators.len(),
            separator_count,
            "mod separator collapse state must align with profile order"
        );

        let mut separator_indices = vec![None; mods.len()];
        let mut next_separator = 0;
        for (model_index, entry) in mods.iter().enumerate() {
            if entry.kind == ModKind::Separator {
                separator_indices[model_index] = Some(next_separator);
                next_separator += 1;
            }
        }

        let mut rows = Vec::with_capacity(mods.len());
        let mut hidden = false;
        for model_index in (0..mods.len()).rev() {
            if mods[model_index].kind == ModKind::Separator {
                let separator_index = separator_indices[model_index]
                    .expect("separator entries have a separator-only index");
                let collapsed = self.separators.is_collapsed(separator_index);
                hidden = collapsed;
                rows.push(ModPaneRow::Separator {
                    model_index,
                    separator_index,
                    collapsed,
                    member_count: member_count(mods, model_index),
                });
            } else if !hidden {
                rows.push(ModPaneRow::Mod { model_index });
            }
        }
        rows
    }
}

/// Count separator entries in profile persistence order
fn separator_count(mods: &[ModListEntry]) -> usize {
    mods.iter()
        .filter(|entry| entry.kind == ModKind::Separator)
        .count()
}

/// Count members owned by the separator at `model_index`
fn member_count(mods: &[ModListEntry], model_index: usize) -> usize {
    mods[..model_index]
        .iter()
        .rev()
        .take_while(|entry| entry.kind != ModKind::Separator)
        .count()
}

#[cfg(test)]
#[path = "tests/mods.rs"]
mod tests;
