//! Reading and deleting a profile's Fallout 4 `.fos` save games

use super::SaveParseError;
use crate::error::{IoError, io_err};
use crate::game::GameKind;
use camino::{Utf8Path, Utf8PathBuf};
use std::time::SystemTime;

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

/// A panic-free, little-endian reader over a byte slice; every read is bounds checked
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
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
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| SaveParseError::BadString)
    }
}

/// Parse the metadata fields of an uncompressed Fallout 4 `.fos` header
fn parse_header(bytes: &[u8], magic: &[u8]) -> Result<SaveMeta, SaveParseError> {
    let mut cur = Cursor::new(bytes);

    if cur.take(magic.len())? != magic {
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

/// Whether `path` is a save of the game (by extension), not a co-save
fn is_save(path: &Utf8Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// List a profile's saves in `dir`, newest first; empty when the game has no save support
pub fn list_saves(dir: &Utf8Path, game: GameKind) -> Result<Vec<SaveInfo>, IoError> {
    let Some(fmt) = game.save_format() else {
        return Ok(Vec::new());
    };
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
        if !is_save(&path, fmt.ext) {
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
            Ok(bytes) => parse_header(&bytes, fmt.magic)
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

/// Delete a save and its script extender co-save, refusing anything but a savegame
pub fn delete_save(path: &Utf8Path, game: GameKind) -> Result<(), IoError> {
    let Some(fmt) = game.save_format() else {
        return Err(io_err(
            path,
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "saves are not supported for this game yet",
            ),
        ));
    };

    if !is_save(path, fmt.ext) {
        return Err(io_err(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "refusing to delete a non-save file",
            ),
        ));
    }

    std::fs::remove_file(path).map_err(|e| io_err(path, e))?;

    if let Err(e) = crate::fs::remove_file_opt(&path.with_extension(fmt.cosave_ext)) {
        tracing::debug!(error = %e, "could not remove co-save");
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/info.rs"]
mod tests;
