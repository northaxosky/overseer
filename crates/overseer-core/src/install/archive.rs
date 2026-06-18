use camino::Utf8Path;

use super::error::{InstallError, io_err};

/// An archive format Overseer can extract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveFormat {
    SevenZip,
    Zip,
}

impl ArchiveFormat {
    /// Every supported format. The one place variants are enumerated
    const ALL: &'static [Self] = &[Self::SevenZip, Self::Zip];

    /// The canonical lowercase extension - the source of truth
    fn extension(self) -> &'static str {
        match self {
            Self::SevenZip => "7z",
            Self::Zip => "zip",
        }
    }

    /// Recognize a format from a path's extension (case-insensitive)
    fn from_path(path: &Utf8Path) -> Option<Self> {
        let ext = path.extension()?.to_ascii_lowercase();
        Self::ALL.iter().copied().find(|f| f.extension() == ext)
    }

    /// Comma-separated supported extensions, for error messages
    pub(crate) fn supported_list() -> String {
        Self::ALL
            .iter()
            .map(|f| f.extension())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Extract a supported archive (`.7z` or `.zip`) into `dest`, creating it if needed
pub fn extract(archive: &Utf8Path, dest: &Utf8Path) -> Result<(), InstallError> {
    std::fs::create_dir_all(dest).map_err(|e| io_err(dest, e))?;

    let format =
        ArchiveFormat::from_path(archive).ok_or_else(|| InstallError::UnsupportedFormat {
            extension: archive.extension().unwrap_or_default().to_owned(),
        })?;

    match format {
        ArchiveFormat::SevenZip => extract_7z(archive, dest),
        ArchiveFormat::Zip => extract_zip(archive, dest),
    }
}

fn extract_7z(archive: &Utf8Path, dest: &Utf8Path) -> Result<(), InstallError> {
    sevenz_rust::decompress_file(archive.as_std_path(), dest.as_std_path()).map_err(|source| {
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
