//! Transactional replacement and provenance-backed reinstallation

use super::archive::{self, Downloaded};
use super::bundle::{self, Manifest, Operation};
use super::{LifecycleError, LifecycleReport};
use crate::instance::Instance;
use camino::Utf8Path;

/// Replace an installed mod from an explicit archive without reading profiles
pub fn replace(
    instance: &Instance,
    name: &str,
    source: &Utf8Path,
) -> Result<LifecycleReport, LifecycleError> {
    replace_with(instance, name, source, |_| Ok(()))
}

/// Replace through an injectable post-publish operation
pub(super) fn replace_with(
    instance: &Instance,
    name: &str,
    source: &Utf8Path,
    after_publish: impl FnOnce(&Utf8Path) -> Result<(), LifecycleError>,
) -> Result<LifecycleReport, LifecycleError> {
    let _lock = super::enter(instance)?;
    let actual = require_installed(instance, name)?;
    let download = archive::import(instance, source)?;

    replace_locked_with(
        instance,
        actual,
        download,
        Operation::Replace,
        after_publish,
        bundle::cleanup,
    )
}

/// Reinstall an installed mod from its strictly validated provenance
pub fn reinstall(instance: &Instance, name: &str) -> Result<LifecycleReport, LifecycleError> {
    reinstall_with(instance, name, bundle::cleanup)
}

/// Reinstall through an injectable success cleanup operation
pub(super) fn reinstall_with(
    instance: &Instance,
    name: &str,
    cleanup: impl FnOnce(&Utf8Path) -> Result<(), crate::IoError>,
) -> Result<LifecycleReport, LifecycleError> {
    let _lock = super::enter(instance)?;
    let actual = require_installed(instance, name)?;
    let download = archive::resolve(instance, &actual)?;

    replace_locked_with(
        instance,
        actual,
        download,
        Operation::Reinstall,
        |_| Ok(()),
        cleanup,
    )
}

/// Resolve one required installed identity with its actual spelling
fn require_installed(instance: &Instance, name: &str) -> Result<String, LifecycleError> {
    super::find_installed(instance, name)?
        .ok_or_else(|| crate::instance::InstanceError::ModNotInstalled(name.to_owned()).into())
}

/// Run one replace transaction while the caller retains the instance lock
fn replace_locked_with(
    instance: &Instance,
    actual: String,
    download: Downloaded,
    operation: Operation,
    after_publish: impl FnOnce(&Utf8Path) -> Result<(), LifecycleError>,
    cleanup: impl FnOnce(&Utf8Path) -> Result<(), crate::IoError>,
) -> Result<LifecycleReport, LifecycleError> {
    let pending = bundle::path(instance);
    let manifest = bundle::serialize(
        &pending,
        &Manifest {
            operation,
            mod_name: actual.clone(),
            archive: Some(download.name.clone()),
            profiles: Vec::new(),
        },
    )?;

    bundle::create(&pending)?;
    let live = instance.mods_dir().join(&actual);
    let old = pending.join("old");
    let candidate = pending.join("new");
    let mut old_moved = false;
    let mut published = false;

    let result = (|| {
        bundle::write_manifest(&pending, &manifest)?;
        crate::install::prepare_candidate(&download.path, &pending)?;
        archive::stamp(&candidate, &download.name)?;
        bundle::rename_tree(&live, &old)?;
        old_moved = true;
        bundle::rename_tree(&candidate, &live)?;
        published = true;
        after_publish(&live)?;

        Ok::<(), LifecycleError>(())
    })();

    if let Err(initiating) = result {
        let mut failures = Vec::new();
        if published && let Err(error) = bundle::rename_tree(&live, &candidate) {
            failures.push(format!(
                "restore candidate `{live}` to `{candidate}`: {error}"
            ));
        }
        if old_moved && let Err(error) = bundle::rename_tree(&old, &live) {
            failures.push(format!("restore old tree `{old}` to `{live}`: {error}"));
        }
        return Err(super::finish_rollback(pending, initiating, failures));
    }
    let residue_warning = cleanup(&pending).err().map(|_| pending);

    Ok(LifecycleReport {
        name: actual,
        archive: Some(download.name),
        residue_warning,
    })
}
