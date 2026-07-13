//! Transactional installation into one active profile

use camino::Utf8Path;

use super::archive;
use super::bundle::{self, Manifest, ManifestProfile, Operation};
use super::{LifecycleError, LifecycleReport};
use crate::instance::{Instance, InstanceError, Profile};

/// Install an archive and add one disabled row to the active profile
pub fn install(
    instance: &Instance,
    active_profile: &str,
    source: &Utf8Path,
    name: &str,
) -> Result<LifecycleReport, LifecycleError> {
    install_with(
        instance,
        active_profile,
        source,
        name,
        Profile::save_modlist,
    )
}

/// Install through an injectable profile save operation
pub(super) fn install_with(
    instance: &Instance,
    active_profile: &str,
    source: &Utf8Path,
    name: &str,
    save: impl FnOnce(&Profile, &Instance) -> Result<(), InstanceError>,
) -> Result<LifecycleReport, LifecycleError> {
    let _lock = super::enter(instance)?;
    if super::find_installed(instance, name)?.is_some() {
        return Err(crate::instance::InstanceError::ModAlreadyInstalled(name.to_owned()).into());
    }

    let modlist = instance.profile_dir(active_profile).join("modlist.txt");
    let original = crate::fs::read_to_string_opt(&modlist)?;
    let mut profile = Profile::load_existing(instance, active_profile)?;
    profile.add(name, false)?;

    let download = archive::import(instance, source)?;
    let pending = bundle::path(instance);
    let manifest = bundle::serialize(
        &pending,
        &Manifest {
            operation: Operation::Install,
            mod_name: name.to_owned(),
            archive: Some(download.name.clone()),
            profiles: vec![ManifestProfile {
                profile: active_profile.to_owned(),
                original_modlist: original.clone(),
            }],
        },
    )?;

    bundle::create(&pending)?;
    let live = instance.mods_dir().join(name);
    let candidate = pending.join("new");
    let mut attempted = false;
    let mut published = false;

    let result = (|| {
        bundle::write_manifest(&pending, &manifest)?;
        crate::install::prepare_candidate(&download.path, &pending)?;
        archive::stamp(&candidate, &download.name)?;
        crate::fs::ensure_dir(&instance.mods_dir())?;
        bundle::rename_tree(&candidate, &live)?;

        published = true;
        attempted = true;
        save(&profile, instance)?;

        Ok::<(), LifecycleError>(())
    })();

    if let Err(initiating) = result {
        let mut failures = Vec::new();

        if attempted
            && let Err(error) = super::remove::restore_profile(&modlist, original.as_deref())
        {
            failures.push(format!("restore profile modlist `{modlist}`: {error}"));
        }
        if published && let Err(error) = bundle::rename_tree(&live, &candidate) {
            failures.push(format!(
                "restore candidate `{live}` to `{candidate}`: {error}"
            ));
        }
        return Err(super::finish_rollback(pending, initiating, failures));
    }

    let residue_warning = bundle::cleanup(&pending).err().map(|_| pending);

    Ok(LifecycleReport {
        name: name.to_owned(),
        archive: Some(download.name),
        residue_warning,
    })
}
