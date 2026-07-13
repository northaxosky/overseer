//! validation and resolution of lifecycle archives in Downloads

use super::LifecycleError;
use crate::error::io_err;
use crate::install::{ArchiveFormat, InstallError};
use crate::instance::Instance;
use camino::{Utf8Path, Utf8PathBuf};
use std::io::ErrorKind;

/// Resolve one safe supported base name to a direct regular Downloads file
pub(super) fn resolve(instance: &Instance, name: &str) -> Result<Utf8PathBuf, LifecycleError> {
    if !safe_basename(name) {
        return Err(LifecycleError::InvalidArchiveName {
            name: name.to_owned(),
        });
    }
    let basename = Utf8Path::new(name);
    if ArchiveFormat::from_path(basename).is_none() {
        return Err(InstallError::UnsupportedFormat {
            extension: basename.extension().unwrap_or_default().to_owned(),
        }
        .into());
    }
    let archive = instance.downloads_dir().join(name);
    match std::fs::symlink_metadata(&archive) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(archive),
        Ok(_) => Err(LifecycleError::ArchiveUnavailable { path: archive }),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err(LifecycleError::ArchiveUnavailable { path: archive })
        }
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
