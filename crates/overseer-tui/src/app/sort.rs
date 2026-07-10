//! Sorting for the view-owned Saves and Downloads lists.
//!
//! Sorting is a front-end concern: core returns a deterministic default order and the
//! view re-sorts to the user's saved preference. Nothing here touches domain data.

use std::cmp::Ordering;

use overseer_core::install::DownloadEntry;
use overseer_core::saves::SaveInfo;
use overseer_core::settings::{DownloadsSort, DownloadsSortKey, SavesSort, SavesSortKey, SortDir};
use strum::IntoEnumIterator;

use crate::app::{App, cycle_variant};

/// Re-order saves in place for `sort`, tie-broken by file name for a stable order
pub(crate) fn sort_saves(entries: &mut [SaveInfo], sort: SavesSort) {
    entries.sort_by(|a, b| {
        let base = match sort.key {
            SavesSortKey::Date => apply_dir(a.modified.cmp(&b.modified), sort.dir),
            SavesSortKey::Name => apply_dir(a.file_name.cmp(&b.file_name), sort.dir),
            SavesSortKey::Character => cmp_optional(
                a.meta.as_ref().map(|m| m.character.as_str()),
                b.meta.as_ref().map(|m| m.character.as_str()),
                sort.dir,
            ),
            SavesSortKey::Level => cmp_optional(
                a.meta.as_ref().map(|m| m.level),
                b.meta.as_ref().map(|m| m.level),
                sort.dir,
            ),
        };
        base.then_with(|| a.file_name.cmp(&b.file_name))
    });
}

/// Re-order downloads in place for `sort`, tie-broken by name for a stable order
pub(crate) fn sort_downloads(entries: &mut [DownloadEntry], sort: DownloadsSort) {
    entries.sort_by(|a, b| {
        let base = match sort.key {
            DownloadsSortKey::Name => apply_dir(cmp_download_name(a, b), sort.dir),
            DownloadsSortKey::Date => apply_dir(a.modified.cmp(&b.modified), sort.dir),
            DownloadsSortKey::Size => apply_dir(a.size.cmp(&b.size), sort.dir),
            DownloadsSortKey::Installed => apply_dir(a.installed.cmp(&b.installed), sort.dir),
        };
        base.then_with(|| cmp_download_name(a, b))
    });
}

/// A compact `key ↑`/`key ↓` tag for the pane title (the key name is `strum::Display`)
pub(crate) fn saves_sort_label(sort: SavesSort) -> String {
    sort_label(sort.key, sort.dir)
}

pub(crate) fn downloads_sort_label(sort: DownloadsSort) -> String {
    sort_label(sort.key, sort.dir)
}

/// A compact `key ↑`/`key ↓` tag (the key name is `strum::Display`)
fn sort_label(key: impl std::fmt::Display, dir: SortDir) -> String {
    format!("{key} {}", dir_arrow(dir))
}

/// A view list (Saves or Downloads) sortable by a persisted key + direction
pub(super) trait SortablePane {
    type Key: IntoEnumIterator + PartialEq + Copy + std::fmt::Display;

    const LABEL: &'static str;

    fn key(app: &App) -> Self::Key;
    fn set_key(app: &mut App, key: Self::Key);
    fn dir(app: &App) -> SortDir;
    fn set_dir(app: &mut App, dir: SortDir);
    fn default_dir(key: Self::Key) -> SortDir;
    fn resort(app: &mut App);

    fn label(app: &App) -> String {
        sort_label(Self::key(app), Self::dir(app))
    }
}

pub(super) struct SavesPane;
pub(super) struct DownloadsPane;

impl SortablePane for SavesPane {
    type Key = SavesSortKey;

    const LABEL: &'static str = "Saves";

    fn key(app: &App) -> Self::Key {
        app.settings.saves_sort.key
    }

    fn set_key(app: &mut App, key: Self::Key) {
        app.settings.saves_sort.key = key;
    }

    fn dir(app: &App) -> SortDir {
        app.settings.saves_sort.dir
    }

    fn set_dir(app: &mut App, dir: SortDir) {
        app.settings.saves_sort.dir = dir;
    }

    fn default_dir(key: Self::Key) -> SortDir {
        default_saves_dir(key)
    }

    fn resort(app: &mut App) {
        sort_saves(&mut app.saves.entries, app.settings.saves_sort);
        app.saves.list.select_first(app.saves.entries.len());
    }
}

impl SortablePane for DownloadsPane {
    type Key = DownloadsSortKey;

    const LABEL: &'static str = "Downloads";

    fn key(app: &App) -> Self::Key {
        app.settings.downloads_sort.key
    }

    fn set_key(app: &mut App, key: Self::Key) {
        app.settings.downloads_sort.key = key;
    }

    fn dir(app: &App) -> SortDir {
        app.settings.downloads_sort.dir
    }

    fn set_dir(app: &mut App, dir: SortDir) {
        app.settings.downloads_sort.dir = dir;
    }

    fn default_dir(key: Self::Key) -> SortDir {
        default_downloads_dir(key)
    }

    fn resort(app: &mut App) {
        sort_downloads(&mut app.downloads.entries, app.settings.downloads_sort);
        app.downloads.list.select_first(app.downloads.entries.len());
    }
}

impl App {
    pub(super) fn cycle_sort<P: SortablePane>(&mut self) {
        let next = cycle_variant(P::key(self), 1);
        P::set_key(self, next);
        P::set_dir(self, P::default_dir(next));
        P::resort(self);
        self.note(format!("{} sort: {}", P::LABEL, P::label(self)));
        self.save_sort_preferences();
    }

    pub(super) fn toggle_sort_dir<P: SortablePane>(&mut self) {
        P::set_dir(self, toggle_dir(P::dir(self)));
        P::resort(self);
        self.note(format!("{} sort: {}", P::LABEL, P::label(self)));
        self.save_sort_preferences();
    }

    /// Persist the sort preferences best-effort — a failed write is logged, not fatal
    fn save_sort_preferences(&self) {
        if let Err(e) = self.settings.save() {
            tracing::warn!(error = %e, "could not save sort preference");
        }
    }
}

/// Flip an ascending ordering when the preference is descending
fn apply_dir(ord: Ordering, dir: SortDir) -> Ordering {
    match dir {
        SortDir::Asc => ord,
        SortDir::Desc => ord.reverse(),
    }
}

/// Compare optional keys, sinking `None` last — direction flips only the `Some`/`Some` case
fn cmp_optional<T: Ord>(a: Option<T>, b: Option<T>, dir: SortDir) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => apply_dir(a.cmp(&b), dir),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Case-insensitive name order (matching the core default); exact bytes break ties
fn cmp_download_name(a: &DownloadEntry, b: &DownloadEntry) -> Ordering {
    a.name
        .to_ascii_lowercase()
        .cmp(&b.name.to_ascii_lowercase())
        .then_with(|| a.name.cmp(&b.name))
}

fn toggle_dir(dir: SortDir) -> SortDir {
    match dir {
        SortDir::Asc => SortDir::Desc,
        SortDir::Desc => SortDir::Asc,
    }
}

/// Each key has a sensible default direction — newest/highest first, names A→Z
fn default_saves_dir(key: SavesSortKey) -> SortDir {
    match key {
        SavesSortKey::Date | SavesSortKey::Level => SortDir::Desc,
        SavesSortKey::Name | SavesSortKey::Character => SortDir::Asc,
    }
}

/// Newest/biggest first for date and size; names and install-state ascending
fn default_downloads_dir(key: DownloadsSortKey) -> SortDir {
    match key {
        DownloadsSortKey::Date | DownloadsSortKey::Size => SortDir::Desc,
        DownloadsSortKey::Name | DownloadsSortKey::Installed => SortDir::Asc,
    }
}

fn dir_arrow(dir: SortDir) -> &'static str {
    match dir {
        SortDir::Asc => "↑",
        SortDir::Desc => "↓",
    }
}

#[cfg(test)]
#[path = "tests/sort.rs"]
mod tests;
