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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::File;

    use crate::test_support::temp;

    /// A normal `.7z` extracts through `extract`, covering the 7z happy path on the backend.
    #[test]
    fn extracts_a_normal_7z_archive() {
        let (_t, base) = temp();
        let src = base.join("src");
        std::fs::create_dir_all(src.join("Textures")).expect("mk src");
        std::fs::write(src.join("Textures/a.dds"), b"tex").expect("write tex");
        std::fs::write(src.join("Cool.esp"), b"plugin").expect("write esp");
        let archive = base.join("mod.7z");
        sevenz_rust2::compress_to_path(src.as_std_path(), archive.as_std_path()).expect("compress");

        let dest = base.join("dest");
        extract(&archive, &dest).expect("extract");

        assert_eq!(
            std::fs::read_to_string(dest.join("Textures/a.dds")).unwrap(),
            "tex"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("Cool.esp")).unwrap(),
            "plugin"
        );
    }

    /// A crafted `.7z` whose entry name escapes the destination is rejected, writing nothing
    /// outside `dest`. Regression for the sevenz-rust 0.6 path-traversal CVE (RUSTSEC-2023-0086);
    /// `sevenz-rust2` confines each entry under `dest`, and `extract` surfaces that as an error.
    #[test]
    fn rejects_a_path_traversal_7z_archive() {
        let (_t, base) = temp();
        // A real payload the malicious entry points at; its declared name escapes `dest`.
        let payload = base.join("payload.txt");
        std::fs::write(&payload, b"pwned").expect("write payload");

        let archive = base.join("evil.7z");
        let entry = sevenz_rust2::ArchiveEntry::from_path(
            payload.as_std_path(),
            "../escape.txt".to_owned(),
        );
        let mut writer =
            sevenz_rust2::ArchiveWriter::new(File::create(&archive).expect("create archive"))
                .expect("archive writer");
        writer
            .push_archive_entry(entry, Some(File::open(&payload).expect("open payload")))
            .expect("push entry");
        writer.finish().expect("finish archive");

        let dest = base.join("dest");
        let err = extract(&archive, &dest).expect_err("traversal must be rejected");
        assert!(matches!(err, InstallError::SevenZip { .. }), "got {err:?}");

        // The escape target (a sibling of `dest`) must never be created.
        assert!(
            !base.join("escape.txt").exists(),
            "a file escaped the destination directory"
        );
    }
}
