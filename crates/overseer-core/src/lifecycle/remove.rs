//! Guarded removal of an installed mod tree

use super::bundle;
use super::{LifecycleError, LifecycleReport};
use crate::instance::Instance;

/// Remove one installed mod without mutating profiles
pub fn remove(instance: &Instance, name: &str) -> Result<LifecycleReport, LifecycleError> {
    let _lock = super::enter(instance)?;
    let actual = super::find_installed(instance, name)?
        .ok_or_else(|| crate::instance::InstanceError::ModNotInstalled(name.to_owned()))?;
    let pending = instance.pending_mod_operation_dir();
    let live = instance.mods_dir().join(&actual);
    let old = pending.join("old");

    bundle::create(&pending)?;
    if let Err(error) = bundle::rename_tree(&live, &old) {
        return Err(super::finish_rollback(pending, error.into(), Vec::new()));
    }

    let residue_warning = bundle::cleanup(&pending).err().map(|_| pending.clone());

    Ok(LifecycleReport {
        name: actual,
        residue_warning,
    })
}
