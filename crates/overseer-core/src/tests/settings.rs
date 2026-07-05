//! Tests for settings persistence and recent instances

use super::*;
use tempfile::TempDir;

/// A temp dir plus a config path inside it (the dir guards the file's lifetime)
fn temp_config() -> (TempDir, Utf8PathBuf) {
    let dir = TempDir::new().expect("temp dir");
    let path = Utf8PathBuf::from_path_buf(dir.path().join("config.toml")).expect("utf8 path");
    (dir, path)
}

#[test]
fn record_opened_dedupes_and_moves_to_front() {
    let mut s = Settings::default();
    s.record_opened(Utf8Path::new("/a"));
    s.record_opened(Utf8Path::new("/b"));
    s.record_opened(Utf8Path::new("/a")); // re-open `a`: to the front, no duplicate
    assert_eq!(
        s.recent_instances,
        vec![Utf8PathBuf::from("/a"), Utf8PathBuf::from("/b")]
    );
}

#[test]
fn record_opened_caps_the_list() {
    let mut s = Settings::default();
    for i in 0..(MAX_RECENT + 5) {
        s.record_opened(Utf8Path::new(&format!("/i{i}")));
    }
    assert_eq!(s.recent_instances.len(), MAX_RECENT);
    // The most recent open is at the front
    assert_eq!(
        s.last_instance(),
        Some(Utf8Path::new(&format!("/i{}", MAX_RECENT + 4)))
    );
}

#[test]
fn record_opened_dedupes_case_insensitively() {
    let mut s = Settings::default();
    s.record_opened(Utf8Path::new("C:/Games/Inst"));
    s.record_opened(Utf8Path::new("c:/games/inst")); // same path, different case
    assert_eq!(s.recent_instances, vec![Utf8PathBuf::from("c:/games/inst")]);
}

#[test]
fn resolve_prefers_explicit_then_last() {
    let mut s = Settings::default();
    assert_eq!(s.resolve_instance(None), None); // first run: nothing to open
    s.record_opened(Utf8Path::new("/last"));
    assert_eq!(s.resolve_instance(None), Some(Utf8PathBuf::from("/last")));
    assert_eq!(
        s.resolve_instance(Some(Utf8PathBuf::from("/explicit"))),
        Some(Utf8PathBuf::from("/explicit"))
    );
}

#[test]
fn save_then_load_round_trips() {
    let (_dir, path) = temp_config();
    let mut s = Settings::default();
    s.record_opened(Utf8Path::new("/x"));
    s.saves_sort = SavesSort {
        key: SavesSortKey::Character,
        dir: SortDir::Asc,
    };
    s.downloads_sort = DownloadsSort {
        key: DownloadsSortKey::Size,
        dir: SortDir::Desc,
    };
    s.save_to(&path).expect("save");
    let loaded = Settings::load_from(&path).expect("load");
    assert_eq!(loaded.recent_instances, s.recent_instances);
    assert_eq!(loaded.saves_sort, s.saves_sort);
    assert_eq!(loaded.downloads_sort, s.downloads_sort);
}

#[test]
fn loading_a_missing_file_yields_defaults() {
    let (_dir, path) = temp_config();
    let loaded = Settings::load_from(&path).expect("load");
    assert!(loaded.recent_instances.is_empty());
    assert_eq!(loaded.saves_sort, SavesSort::default());
    assert_eq!(loaded.downloads_sort, DownloadsSort::default());
}

#[test]
fn old_toml_missing_sort_fields_loads_defaults() {
    let (_dir, path) = temp_config();
    std::fs::write(&path, r#"recent_instances = ["/old/instance"]"#).expect("write");

    let loaded = Settings::load_from(&path).expect("load");

    assert_eq!(
        loaded.recent_instances,
        vec![Utf8PathBuf::from("/old/instance")]
    );
    assert_eq!(loaded.saves_sort, SavesSort::default());
    assert_eq!(loaded.downloads_sort, DownloadsSort::default());
}

#[test]
fn sort_defaults_are_preserved() {
    assert_eq!(
        SavesSort::default(),
        SavesSort {
            key: SavesSortKey::Date,
            dir: SortDir::Desc,
        }
    );
    assert_eq!(
        DownloadsSort::default(),
        DownloadsSort {
            key: DownloadsSortKey::Name,
            dir: SortDir::Asc,
        }
    );
}

#[test]
fn partial_sort_tables_use_pane_defaults() {
    let (_dir, path) = temp_config();
    std::fs::write(
        &path,
        r#"
[saves_sort]
key = "name"

[downloads_sort]
dir = "desc"
"#,
    )
    .expect("write");

    let loaded = Settings::load_from(&path).expect("load");

    assert_eq!(loaded.saves_sort.key, SavesSortKey::Name);
    assert_eq!(loaded.saves_sort.dir, SortDir::Desc);
    assert_eq!(loaded.downloads_sort.key, DownloadsSortKey::Name);
    assert_eq!(loaded.downloads_sort.dir, SortDir::Desc);
}

#[test]
fn unknown_sort_key_degrades_to_default_without_losing_recents() {
    let (_dir, path) = temp_config();
    std::fs::write(
        &path,
        r#"
recent_instances = ["/keep/me"]

[saves_sort]
key = "future_key"
dir = "desc"

[downloads_sort]
key = "newer_key"
dir = "asc"
"#,
    )
    .expect("write");

    let loaded = Settings::load_from(&path).expect("load");

    assert_eq!(loaded.recent_instances, vec![Utf8PathBuf::from("/keep/me")]);
    assert_eq!(loaded.saves_sort.key, SavesSortKey::Date);
    assert_eq!(loaded.saves_sort.dir, SortDir::Desc);
    assert_eq!(loaded.downloads_sort.key, DownloadsSortKey::Name);
    assert_eq!(loaded.downloads_sort.dir, SortDir::Asc);
}
