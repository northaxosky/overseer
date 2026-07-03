//! Sorting for the view-owned Saves and Downloads lists.
//!
//! Sorting is a front-end concern: core returns a deterministic default order and the
//! view re-sorts to the user's saved preference. Nothing here touches domain data.

use std::cmp::Ordering;

use overseer_core::install::DownloadEntry;
use overseer_core::saves::SaveInfo;
use overseer_core::settings::{DownloadsSort, DownloadsSortKey, SavesSort, SavesSortKey, SortDir};
use strum::IntoEnumIterator;

use crate::app::App;

/// Re-order saves in place for `sort`, tie-broken by file name for a stable order.
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

/// Re-order downloads in place for `sort`, tie-broken by name for a stable order.
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

/// A compact `key ↑`/`key ↓` tag for the pane title (the key name is `strum::Display`).
pub(crate) fn saves_sort_label(sort: SavesSort) -> String {
    format!("{} {}", sort.key, dir_arrow(sort.dir))
}

pub(crate) fn downloads_sort_label(sort: DownloadsSort) -> String {
    format!("{} {}", sort.key, dir_arrow(sort.dir))
}

/// A view list (Saves or Downloads) sortable by a persisted key + direction.
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
        format!("{} {}", Self::key(app), dir_arrow(Self::dir(app)))
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
        app.saves
            .list
            .select((!app.saves.entries.is_empty()).then_some(0));
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
        app.downloads
            .list
            .select((!app.downloads.entries.is_empty()).then_some(0));
    }
}

impl App {
    pub(super) fn cycle_sort<P: SortablePane>(&mut self) {
        let next = next_variant(P::key(self));
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

    /// Persist the sort preferences best-effort — a failed write is logged, not fatal.
    fn save_sort_preferences(&self) {
        if let Err(e) = self.settings.save() {
            tracing::warn!(error = %e, "could not save sort preference");
        }
    }
}

/// The next variant after `current` in declaration order, wrapping at the end.
fn next_variant<T: IntoEnumIterator + PartialEq + Copy>(current: T) -> T {
    let all: Vec<T> = T::iter().collect();
    let idx = all.iter().position(|&v| v == current).unwrap_or(0);
    all[(idx + 1) % all.len()]
}

/// Flip an ascending ordering when the preference is descending.
fn apply_dir(ord: Ordering, dir: SortDir) -> Ordering {
    match dir {
        SortDir::Asc => ord,
        SortDir::Desc => ord.reverse(),
    }
}

/// Compare optional keys, sinking `None` last — direction flips only the `Some`/`Some` case.
fn cmp_optional<T: Ord>(a: Option<T>, b: Option<T>, dir: SortDir) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => apply_dir(a.cmp(&b), dir),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Case-insensitive name order (matching the core default); exact bytes break ties.
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

/// Each key has a sensible default direction — newest/highest first, names A→Z.
fn default_saves_dir(key: SavesSortKey) -> SortDir {
    match key {
        SavesSortKey::Date | SavesSortKey::Level => SortDir::Desc,
        SavesSortKey::Name | SavesSortKey::Character => SortDir::Asc,
    }
}

/// Newest/biggest first for date and size; names and install-state ascending.
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
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use overseer_core::saves::SaveMeta;
    use std::time::{Duration, SystemTime};

    fn save(name: &str, modified_secs: u64, meta: Option<SaveMeta>) -> SaveInfo {
        SaveInfo {
            path: Utf8PathBuf::from(format!("Saves/{name}")),
            file_name: name.to_owned(),
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs),
            meta,
        }
    }

    fn meta(character: &str, level: u32) -> SaveMeta {
        SaveMeta {
            save_number: 1,
            character: character.to_owned(),
            level,
            location: "Sanctuary".to_owned(),
            game_date: "Day 1".to_owned(),
        }
    }

    fn download(name: &str, size: u64, modified_secs: u64, installed: bool) -> DownloadEntry {
        DownloadEntry {
            name: name.to_owned(),
            path: Utf8PathBuf::from(format!("downloads/{name}")),
            installed,
            size,
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs),
        }
    }

    #[test]
    fn sorts_saves_by_date_in_both_directions() {
        let mut entries = vec![save("Old.fos", 10, None), save("New.fos", 20, None)];
        sort_saves(&mut entries, sort(SavesSortKey::Date, SortDir::Desc));
        assert_eq!(entries[0].file_name, "New.fos");
        sort_saves(&mut entries, sort(SavesSortKey::Date, SortDir::Asc));
        assert_eq!(entries[0].file_name, "Old.fos");
    }

    #[test]
    fn save_meta_sorts_keep_unparsed_entries_last_in_both_directions() {
        let mut entries = vec![
            save("Broken.fos", 30, None),
            save("Nora.fos", 10, Some(meta("Nora", 20))),
            save("Ada.fos", 20, Some(meta("Ada", 10))),
        ];
        sort_saves(&mut entries, sort(SavesSortKey::Character, SortDir::Asc));
        assert_eq!(names(&entries), ["Ada.fos", "Nora.fos", "Broken.fos"]);
        sort_saves(&mut entries, sort(SavesSortKey::Character, SortDir::Desc));
        assert_eq!(names(&entries), ["Nora.fos", "Ada.fos", "Broken.fos"]);
    }

    #[test]
    fn save_level_sort_keeps_unparsed_entries_last() {
        let mut entries = vec![
            save("Broken.fos", 30, None),
            save("Low.fos", 10, Some(meta("Nora", 10))),
            save("High.fos", 20, Some(meta("Nora", 30))),
        ];
        sort_saves(&mut entries, sort(SavesSortKey::Level, SortDir::Desc));
        assert_eq!(names(&entries), ["High.fos", "Low.fos", "Broken.fos"]);
    }

    #[test]
    fn sorts_downloads_by_each_key() {
        let entries = vec![
            download("Zeta.zip", 5, 10, true),
            download("alpha.7z", 10, 20, false),
        ];
        for key in [
            DownloadsSortKey::Name,
            DownloadsSortKey::Date,
            DownloadsSortKey::Size,
            DownloadsSortKey::Installed,
        ] {
            let mut sorted = entries.clone();
            sort_downloads(
                &mut sorted,
                DownloadsSort {
                    key,
                    dir: default_downloads_dir(key),
                },
            );
            assert_eq!(
                sorted[0].name, "alpha.7z",
                "alpha wins {key} in its default direction"
            );
        }
    }

    #[test]
    fn sort_labels_include_key_and_direction() {
        assert_eq!(saves_sort_label(SavesSort::default()), "date ↓");
        assert_eq!(downloads_sort_label(DownloadsSort::default()), "name ↑");
    }

    #[test]
    fn applying_a_sort_moves_the_cursor_to_the_top() {
        let mut app = App::sample();
        app.downloads.entries = vec![
            download("B.zip", 1, 10, false),
            download("A.zip", 1, 20, false),
        ];
        app.downloads.list.select(Some(1));
        app.settings.downloads_sort = DownloadsSort {
            key: DownloadsSortKey::Name,
            dir: SortDir::Asc,
        };
        DownloadsPane::resort(&mut app);
        assert_eq!(app.downloads.entries[0].name, "A.zip");
        assert_eq!(app.downloads.list.selected(), Some(0));
    }

    fn sort(key: SavesSortKey, dir: SortDir) -> SavesSort {
        SavesSort { key, dir }
    }

    fn names(entries: &[SaveInfo]) -> Vec<&str> {
        entries.iter().map(|e| e.file_name.as_str()).collect()
    }
}
