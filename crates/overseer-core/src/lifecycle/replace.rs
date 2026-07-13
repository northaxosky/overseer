//! Guarded replacement with exact in-process tree-swap rollback

use super::{LifecycleError, LifecycleReport, archive, bundle};
use crate::instance::Instance;

/// Replace an installed mod from one direct Downloads archive
pub fn replace(
    instance: &Instance,
    name: &str,
    archive_name: &str,
) -> Result<LifecycleReport, LifecycleError> {
    let _lock = super::enter(instance)?;
    let actual = super::find_installed(instance, name)?
        .ok_or_else(|| crate::instance::InstanceError::ModNotInstalled(name.to_owned()))?;
    let archive = archive::resolve(instance, archive_name)?;
    let pending = instance.pending_mod_operation_dir();
    let live = instance.mods_dir().join(&actual);
    let old = pending.join("old");
    let candidate = pending.join("new");

    bundle::create(&pending)?;
    if let Err(error) = crate::install::prepare_candidate(&archive, &pending) {
        return Err(super::finish_rollback(pending, error.into(), Vec::new()));
    }
    if let Err(error) = bundle::rename_tree(&live, &old) {
        return Err(super::finish_rollback(pending, error.into(), Vec::new()));
    }
    if let Err(initiating) = bundle::rename_tree(&candidate, &live) {
        let mut failures = Vec::new();
        if let Err(error) = bundle::rename_tree(&old, &live) {
            failures.push(format!("restore old tree `{old}` to `{live}`: {error}"));
        }
        return Err(super::finish_rollback(pending, initiating.into(), failures));
    }

    let residue_warning = bundle::cleanup(&pending).err().map(|_| pending.clone());

    Ok(LifecycleReport {
        name: actual,
        residue_warning,
    })
}
