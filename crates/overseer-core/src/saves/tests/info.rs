//! Tests for parsing save-file info

use super::*;
use crate::test_support::{fos_bytes, set_mtime, temp, write_fos};
use std::time::Duration;

// --- parse_header ---

#[test]
fn parses_a_valid_header_into_exact_metadata() {
    let bytes = fos_bytes(7, "Nora", 42, "Sanctuary Hills", "Sundas, 12 Last Seed");
    assert_eq!(
        parse_header(&bytes).expect("parse"),
        SaveMeta {
            save_number: 7,
            character: "Nora".to_owned(),
            level: 42,
            location: "Sanctuary Hills".to_owned(),
            game_date: "Sundas, 12 Last Seed".to_owned(),
        }
    );
}

#[test]
fn an_empty_player_name_parses() {
    let bytes = fos_bytes(1, "", 1, "Vault 111", "Day 1");
    assert_eq!(parse_header(&bytes).expect("parse").character, "");
}

#[test]
fn bad_magic_is_rejected() {
    let mut bytes = fos_bytes(1, "Nate", 3, "Concord", "Day 2");
    bytes[0] = b'X';
    assert_eq!(parse_header(&bytes), Err(SaveParseError::BadMagic));
}

#[test]
fn a_header_truncated_mid_string_is_eof() {
    let bytes = fos_bytes(1, "Nate", 3, "Concord", "Day 2");
    // Cut into the trailing game-date wstring's content bytes
    let truncated = &bytes[..bytes.len() - 3];
    assert_eq!(parse_header(truncated), Err(SaveParseError::UnexpectedEof));
}

#[test]
fn a_bogus_huge_string_length_is_eof_not_a_huge_alloc() {
    // The player-name length prefix sits at magic(12)+headerSize(4)+version(4)+saveNumber(4)
    let mut bytes = fos_bytes(1, "Nate", 3, "Concord", "Day 2");
    bytes[24] = 0xFF;
    bytes[25] = 0xFF; // claims a 65535-byte name in a tiny buffer
    assert_eq!(parse_header(&bytes), Err(SaveParseError::UnexpectedEof));
}

#[test]
fn a_non_utf8_string_is_a_bad_string() {
    // Hand-build a header whose player-name bytes are not valid UTF-8
    let mut body = Vec::new();
    body.extend_from_slice(&14u32.to_le_bytes()); // version
    body.extend_from_slice(&1u32.to_le_bytes()); // saveNumber
    body.extend_from_slice(&2u16.to_le_bytes()); // name length 2
    body.extend_from_slice(&[0xFF, 0xFF]); // invalid UTF-8
    body.extend_from_slice(&1u32.to_le_bytes()); // level
    body.extend_from_slice(&0u16.to_le_bytes()); // location ""
    body.extend_from_slice(&0u16.to_le_bytes()); // gameDate ""
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"FO4_SAVEGAME");
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&body);
    assert_eq!(parse_header(&bytes), Err(SaveParseError::BadString));
}

#[test]
fn an_absurd_header_size_is_rejected() {
    let mut bytes = fos_bytes(1, "Nate", 3, "Concord", "Day 2");
    // headerSize is the u32 immediately after the 12-byte magic
    bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        parse_header(&bytes),
        Err(SaveParseError::HeaderTooLarge(_))
    ));
}

// --- list_saves ---

#[test]
fn missing_saves_dir_is_an_empty_list() {
    let (_t, root) = temp();
    assert!(
        list_saves(&root.join("Saves/None"))
            .expect("list")
            .is_empty()
    );
}

#[test]
fn lists_fos_saves_newest_first_ignoring_other_entries() {
    let (_t, dir) = temp();
    write_fos(&dir.join("Old.fos"), 1, "Nora", 5, "Vault 111", "Day 1");
    write_fos(&dir.join("New.fos"), 2, "Nora", 9, "Concord", "Day 3");
    // A co-save, junk, and a subdirectory that must all be ignored
    std::fs::write(dir.join("New.f4se"), b"cosave").expect("cosave");
    std::fs::write(dir.join("notes.txt"), b"x").expect("junk");
    std::fs::create_dir_all(dir.join("Backups")).expect("subdir");

    let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    set_mtime(&dir.join("Old.fos"), base);
    set_mtime(&dir.join("New.fos"), base + Duration::from_secs(60));

    let saves = list_saves(&dir).expect("list");
    let names: Vec<&str> = saves.iter().map(|s| s.file_name.as_str()).collect();
    assert_eq!(names, ["New.fos", "Old.fos"], "newest first, only .fos");
    assert_eq!(saves[0].meta.as_ref().expect("meta").save_number, 2);
}

#[test]
fn equal_mtimes_break_ties_by_name() {
    let (_t, dir) = temp();
    write_fos(&dir.join("Bravo.fos"), 2, "A", 1, "L", "D");
    write_fos(&dir.join("Alpha.fos"), 1, "A", 1, "L", "D");
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(500_000);
    set_mtime(&dir.join("Bravo.fos"), when);
    set_mtime(&dir.join("Alpha.fos"), when);

    let names: Vec<String> = list_saves(&dir)
        .expect("list")
        .into_iter()
        .map(|s| s.file_name)
        .collect();
    assert_eq!(names, ["Alpha.fos", "Bravo.fos"], "ties sort by name");
}

#[test]
fn the_fos_extension_match_is_case_insensitive() {
    let (_t, dir) = temp();
    write_fos(&dir.join("Upper.FOS"), 1, "A", 1, "L", "D");
    assert_eq!(list_saves(&dir).expect("list").len(), 1);
}

#[test]
fn a_corrupt_save_still_lists_with_no_meta() {
    let (_t, dir) = temp();
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("Broken.fos"), b"not a real save").expect("write");

    let saves = list_saves(&dir).expect("list");
    assert_eq!(saves.len(), 1);
    assert_eq!(saves[0].file_name, "Broken.fos");
    assert!(
        saves[0].meta.is_none(),
        "an unparseable save has meta: None"
    );
}

// --- delete_save ---

#[test]
fn delete_removes_the_save_and_its_co_save() {
    let (_t, dir) = temp();
    write_fos(&dir.join("Save1.fos"), 1, "A", 1, "L", "D");
    std::fs::write(dir.join("Save1.f4se"), b"cosave").expect("cosave");

    delete_save(&dir.join("Save1.fos")).expect("delete");
    assert!(!dir.join("Save1.fos").exists(), "the save is gone");
    assert!(!dir.join("Save1.f4se").exists(), "the co-save is gone");
}

#[test]
fn delete_tolerates_a_missing_co_save() {
    let (_t, dir) = temp();
    write_fos(&dir.join("Save1.fos"), 1, "A", 1, "L", "D");
    delete_save(&dir.join("Save1.fos")).expect("delete");
    assert!(!dir.join("Save1.fos").exists());
}

#[test]
fn delete_refuses_a_non_fos_file() {
    let (_t, dir) = temp();
    std::fs::write(dir.join("keep.txt"), b"important").expect("write");
    delete_save(&dir.join("keep.txt")).expect_err("must refuse a non-save");
    assert!(dir.join("keep.txt").exists(), "the non-save is untouched");
}
