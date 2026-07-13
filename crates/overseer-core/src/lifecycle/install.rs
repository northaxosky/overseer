//! Guarded publication of a new installed mod

use super::archive;
use super::bundle;
use super::{LifecycleError, LifecycleReport};
use crate::instance::Instance;

/// Install one direct Downloads archive without mutating profiles
pub fn install(
    instance: &Instance,
    archive_name: &str,
    name: &str,
) -> Result<LifecycleReport, LifecycleError> {
    let _lock = super::enter(instance)?;
    if super::find_installed(instance, name)?.is_some() {
        return Err(crate::instance::InstanceError::ModAlreadyInstalled(name.to_owned()).into());
    }

    let archive = archive::resolve(instance, archive_name)?;
    let pending = instance.pending_mod_operation_dir();
    bundle::create(&pending)?;
    let live = instance.mods_dir().join(name);
    let candidate = pending.join("new");

    let result = (|| {
        crate::install::prepare_candidate(&archive, &pending)?;
        crate::fs::ensure_dir(&instance.mods_dir())?;
        bundle::rename_tree(&candidate, &live)?;
        Ok::<(), LifecycleError>(())
    })();

    if let Err(initiating) = result {
        return Err(super::finish_rollback(pending, initiating, Vec::new()));
    }

    let residue_warning = bundle::cleanup(&pending).err().map(|_| pending);

    Ok(LifecycleReport {
        name: name.to_owned(),
        residue_warning,
    })
}
