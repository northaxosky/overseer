//! Removal of an installed mod and its managed profile rows

use camino::{Utf8Path, Utf8PathBuf};

use super::bundle::{self, Manifest, ManifestProfile, Operation};
use super::{LifecycleError, LifecycleReport};
use crate::apply::{Deployment, InstanceLock};
use crate::fs;
use crate::instance::{Instance, ModKind, Profile, validate_mod_name};

struct PlannedProfile {
    profile: Profile,
    original: Option<String>,
    attempted: bool,
}

struct RemoveTransaction<'a> {
    instance: &'a Instance,
    bundle: Utf8PathBuf,
    actual: String,
    profiles: Vec<PlannedProfile>,
    moved: bool,
}

/// Remove one installed mod and every managed profile row that references it
pub fn remove(instance: &Instance, name: &str) -> Result<LifecycleReport, LifecycleError> {
    let _lock = InstanceLock::acquire(instance)?;
    let bundle_path = bundle::path(instance);
    if bundle::occupied(&bundle_path)? {
        return Err(LifecycleError::PendingOperation { path: bundle_path });
    }

    let deployment_path = Deployment::path(instance);
    if bundle::occupied(&deployment_path)? {
        return Err(LifecycleError::DeploymentExists {
            path: deployment_path,
        });
    }

    let (actual, profiles) = plan(instance, name)?;
    let manifest = Manifest {
        operation: Operation::Remove,
        mod_name: actual.clone(),
        archive: None,
        profiles: profiles
            .iter()
            .map(|planned| ManifestProfile {
                profile: planned.profile.name.clone(),
                original_modlist: planned.original.clone(),
            })
            .collect(),
    };

    let manifest_bytes = bundle::serialize(&bundle_path, &manifest)?;
    bundle::create(&bundle_path)?;

    RemoveTransaction {
        instance,
        bundle: bundle_path,
        actual,
        profiles,
        moved: false,
    }
    .run(&manifest_bytes)
}

/// Resolve the installed name and prepare every affected profile without mutation
fn plan(
    instance: &Instance,
    requested: &str,
) -> Result<(String, Vec<PlannedProfile>), LifecycleError> {
    validate_mod_name(requested)?;

    let actual = instance
        .installed_mods()?
        .into_iter()
        .find(|installed| installed.name.eq_ignore_ascii_case(requested))
        .map(|installed| installed.name)
        .ok_or_else(|| crate::instance::InstanceError::ModNotInstalled(requested.to_owned()))?;

    let mut names = instance.profiles()?;
    names.sort();
    let mut profiles = Vec::new();
    for name in names {
        let modlist = instance.profile_dir(&name).join("modlist.txt");

        let original = fs::read_to_string_opt(&modlist)?;

        let mut profile = Profile::load_existing(instance, &name)?;

        let before = profile.mods.len();

        profile.mods.retain(|entry| {
            entry.kind != ModKind::Managed || !entry.name.eq_ignore_ascii_case(&actual)
        });

        if profile.mods.len() != before {
            profiles.push(PlannedProfile {
                profile,
                original,
                attempted: false,
            });
        }
    }

    Ok((actual, profiles))
}

impl RemoveTransaction<'_> {
    /// Write the manifest, perform the removal, and clean the completed bundle
    fn run(mut self, manifest: &[u8]) -> Result<LifecycleReport, LifecycleError> {
        if let Err(error) = bundle::write_manifest(&self.bundle, manifest) {
            return Err(self.rollback(error.into()));
        }
        let live = self.instance.mods_dir().join(&self.actual);
        let old = self.bundle.join("old");
        if let Err(error) = bundle::rename_tree(&live, &old) {
            return Err(self.rollback(error.into()));
        }
        self.moved = true;

        for index in 0..self.profiles.len() {
            self.profiles[index].attempted = true;

            #[cfg(test)]
            {
                let path = self
                    .instance
                    .profile_dir(&self.profiles[index].profile.name)
                    .join("modlist.txt");
                if let Err(error) = super::failpoint::check(super::failpoint::Point::Save, &path) {
                    return Err(self.rollback(error.into()));
                }
            }

            if let Err(error) = self.profiles[index].profile.save_modlist(self.instance) {
                return Err(self.rollback(error.into()));
            }
        }

        let residue_warning = bundle::cleanup(&self.bundle)
            .err()
            .map(|_| self.bundle.clone());

        Ok(LifecycleReport {
            name: self.actual,
            archive: None,
            residue_warning,
        })
    }

    /// Attempt every completed inverse and preserve the bundle on any failure
    fn rollback(self, initiating: LifecycleError) -> LifecycleError {
        let mut failures = Vec::new();
        for planned in self.profiles.iter().filter(|planned| planned.attempted) {
            let path = self
                .instance
                .profile_dir(&planned.profile.name)
                .join("modlist.txt");

            if let Err(error) = restore_profile(&path, planned.original.as_deref()) {
                failures.push(format!("restore profile modlist `{path}`: {error}"));
            }
        }

        if self.moved {
            let old = self.bundle.join("old");
            let live = self.instance.mods_dir().join(&self.actual);
            if let Err(error) = bundle::rename_tree(&old, &live) {
                failures.push(format!("restore mod tree `{old}` to `{live}`: {error}"));
            }
        }

        if failures.is_empty()
            && let Err(error) = bundle::cleanup(&self.bundle)
        {
            failures.push(format!("remove rollback bundle `{}`: {error}", self.bundle));
        }
        if failures.is_empty() {
            return initiating;
        }
        let mut issues = vec![format!("initiating error: {initiating}")];
        issues.extend(failures);

        LifecycleError::RollbackIncomplete {
            bundle: self.bundle,
            issues,
        }
    }
}

/// Restore one exact supported original modlist
fn restore_profile(path: &Utf8Path, original: Option<&str>) -> Result<(), crate::IoError> {
    #[cfg(test)]
    super::failpoint::check(super::failpoint::Point::Restore, path)?;

    match original {
        Some(text) => fs::write_atomic(path, text.as_bytes()),
        None => fs::remove_file_opt(path),
    }
}
