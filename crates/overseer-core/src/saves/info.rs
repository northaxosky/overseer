//! Reading and deleting a profile's Fallout 4 `.fos` save games

use crate::error::{IoError, io_err};
use camino::{Utf8Path, Utf8PathBuf};
use std::time::SystemTime;
use thiserror::Error;

/// The 12-byte magic every Fallout 4 save begins with
const MAGIC: &[u8] = b"FO4_SAVEGAME";

/// Read at most this many bytes of a save to parse its header
const MAX_HEADER_BYTES: usize = 64 * 1024;

/// One save game in a profile's save folder, best-effort parsed metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveInfo {
    /// Absolute path to the `.fos` file
    pub path: Utf8PathBuf,
    /// The save's file name, e.g. `Save7_...fos`
    pub file_name: String,
    /// The file's last-modified time, used to sort newest-first
    pub modified: SystemTime,
    /// Parsed header fields, or `None` when the header couldn't be parsed/read
    pub meta: Option<SaveMeta>,
}

/// The header fields Overseer surfaces for a save
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveMeta {
    /// The in-game save slot number
    pub save_number: u32,
    /// The player character's name
    pub character: String,
    /// The character's level
    pub level: u32,
    /// The cell/workspace the save was made in
    pub location: String,
    /// Opaque in-game date text (format is engine-defined)
    pub game_date: String,
}

/// Why a `.fos` save header could not be parsed
#[derive(Debug, Clone, PartialEq, Eq, Error)]
enum SaveParseError {
    /// The file did not begin with the `FO4_SAVEGAME` magic
    #[error("not a Fallout 4 save: bad magic")]
    BadMagic,
    /// The header ended before a field we needed was fully read
    #[error("save header ended unexpectedly")]
    UnexpectedEof,
    /// A header string was not valid UTF-8
    #[error("save header string was not valid UTF-8")]
    BadString,
    /// The declared header size was implausibly large
    #[error("save header size {0} is implausibly large")]
    HeaderTooLarge(usize),
}

/// A panic-free, little-endian reader over a byte slice; every read is bounds checked
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Unread bytes left in the slice
    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    /// Take `n` bytes and advance; [`SaveParseError::UnexpectedEof`] if short
    fn take(&mut self, n: usize) -> Result<&'a [u8], SaveParseError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(SaveParseError::UnexpectedEof)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(SaveParseError::UnexpectedEof)?;
        self.pos = end;
        Ok(slice)
    }

    /// Read a little-endian `u16`
    fn u16(&mut self) -> Result<u16, SaveParseError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    /// Read a little-endian `u32`
    fn u32(&mut self) -> Result<u32, SaveParseError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Read a Bethesda wstring: a `u16` LE *byte length*, then that many UTF-8 bytes
    fn wstring(&mut self) -> Result<String, SaveParseError> {
        let len = self.u16()? as usize;
        if len > self.remaining() {
            return Err(SaveParseError::UnexpectedEof);
        }
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| SaveParseError::BadString)
    }
}

/// Parse the metadata fields of an uncompressed Fallout 4 `.fos` header
fn parse_header(bytes: &[u8]) -> Result<SaveMeta, SaveParseError> {
    let mut cur = Cursor::new(bytes);

    if cur.take(MAGIC.len())? != MAGIC {
        return Err(SaveParseError::BadMagic);
    }

    let header_size = cur.u32()? as usize;
    if header_size > MAX_HEADER_BYTES {
        return Err(SaveParseError::HeaderTooLarge(header_size));
    }

    cur.u32()?; // version
    let save_number = cur.u32()?;
    let character = cur.wstring()?;
    let level = cur.u32()?;
    let location = cur.wstring()?;
    let game_date = cur.wstring()?;

    Ok(SaveMeta {
        save_number,
        character,
        level,
        location,
        game_date,
    })
}

/// Read the front of a save (up to [`MAX_HEADER_BYTES`], or whole file if smaller)
fn read_header_prefix(path: &Utf8Path) -> Result<Vec<u8>, IoError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| io_err(path, e))?;
    let mut buf = Vec::new();
    file.take(MAX_HEADER_BYTES as u64)
        .read_to_end(&mut buf)
        .map_err(|e| io_err(path, e))?;
    Ok(buf)
}

/// List a profile's `.fos` saves in `dir`, newest first
pub fn list_saves(dir: &Utf8Path) -> Result<Vec<SaveInfo>, IoError> {
    let Some(entries) = crate::fs::read_dir_opt(dir)? else {
        return Ok(Vec::new());
    };

    let mut saves = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tracing::debug!(error = %e, "skipping unreadable saves entry");
                continue;
            }
        };

        // Files only: skip subdirectories
        match entry.file_type() {
            Ok(ft) if ft.is_file() => {}
            Ok(_) => continue,
            Err(e) => {
                tracing::debug!(error = %e, "skipping unreadable saves entry");
                continue;
            }
        }

        // Non UTF-8 names cant be saves we manage
        let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let path = dir.join(&file_name);

        // Only `.fos` saves; ignore cosaves
        if path.extension().map(|e| e.eq_ignore_ascii_case("fos")) != Some(true) {
            continue;
        }

        // mtime drives the sort
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or_else(|e| {
                tracing::debug!(path = %path, error = %e, "save mtime unavailable; using epoch");
                SystemTime::UNIX_EPOCH
            });

        let meta = match read_header_prefix(&path) {
            Ok(bytes) => parse_header(&bytes)
                .inspect_err(
                    |e| tracing::debug!(path = %path, error = %e, "could not parse save header"),
                )
                .ok(),
            Err(e) => {
                tracing::debug!(path = %path, error = %e, "could not read save header");
                None
            }
        };

        saves.push(SaveInfo {
            path,
            file_name,
            modified,
            meta,
        });
    }

    // Newest first; ties broken by name for a stable, deterministic order
    saves.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    Ok(saves)
}

/// Delete a save and its script extender co-save, refusing anything but a `.fos`
pub fn delete_save(path: &Utf8Path) -> Result<(), IoError> {
    // Safety guard: never delete anything but a Fallout 4 save
    if path.extension().map(|e| e.eq_ignore_ascii_case("fos")) != Some(true) {
        return Err(io_err(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "refusing to delete a non-.fos file",
            ),
        ));
    }

    std::fs::remove_file(path).map_err(|e| io_err(path, e))?;

    if let Err(e) = crate::fs::remove_file_opt(&path.with_extension("f4se")) {
        tracing::debug!(error = %e, "could not remove co-save");
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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
}
