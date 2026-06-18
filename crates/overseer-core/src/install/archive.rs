use super::error::InstallError;
use camino::Utf8Path;

/// Extract a supported archive into `dest`, which is created if needed
pub fn extract(archive: &Utf8Path, dest: &Utf8Path) -> Result<(), InstallError> {
    std::fs::create_dir_all(dest).map_err(|e| super::error::io_err(dest, e))?;

    match archive.extension().map(str::to_ascii_lowercase).as_deref() {
        Some("7z") => extract_7z(archive, dest),
        Some("zip") => extract_zip(archive, dest),
        other => Err(InstallError::UnsupportedFormat(
            other.unwrap_or_default().to_owned(),
        )),
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
    let file = std::fs::File::open(archive).map_err(|e| super::error::io_err(archive, e))?;
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
