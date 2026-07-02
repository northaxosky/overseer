//! Patching Bethesda archives in place.
//!
//! [`set_version`] is the game-agnostic mechanism — it flips a BA2's header version field and
//! nothing else. Per-game policy (which version means which edition, which transitions are
//! valid) lives in submodules like [`fallout4`].

pub mod delta;
pub mod fallout4;

use crate::archive::{Ba2Error, Ba2Header, HEADER_LEN};
use crate::error::IoError;
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
        .map_err(|e| IoError::new(path, e))?;

    let mut head = [0u8; HEADER_LEN];
    file.read_exact(&mut head).map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => Ba2Error::TooShort,
        _ => Ba2Error::Io(IoError::new(path, e)),
    })?;
    let header = Ba2Header::parse(&head)?; // validates the BTDX magic

    if header.version == new_version {
        return Ok(VersionChange::Unchanged {
            version: new_version,
        });
    }

    file.seek(SeekFrom::Start(VERSION_OFFSET))
        .map_err(|e| IoError::new(path, e))?;
    file.write_all(&new_version.to_le_bytes())
        .map_err(|e| IoError::new(path, e))?;
    file.sync_data().map_err(|e| IoError::new(path, e))?;

    // Read the field back to confirm the write actually landed (these can be real game files).
    file.seek(SeekFrom::Start(VERSION_OFFSET))
        .map_err(|e| IoError::new(path, e))?;
    let mut check = [0u8; 4];
    file.read_exact(&mut check)
        .map_err(|e| Ba2Error::Io(IoError::new(path, e)))?;
    if u32::from_le_bytes(check) != new_version {
        return Err(Ba2Error::Io(IoError::new(
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
mod tests {
    use super::*;
    use crate::archive::Ba2Error;
    use crate::test_support::{ba2_bytes, temp};
    use camino::{Utf8Path, Utf8PathBuf};

    fn write_ba2(root: &Utf8Path, version: u32, tag: &[u8; 4], body: &[u8]) -> Utf8PathBuf {
        let path = root.join("test.ba2");
        std::fs::write(&path, ba2_bytes(version, tag, body)).expect("write ba2");
        path
    }

    #[test]
    fn flips_only_the_version_field() {
        let body = b"archive body that must survive untouched";
        let (_tmp, root) = temp();
        let path = write_ba2(&root, 8, b"GNRL", body);
        let original = std::fs::read(&path).unwrap();

        assert_eq!(
            set_version(&path, 1).unwrap(),
            VersionChange::Changed { from: 8, to: 1 }
        );

        let patched = std::fs::read(&path).unwrap();
        assert_eq!(&patched[4..8], 1u32.to_le_bytes().as_slice());
        // Restoring just the version field reproduces the original byte-for-byte.
        let mut restored = patched.clone();
        restored[4..8].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(restored, original);
    }

    #[test]
    fn unchanged_when_already_at_the_value() {
        let (_tmp, root) = temp();
        let path = write_ba2(&root, 1, b"GNRL", b"body");
        let original = std::fs::read(&path).unwrap();
        assert_eq!(
            set_version(&path, 1).unwrap(),
            VersionChange::Unchanged { version: 1 }
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn sets_any_value_without_judging_it() {
        // The mechanism is game-agnostic: which versions are valid is the caller's policy.
        let (_tmp, root) = temp();
        let path = write_ba2(&root, 8, b"DX10", b"body");
        assert_eq!(
            set_version(&path, 3).unwrap(),
            VersionChange::Changed { from: 8, to: 3 }
        );
        assert_eq!(
            &std::fs::read(&path).unwrap()[4..8],
            3u32.to_le_bytes().as_slice()
        );
    }

    #[test]
    fn rejects_a_non_ba2_file() {
        let (_tmp, root) = temp();
        let path = root.join("x.ba2");
        std::fs::write(&path, b"NOPE plus enough trailing bytes to fill a header").unwrap();
        assert!(matches!(set_version(&path, 1), Err(Ba2Error::BadMagic)));
    }

    #[test]
    fn rejects_a_too_short_file() {
        let (_tmp, root) = temp();
        let path = root.join("x.ba2");
        std::fs::write(&path, b"BTDX").unwrap();
        assert!(matches!(set_version(&path, 1), Err(Ba2Error::TooShort)));
    }
}
