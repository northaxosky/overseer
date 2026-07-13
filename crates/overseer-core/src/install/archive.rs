//! Extracting `.7z` and `.zip` archives

use crate::fs;
use camino::Utf8Path;
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use super::error::InstallError;
use crate::error::io_err;

/// Archives whose declared output stays under this are never bomb-checked
const BOMB_FLOOR_BYTES: u64 = 100 * 1024 * 1024;

/// Output beyond this multiple of the archive's own on-disk size is considered a bomb
const BOMB_RATIO: u64 = 100;

/// An archive format Overseer can extract
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, EnumString, IntoStaticStr)]
pub(crate) enum ArchiveFormat {
    #[strum(serialize = "7z")]
    SevenZip,
    #[strum(serialize = "zip")]
    Zip,
}

impl ArchiveFormat {
    /// The canonical lowercase extension - the source of truth
    fn extension(self) -> &'static str {
        self.into()
    }

    /// Recognize a format from a path's extension (case-insensitive)
    pub(crate) fn from_path(path: &Utf8Path) -> Option<Self> {
        path.extension()?.to_ascii_lowercase().parse().ok()
    }

    /// Comma-separated supported extensions, for error messages
    pub(crate) fn supported_list() -> String {
        Self::iter()
            .map(|f| f.extension())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Extract a supported archive (`.7z` or `.zip`) into `dest`, creating it if needed
pub(super) fn extract(archive: &Utf8Path, dest: &Utf8Path) -> Result<(), InstallError> {
    fs::ensure_dir(dest)?;

    let format =
        ArchiveFormat::from_path(archive).ok_or_else(|| InstallError::UnsupportedFormat {
            extension: archive.extension().unwrap_or_default().to_owned(),
        })?;

    guard_decompression_bomb(archive, format)?;

    match format {
        ArchiveFormat::SevenZip => extract_7z(archive, dest),
        ArchiveFormat::Zip => extract_zip(archive, dest),
    }
}

fn extract_7z(archive: &Utf8Path, dest: &Utf8Path) -> Result<(), InstallError> {
    sevenz_rust2::decompress_file(archive.as_std_path(), dest.as_std_path()).map_err(|source| {
        InstallError::SevenZip {
            path: archive.to_owned(),
            source,
        }
    })
}

fn extract_zip(archive: &Utf8Path, dest: &Utf8Path) -> Result<(), InstallError> {
    let file = std::fs::File::open(archive).map_err(|e| io_err(archive, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|source| InstallError::Zip {
        path: archive.to_owned(),
        source,
    })?;
    zip.extract(dest.as_std_path())
        .map_err(|source| InstallError::Zip {
            path: archive.to_owned(),
            source,
        })
}

/// Whether declared output is an implausible expansion of the archive's size (BOMBBBBB)
fn is_bomb(uncompressed: u64, compressed: u64) -> bool {
    uncompressed > BOMB_FLOOR_BYTES && uncompressed > compressed.saturating_mul(BOMB_RATIO)
}

/// Refuse an archive whose declared output mogs its on-disk size
fn guard_decompression_bomb(archive: &Utf8Path, format: ArchiveFormat) -> Result<(), InstallError> {
    let uncompressed = declared_uncompressed(archive, format)?;
    let compressed = std::fs::metadata(archive)
        .map_err(|e| io_err(archive, e))?
        .len();
    if is_bomb(uncompressed, compressed) {
        return Err(InstallError::TooLarge {
            path: archive.to_owned(),
            uncompressed,
            compressed,
        });
    }
    Ok(())
}

/// The total uncompressed size the archive's metadata declares
fn declared_uncompressed(archive: &Utf8Path, format: ArchiveFormat) -> Result<u64, InstallError> {
    match format {
        ArchiveFormat::Zip => {
            let file = std::fs::File::open(archive).map_err(|e| io_err(archive, e))?;
            let zip = zip::ZipArchive::new(file).map_err(|source| InstallError::Zip {
                path: archive.to_owned(),
                source,
            })?;
            Ok(zip.decompressed_size().unwrap_or(0).min(u64::MAX as u128) as u64)
        }
        ArchiveFormat::SevenZip => {
            let mut file = std::fs::File::open(archive).map_err(|e| io_err(archive, e))?;
            let meta = sevenz_rust2::Archive::read(&mut file, &sevenz_rust2::Password::empty())
                .map_err(|source| InstallError::SevenZip {
                    path: archive.to_owned(),
                    source,
                })?;
            Ok(meta
                .files
                .iter()
                .map(|e| e.size())
                .fold(0u64, u64::saturating_add))
        }
    }
}

#[cfg(test)]
#[path = "tests/archive.rs"]
mod tests;
