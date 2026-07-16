//! Lazy plugin provider resolution against deployment sources

use super::error::{PluginError, io_err};
use crate::apply::deploy_sources;
use crate::deploy::ProviderOrigin;
use crate::error::non_utf8;
use crate::instance::{Instance, Profile};
use camino::Utf8Path;
use walkdir::WalkDir;

/// Find the highest-priority deploy source that provides a top-level plugin filename
pub fn plugin_provider(
    instance: &Instance,
    profile: &Profile,
    filename: &str,
) -> Result<Option<ProviderOrigin>, PluginError> {
    for source in deploy_sources(instance, profile).iter().rev() {
        for entry in WalkDir::new(&source.staging_dir).min_depth(1).max_depth(1) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error)
                    if error.io_error().map(std::io::Error::kind)
                        == Some(std::io::ErrorKind::NotFound) =>
                {
                    break;
                }
                Err(error) => {
                    return Err(io_err(&source.staging_dir, error.into()).into());
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = Utf8Path::from_path(entry.path())
                .ok_or_else(|| PluginError::NonUtf8Path(non_utf8(entry.path())))?;
            if path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(filename))
            {
                return Ok(Some(source.origin.clone()));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "tests/provider.rs"]
mod tests;
