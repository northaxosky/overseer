//! Downloads import and installed-mod provenance

use super::LifecycleError;
use crate::error::io_err;
use crate::install::{ArchiveFormat, InstallError};
use crate::instance::Instance;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};

pub(super) const PROVENANCE: &str = ".overseer-mod.toml";

pub(super) struct Downloaded {
    pub(super) name: String,
    pub(super) path: Utf8PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    format: u32,
    archive: String,
}

/// Reuse a direct download or copy an external archive without overwriting
pub(super) fn import(instance: &Instance, source: &Utf8Path) -> Result<Downloaded, LifecycleError> {
    let name = source
        .file_name()
        .filter(|name| safe_basename(name))
        .ok_or_else(|| LifecycleError::InvalidArchive {
            path: source.to_owned(),
        })?
        .to_owned();
    if ArchiveFormat::from_path(Utf8Path::new(&name)).is_none() {
        return Err(InstallError::UnsupportedFormat {
            extension: source.extension().unwrap_or_default().to_owned(),
        }
        .into());
    }
    let downloads = instance.downloads_dir();
    let path = downloads.join(&name);

    if let Some(parent) = source.parent()
        && same_directory(parent, &downloads)?
    {
        let metadata = std::fs::symlink_metadata(source).map_err(|error| io_err(source, error))?;
        if !metadata.file_type().is_file() {
            return Err(LifecycleError::InvalidArchive {
                path: source.to_owned(),
            });
        }
        return Ok(Downloaded { name, path });
    }
    crate::fs::ensure_dir(&downloads)?;
    copy_new(source, &path)?;

    Ok(Downloaded { name, path })
}

/// Compare existing directories by filesystem identity
fn same_directory(left: &Utf8Path, right: &Utf8Path) -> Result<bool, LifecycleError> {
    match same_file::is_same_file(left, right) {
        Ok(same) => Ok(same),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_err(left, error).into()),
    }
}

/// Stamp exact Overseer provenance onto a prepared candidate
pub(super) fn stamp(candidate: &Utf8Path, archive: &str) -> Result<(), LifecycleError> {
    let provenance = toml::to_string(&Provenance {
        format: 1,
        archive: archive.to_owned(),
    })
    .expect("fixed provenance schema serializes");
    crate::fs::write(&candidate.join(PROVENANCE), provenance)?;

    Ok(())
}

/// Resolve the strictly parsed provenance archive for an installed mod
pub(super) fn resolve(instance: &Instance, actual: &str) -> Result<Downloaded, LifecycleError> {
    let path = instance.mods_dir().join(actual).join(PROVENANCE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(LifecycleError::MissingProvenance {
                name: actual.to_owned(),
                path,
            });
        }
        Err(error) => {
            return Err(io_err(&path, error).into());
        }
    };

    let invalid = |reason| LifecycleError::InvalidProvenance {
        name: actual.to_owned(),
        path: path.clone(),
        reason,
    };
    let value: Provenance = toml::from_str(&text).map_err(|error| invalid(error.to_string()))?;
    if value.format != 1
        || !safe_basename(&value.archive)
        || ArchiveFormat::from_path(Utf8Path::new(&value.archive)).is_none()
    {
        return Err(invalid(
            "expected format 1 and one safe `.zip` or `.7z` basename".to_owned(),
        ));
    }

    let archive = instance.downloads_dir().join(&value.archive);

    match std::fs::symlink_metadata(&archive) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Downloaded {
            name: value.archive,
            path: archive,
        }),
        Ok(_) => Err(LifecycleError::MissingArchive {
            name: actual.to_owned(),
            path: archive,
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Err(LifecycleError::MissingArchive {
            name: actual.to_owned(),
            path: archive,
        }),
        Err(error) => Err(io_err(&archive, error).into()),
    }
}

/// Decide whether a string is one plain UTF-8 path basename
fn safe_basename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        && !name.chars().any(char::is_control)
        && Utf8Path::new(name).file_name() == Some(name)
}

/// Stream one archive into a newly owned Downloads entry
fn copy_new(source: &Utf8Path, destination: &Utf8Path) -> Result<(), LifecycleError> {
    copy_new_with(
        source,
        destination,
        |input, output| {
            std::io::copy(input, output)?;
            output.flush()
        },
        crate::fs::remove_file_opt,
    )
}

/// Copy into a newly owned destination through injectable operation seams
fn copy_new_with(
    source: &Utf8Path,
    destination: &Utf8Path,
    copy: impl FnOnce(&mut File, &mut File) -> std::io::Result<()>,
    cleanup: impl FnOnce(&Utf8Path) -> Result<(), crate::IoError>,
) -> Result<(), LifecycleError> {
    let mut input = File::open(source).map_err(|error| io_err(source, error))?;
    let mut output = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err(LifecycleError::DownloadCollision {
                path: destination.to_owned(),
            });
        }
        Err(error) => {
            return Err(io_err(destination, error).into());
        }
    };

    let copied = copy(&mut input, &mut output).map_err(|error| io_err(destination, error));
    drop(output);
    if let Err(copy) = copied {
        if let Err(cleanup) = cleanup(destination) {
            return Err(LifecycleError::PartialCopy {
                path: destination.to_owned(),
                copy: format!("{copy}: {}", copy.source,),
                cleanup: format!("{cleanup}: {}", cleanup.source,),
            });
        }
        return Err(copy.into());
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/archive_copy.rs"]
mod tests;
