//! Lifecycle rollback and terminal cleanup

use super::journal::{self, Journal, Operation, Phase};
use super::rename::rename_no_replace;
use super::{
    CleanupWarning, LifecycleError, MARKER_PREFIX, conflict, exists, marker_path, modlist,
    work_path,
};
use crate::instance::Instance;
use camino::{Utf8Path, Utf8PathBuf};
use std::ffi::OsStr;

/// Recover a pending lifecycle journal while the instance lock is held
pub(crate) fn recover_locked(instance: &Instance) -> Result<(), LifecycleError> {
    let Some(mut journal) = journal::load(instance)? else {
        return Ok(());
    };
    journal::validate(instance, &journal)?;

    if journal.phase == Phase::Active {
        rollback_tree(instance, &journal)?;
        restore_profiles(instance, &journal)?;
        journal.phase = Phase::RolledBack;
        journal::save(instance, &journal)?;
    }

    match journal.phase {
        Phase::RolledBack => cleanup(instance, &journal)
            .map_err(|path| LifecycleError::RecoveryConflict { paths: vec![path] }),
        Phase::Committed => cleanup(instance, &journal).map_err(|blocked_path| {
            LifecycleError::CleanupPending(CleanupWarning { blocked_path })
        }),
        Phase::Active => unreachable!("active recovery always reaches a terminal phase"),
    }
}

/// Restore public rename slots to their pre-transaction layout
fn rollback_tree(instance: &Instance, journal: &Journal) -> Result<(), LifecycleError> {
    let Operation::Rename { old, new } = &journal.operation;
    let old_root = instance.mods_dir().join(old);
    let new_root = instance.mods_dir().join(new);
    let private = work_path(instance).join("old");
    let (old_here, new_here) = live_slots(instance, old, new)?;
    let private_here = exists(&private)?;

    match (old_here, private_here, new_here) {
        (true, false, false) => {
            marker_present(&old_root, &journal.transaction)?;
            Ok(())
        }
        (false, true, false) => {
            require_marker(&private, &journal.transaction)?;
            rename_no_replace(&private, &old_root)?;
            after_old_restored()
        }
        (false, false, true) => {
            require_marker(&new_root, &journal.transaction)?;
            rename_no_replace(&new_root, &private)?;
            rename_no_replace(&private, &old_root)?;
            after_old_restored()
        }
        _ => conflict(if old_here {
            old_root
        } else if private_here {
            private
        } else {
            new_root
        }),
    }
}

fn after_old_restored() -> Result<(), LifecycleError> {
    #[cfg(test)]
    super::tests::trip(super::tests::Failpoint::OldRestored)?;
    Ok(())
}

fn require_marker(root: &Utf8Path, transaction: &str) -> Result<(), LifecycleError> {
    if marker_present(root, transaction)? {
        Ok(())
    } else {
        conflict(root.to_owned())
    }
}

/// Restore profiles that still match original or intended snapshots
fn restore_profiles(instance: &Instance, journal: &Journal) -> Result<(), LifecycleError> {
    let mut conflicts = Vec::new();

    for snapshot in &journal.profiles {
        let live = modlist(instance, &snapshot.profile);
        let current = crate::fs::read_to_string_opt(&live)?;

        if current.as_deref() == Some(snapshot.intended.as_str()) {
            match &snapshot.original {
                Some(original) => crate::fs::write_atomic(&live, original.as_bytes())?,
                None => crate::fs::remove_file_opt(&live)?,
            }
        } else if current.as_ref() != snapshot.original.as_ref() {
            conflicts.push(live);
        }
    }

    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(LifecycleError::RecoveryConflict { paths: conflicts })
    }
}

/// Remove terminal transaction markers, workspace, and journal
pub(super) fn cleanup(instance: &Instance, journal: &Journal) -> Result<(), Utf8PathBuf> {
    let Operation::Rename { old, new } = &journal.operation;
    let target = match journal.phase {
        Phase::RolledBack => instance.mods_dir().join(old),
        Phase::Committed => instance.mods_dir().join(new),
        Phase::Active => return Err(journal::journal_path(instance)),
    };
    remove_marker(&target, &journal.transaction)?;

    #[cfg(test)]
    if super::tests::hit(super::tests::Failpoint::Cleanup) {
        return Err(work_path(instance));
    }

    remove_work(instance)?;
    crate::fs::remove_file_opt(&journal::journal_path(instance))
        .map_err(|_| journal::journal_path(instance))
}

fn remove_work(instance: &Instance) -> Result<(), Utf8PathBuf> {
    let work = work_path(instance);
    match std::fs::remove_dir(&work) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(work),
    }
}

/// Classify old and new public slots for ordinary or case-only rename
fn live_slots(instance: &Instance, old: &str, new: &str) -> Result<(bool, bool), LifecycleError> {
    if !cfg!(windows) || !old.eq_ignore_ascii_case(new) {
        return Ok((
            exists(&instance.mods_dir().join(old))?,
            exists(&instance.mods_dir().join(new))?,
        ));
    }

    let matches: Vec<String> = instance
        .installed_mods()?
        .into_iter()
        .filter(|item| item.name.eq_ignore_ascii_case(old))
        .map(|item| item.name)
        .collect();

    match matches.as_slice() {
        [] => Ok((false, false)),
        [name] if name == old => Ok((true, false)),
        [name] if name == new => Ok((false, true)),
        _ => conflict(instance.mods_dir()),
    }
}

/// Create this transaction's zero-byte marker without replacing another entry
pub(super) fn create_marker(root: &Utf8Path, transaction: &str) -> Result<(), LifecycleError> {
    if marker_present(root, transaction)? {
        return conflict(marker_path(root, transaction));
    }

    let path = marker_path(root, transaction);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                LifecycleError::RecoveryConflict {
                    paths: vec![path.clone()],
                }
            } else {
                crate::error::io_err(&path, error).into()
            }
        })?;
    file.sync_all()
        .map_err(|error| crate::error::io_err(&path, error))?;
    Ok(())
}

/// Scan a mod root for this transaction's marker or any stale marker
pub(super) fn marker_present(root: &Utf8Path, transaction: &str) -> Result<bool, LifecycleError> {
    let expected = marker_path(root, transaction);
    let expected_name = expected
        .file_name()
        .expect("transaction validation guarantees a marker filename");
    let Some(entries) = crate::fs::read_dir_opt(root)? else {
        return Ok(false);
    };
    let mut present = false;

    for entry in entries {
        let entry = entry.map_err(|error| crate::error::io_err(root, error))?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(MARKER_PREFIX) {
            continue;
        }
        if name.as_os_str() != OsStr::new(expected_name) {
            return conflict(root.to_owned());
        }
        let file_type = entry
            .file_type()
            .map_err(|error| crate::error::io_err(&expected, error))?;
        let metadata = entry
            .metadata()
            .map_err(|error| crate::error::io_err(&expected, error))?;
        if !file_type.is_file() || metadata.len() != 0 {
            return conflict(expected);
        }
        present = true;
    }
    Ok(present)
}

/// Remove only the expected marker and preserve every other entry
fn remove_marker(root: &Utf8Path, transaction: &str) -> Result<(), Utf8PathBuf> {
    let present = marker_present(root, transaction).map_err(|_| root.to_owned())?;
    if !present {
        return Ok(());
    }
    let marker = marker_path(root, transaction);
    crate::fs::remove_file_opt(&marker).map_err(|_| marker)
}
