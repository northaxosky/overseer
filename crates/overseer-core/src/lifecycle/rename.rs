//! Transactional rename preparation and happy path

use super::journal::{self, Journal, Operation, Phase, ProfileSnapshot};
use super::recovery::{cleanup, create_marker, marker_present};
use super::{
    CleanupWarning, LifecycleError, LifecycleOutcome, RenameReport, conflict, exists, modlist,
    work_path,
};
use crate::instance::{Instance, InstanceError, ModKind, Profile};
use camino::Utf8Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn rename_locked(
    instance: &Instance,
    old: &str,
    new: &str,
) -> Result<LifecycleOutcome<RenameReport>, LifecycleError> {
    let mut journal = prepare(instance, old, new)?;
    #[cfg(test)]
    super::tests::trip(super::tests::Failpoint::Active)?;
    let Operation::Rename { old, new } = journal.operation.clone();
    let source = instance.mods_dir().join(&old);
    let private = work_path(instance).join("old");

    create_marker(&source, &journal.transaction)?;
    #[cfg(test)]
    super::tests::trip(super::tests::Failpoint::MarkerCreated)?;
    rename_no_replace(&source, &private)?;
    #[cfg(test)]
    super::tests::trip(super::tests::Failpoint::OldMoved)?;
    rename_no_replace(&private, &instance.mods_dir().join(&new))?;
    #[cfg(test)]
    super::tests::trip(super::tests::Failpoint::NewPublished)?;

    for snapshot in &journal.profiles {
        let path = modlist(instance, &snapshot.profile);
        if crate::fs::read_to_string_opt(&path)? != snapshot.original {
            return conflict(path);
        }
        crate::fs::write_atomic(&path, snapshot.intended.as_bytes())?;
        #[cfg(test)]
        super::tests::trip(super::tests::Failpoint::ProfileWritten)?;
    }

    journal.phase = Phase::Committed;
    if let Err(error) = journal::save(instance, &journal)
        && !journal::load(instance)?.is_some_and(|saved| saved.phase == Phase::Committed)
    {
        return Err(error);
    }
    let report = RenameReport { old, new };

    Ok(LifecycleOutcome {
        report,
        cleanup_warning: cleanup(instance, &journal)
            .err()
            .map(|blocked_path| CleanupWarning { blocked_path }),
    })
}

fn prepare(instance: &Instance, old: &str, new: &str) -> Result<Journal, LifecycleError> {
    crate::instance::validate_mod_name(old)?;
    crate::instance::validate_mod_name(new)?;
    let installed = instance.installed_mods()?;

    let old = installed
        .iter()
        .find(|item| item.name.eq_ignore_ascii_case(old))
        .map(|item| item.name.clone())
        .ok_or_else(|| InstanceError::ModNotInstalled(old.to_owned()))?;

    if old == new {
        return Err(
            InstanceError::InvalidModName("new name matches the old name".to_owned()).into(),
        );
    }
    let dest = instance.mods_dir().join(new);
    let occupied = installed
        .iter()
        .any(|item| item.name != old && item.name.eq_ignore_ascii_case(new));
    let case_only = cfg!(windows) && old.eq_ignore_ascii_case(new);

    if occupied || (!case_only && exists(&dest)?) {
        return Err(InstanceError::ModAlreadyInstalled(new.to_owned()).into());
    }

    let transaction = transaction_id();
    let source = instance.mods_dir().join(&old);
    if marker_present(&source, &transaction)? {
        return conflict(source);
    }

    let mut snapshots = Vec::new();

    for name in instance.profiles()? {
        let mut profile = Profile::load_existing(instance, &name)?;
        let is_source = |entry: &crate::instance::ModListEntry| {
            entry.kind == ModKind::Managed && entry.name.eq_ignore_ascii_case(&old)
        };
        if profile
            .mods
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(new) && !is_source(entry))
        {
            return Err(InstanceError::ModAlreadyInList(new.to_owned()).into());
        }
        if !profile.mods.iter().any(is_source) {
            continue;
        }
        let original = crate::fs::read_to_string_opt(&modlist(instance, &name))?;

        profile
            .mods
            .iter_mut()
            .filter(|entry| entry.kind == ModKind::Managed && entry.name.eq_ignore_ascii_case(&old))
            .for_each(|entry| {
                entry.name = new.to_owned();
            });

        snapshots.push(ProfileSnapshot {
            profile: name,
            original,
            intended: profile.to_modlist_string(),
        });
    }

    let work = work_path(instance);
    if exists(&work)? {
        return conflict(work);
    }
    let journal = Journal {
        version: 1,
        transaction,
        phase: Phase::Active,
        operation: Operation::Rename {
            old,
            new: new.to_owned(),
        },
        profiles: snapshots,
    };

    journal::save(instance, &journal)?;
    crate::fs::ensure_dir(&work)?;
    Ok(journal)
}

/// Rename a path only when its destination is absent
pub(super) fn rename_no_replace(from: &Utf8Path, to: &Utf8Path) -> Result<(), LifecycleError> {
    if exists(to)? {
        return conflict(to.to_owned());
    }
    #[cfg(test)]
    super::tests::inject_rename_race(to)?;
    match atomicwrites::move_atomic(from.as_std_path(), to.as_std_path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => conflict(to.to_owned()),
        Err(_) if exists(to)? => conflict(to.to_owned()),
        Err(error) => Err(crate::error::io_err(from, error).into()),
    }
}

fn transaction_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}
