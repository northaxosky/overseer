//! In-place binary and archive patching: game-agnostic mechanisms, with per-game policy in [`fallout4`].
//!
//! [`set_version`] flips a BA2 header version field; [`delta`] and [`vcdiff`] apply and map VCDIFF
//! binary deltas; [`fingerprint`] verifies file identity; [`engine`] is the shared crash-safe convert
//! engine (apply a verified delta, swap atomically). Per-game policy (which edition maps to which
//! fingerprint, which transitions are valid) lives in [`fallout4`].

pub mod delta;
pub mod engine;
pub mod fallout4;
pub mod fingerprint;
pub mod vcdiff;

use crate::archive::{Ba2Error, Ba2Header, HEADER_LEN};
use crate::error::io_err;
use camino::Utf8Path;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};

/// Byte offset of the `u32` version field within a BA2 header
const VERSION_OFFSET: u64 = 4;

/// Whether a [`set_version`] call had to change the field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionChange {
    /// The field was rewritten from `from` to `to`
    Changed { from: u32, to: u32 },
    /// The field was already `version`; nothing was written
    Unchanged { version: u32 },
}

/// Set the BA2 at `path` to header version `new_version`, in place
pub fn set_version(path: &Utf8Path, new_version: u32) -> Result<VersionChange, Ba2Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| io_err(path, e))?;

    let mut head = [0u8; HEADER_LEN];
    file.read_exact(&mut head).map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => Ba2Error::TooShort,
        _ => Ba2Error::Io(io_err(path, e)),
    })?;
    let header = Ba2Header::parse(&head)?; // validates the BTDX magic

    if header.version == new_version {
        return Ok(VersionChange::Unchanged {
            version: new_version,
        });
    }

    file.seek(SeekFrom::Start(VERSION_OFFSET))
        .map_err(|e| io_err(path, e))?;
    file.write_all(&new_version.to_le_bytes())
        .map_err(|e| io_err(path, e))?;
    file.sync_data().map_err(|e| io_err(path, e))?;

    // Read the field back to confirm the write actually landed (these can be real game files)
    file.seek(SeekFrom::Start(VERSION_OFFSET))
        .map_err(|e| io_err(path, e))?;
    let mut check = [0u8; 4];
    file.read_exact(&mut check)
        .map_err(|e| Ba2Error::Io(io_err(path, e)))?;
    if u32::from_le_bytes(check) != new_version {
        return Err(Ba2Error::Io(io_err(
            path,
            std::io::Error::other("version field did not persist after write"),
        )));
    }

    Ok(VersionChange::Changed {
        from: header.version,
        to: new_version,
    })
}

#[cfg(test)]
#[path = "tests/patch.rs"]
mod tests;
