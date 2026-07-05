//! Tests for sorting the Saves and Downloads lists

use super::*;
use crate::test_support::{download_entry, save_info};
use overseer_core::saves::SaveMeta;

fn meta(character: &str, level: u32) -> SaveMeta {
    SaveMeta {
        save_number: 1,
        character: character.to_owned(),
        level,
        location: "Sanctuary".to_owned(),
        game_date: "Day 1".to_owned(),
    }
}

#[test]
fn sorts_saves_by_date_in_both_directions() {
    let mut entries = vec![
        save_info("Old.fos", 10, None),
        save_info("New.fos", 20, None),
    ];
    sort_saves(&mut entries, sort(SavesSortKey::Date, SortDir::Desc));
    assert_eq!(entries[0].file_name, "New.fos");
    sort_saves(&mut entries, sort(SavesSortKey::Date, SortDir::Asc));
    assert_eq!(entries[0].file_name, "Old.fos");
}

#[test]
fn save_meta_sorts_keep_unparsed_entries_last_in_both_directions() {
    let mut entries = vec![
        save_info("Broken.fos", 30, None),
        save_info("Nora.fos", 10, Some(meta("Nora", 20))),
        save_info("Ada.fos", 20, Some(meta("Ada", 10))),
    ];
    sort_saves(&mut entries, sort(SavesSortKey::Character, SortDir::Asc));
    assert_eq!(names(&entries), ["Ada.fos", "Nora.fos", "Broken.fos"]);
    sort_saves(&mut entries, sort(SavesSortKey::Character, SortDir::Desc));
    assert_eq!(names(&entries), ["Nora.fos", "Ada.fos", "Broken.fos"]);
}

#[test]
fn save_level_sort_keeps_unparsed_entries_last() {
    let mut entries = vec![
        save_info("Broken.fos", 30, None),
        save_info("Low.fos", 10, Some(meta("Nora", 10))),
        save_info("High.fos", 20, Some(meta("Nora", 30))),
    ];
    sort_saves(&mut entries, sort(SavesSortKey::Level, SortDir::Desc));
    assert_eq!(names(&entries), ["High.fos", "Low.fos", "Broken.fos"]);
}

#[test]
fn sorts_downloads_by_each_key() {
    let entries = vec![
        download_entry("Zeta.zip", 5, 10, true),
        download_entry("alpha.7z", 10, 20, false),
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
        download_entry("B.zip", 1, 10, false),
        download_entry("A.zip", 1, 20, false),
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

#[test]
fn equal_keys_keep_a_name_ascending_tiebreak_regardless_of_direction() {
    // Same mtime so Date can't decide the order: the file-name tiebreak must
    let mut entries = vec![save_info("B.fos", 5, None), save_info("A.fos", 5, None)];
    sort_saves(&mut entries, sort(SavesSortKey::Date, SortDir::Desc));
    assert_eq!(
        names(&entries),
        ["A.fos", "B.fos"],
        "the tiebreak stays ascending under Desc"
    );
    sort_saves(&mut entries, sort(SavesSortKey::Date, SortDir::Asc));
    assert_eq!(names(&entries), ["A.fos", "B.fos"], "and under Asc");
}

#[test]
fn download_date_ties_break_on_case_insensitive_name() {
    // Equal mtime forces the tiebreak; "apple" < "banana" only case-insensitively
    let mut entries = vec![
        download_entry("Banana.zip", 0, 1, false),
        download_entry("apple.zip", 0, 1, false),
    ];
    sort_downloads(
        &mut entries,
        DownloadsSort {
            key: DownloadsSortKey::Date,
            dir: SortDir::Desc,
        },
    );
    assert_eq!(
        entries[0].name, "apple.zip",
        "case-insensitive name breaks the tie, ascending"
    );
    assert_eq!(entries[1].name, "Banana.zip");
}
