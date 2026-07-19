//! Deploy, purge, and status orchestration over an instance's profile

use super::error::{ApplyError, non_utf8};
use super::lock::InstanceLock;
use super::outcome::{CapturedPath, ReversalOutcome};
use super::preparation::PreparedDeployment;
use super::save_paths;
use super::state::{Deployment, SaveRedirect, Status};

use crate::deploy::{
    BACKUP_DIR, DeployEntry, DeployError, DeployRecord, ModSource, NullSink, PreservedConflict,
    ProgressSink, ROOT_DIR, ReversalIssue, TargetOwnership, VerifyReport, deployer_for,
    logical_path_key, strip_data_prefix,
};
use crate::fs;
use crate::instance::{Instance, Profile};
use crate::plugins;
use crate::restore::Restore;
use crate::saves;
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::{BTreeMap, BTreeSet};

/// Deploy a profile's enabled mods into the instance's game directory
pub fn deploy_profile(
    instance: &Instance,
    profile_name: &str,
    progress: &dyn ProgressSink,
) -> Result<Deployment, ApplyError> {
    tracing::info!(profile = profile_name, "deploying profile");
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, progress)?;
    let prepared = PreparedDeployment::build(instance, profile_name)?;
    deploy_profile_locked(instance, &prepared, progress)
}

/// Deploy one prepared profile while the caller owns the instance lock
pub(crate) fn deploy_profile_locked(
    instance: &Instance,
    prepared: &PreparedDeployment,
    progress: &dyn ProgressSink,
) -> Result<Deployment, ApplyError> {
    if Deployment::exists(instance) {
        return Err(ApplyError::AlreadyDeployed {
            path: Deployment::path(instance),
        });
    }

    fs::ensure_dir(&instance.overwrite_dir())?;
    let deployer = deployer_for(instance.config.deployer);
    deployer.check_supported(&prepared.plan)?;

    let backup_root = instance.config.game_dir.join(BACKUP_DIR);
    guard_no_orphaned_backup(&backup_root)?;
    let record = DeployRecord::from_plan(&prepared.plan, backup_root, instance.config.deployer)?;
    let baseline = snapshot_paths(&record.target_root, &record.backup_root)?;

    fs::ensure_dir(&prepared.local_dir)?;
    prepared.plugin_order.save(instance)?;
    let plugins_txt_backup = plugins::read_plugins_txt(&prepared.local_dir)?;

    let prepared_save = if prepared.local_saves {
        let (custom_ini, saves_dir) = prepared
            .save_paths
            .clone()
            .expect("local saves preparation includes paths");
        let original = saves::read_save_redirect(&custom_ini)?;
        Some((custom_ini, saves_dir, original))
    } else {
        None
    };

    let mut deployment = Deployment {
        status: Status::InProgress,
        committed: Some(false),
        profile: prepared.profile.clone(),
        record,
        plugins_txt_backup,
        plugins_txt_intended: None,
        save_redirect: prepared_save.as_ref().map(|(_, _, original)| SaveRedirect {
            original: original.clone(),
        }),
    };

    Deployment::save_baseline(instance, &baseline)?;
    if let Err(error) = deployment.save(instance) {
        let _ = Deployment::remove_baseline(instance);
        return Err(error);
    }

    if let Err(error) = deployer.deploy(&deployment.record, progress) {
        rollback_failed_deploy(instance, deployment, progress);
        return Err(error.into());
    }

    if let Err(error) = plugins::write_active_plugins(
        instance.config.game.load_order_id(),
        &instance.config.game_dir,
        &prepared.local_dir,
        &prepared.plugin_order.plugins,
    ) {
        rollback_failed_deploy(instance, deployment, progress);
        return Err(error.into());
    }

    deployment.plugins_txt_intended = match plugins::read_plugins_txt(&prepared.local_dir) {
        Ok(intended) => intended,
        Err(error) => {
            rollback_failed_deploy(instance, deployment, progress);
            return Err(error.into());
        }
    };
    if let Err(error) = deployment.save(instance) {
        rollback_failed_deploy(instance, deployment, progress);
        return Err(error);
    }

    if let Some((custom_ini, saves_dir, _)) = &prepared_save
        && let Err(error) = saves::write_save_redirect(custom_ini, saves_dir, &deployment.profile)
    {
        rollback_failed_deploy(instance, deployment, progress);
        return Err(error.into());
    }

    let mut committed = deployment.clone();
    committed.status = Status::Committed;
    committed.committed = Some(true);
    if let Err(error) = committed.save(instance) {
        rollback_failed_deploy(instance, deployment, progress);
        return Err(error);
    }

    tracing::info!(
        profile = %committed.profile,
        files = committed.record.entries.len(),
        "deployment committed"
    );
    Ok(committed)
}

/// Reverse the instance's recorded deployment
pub fn purge(
    instance: &Instance,
    progress: &dyn ProgressSink,
) -> Result<ReversalOutcome, ApplyError> {
    purge_with_force(instance, progress, false)
}

/// Reverse a deployment despite a stale launch marker
pub fn purge_forced(
    instance: &Instance,
    progress: &dyn ProgressSink,
) -> Result<ReversalOutcome, ApplyError> {
    purge_with_force(instance, progress, true)
}

fn purge_with_force(
    instance: &Instance,
    progress: &dyn ProgressSink,
    force: bool,
) -> Result<ReversalOutcome, ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    let deployment = Deployment::load(instance)?;
    purge_locked(instance, deployment, progress, force)
}

/// A snapshot of an instance's recorded deployment
#[derive(Debug)]
pub struct DeploymentStatus {
    pub deployment: Deployment,
    pub verified: VerifyReport,
}

/// Report the recorded deployment without changing interrupted state
pub fn status(instance: &Instance) -> Result<Option<DeploymentStatus>, ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    if !Deployment::exists(instance) {
        return Ok(None);
    }
    let deployment = Deployment::load(instance)?;
    let verified = deployer_for(deployment.record.deployer).verify(&deployment.record);
    Ok(Some(DeploymentStatus {
        deployment,
        verified,
    }))
}

/// The ordered deploy sources for a profile: managed mods (low->high) plus the overwrite if present
pub fn deploy_sources(instance: &Instance, profile: &Profile) -> Vec<ModSource> {
    let mut sources = profile.deploy_sources(instance);
    let overwrite = instance.overwrite_dir();
    if overwrite.symlink_metadata().is_ok_and(|m| m.is_dir()) {
        sources.push(ModSource::overwrite(overwrite));
    }
    sources
}

/// Rename an installed mod, refusing while a committed deployment is live
pub fn rename_mod(instance: &Instance, old: &str, new: &str) -> Result<(), ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, &NullSink)?;

    if Deployment::exists(instance) {
        return Err(ApplyError::DeployedCannotRename {
            path: Deployment::path(instance),
        });
    }

    instance.rename_mod(old, new).map_err(Into::into)
}

/// Rename a profile, refusing only while that profile's own deployment is live
pub fn rename_profile(instance: &mut Instance, old: &str, new: &str) -> Result<(), ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, &NullSink)?;

    if Deployment::exists(instance)
        && Deployment::load(instance)?
            .profile
            .eq_ignore_ascii_case(old)
    {
        return Err(ApplyError::DeployedCannotRename {
            path: Deployment::path(instance),
        });
    }
    instance.rename_profile(old, new)?;

    if instance.config.default_profile.eq_ignore_ascii_case(old) {
        let previous = std::mem::replace(&mut instance.config.default_profile, new.to_owned());
        if let Err(error) = instance.save() {
            instance.config.default_profile = previous;
            return Err(ApplyError::DefaultProfileNotUpdated(error));
        }
    }
    Ok(())
}

/// Purge any non-committed journal before a mutating operation continues
pub(crate) fn recover_if_needed(
    instance: &Instance,
    progress: &dyn ProgressSink,
) -> Result<Option<ReversalOutcome>, ApplyError> {
    guard_launch_inactive(instance)?;
    if !Deployment::exists(instance) {
        return Ok(None);
    }
    let deployment = Deployment::load(instance)?;
    if deployment.status == Status::Committed {
        return Ok(None);
    }

    tracing::warn!(
        status = ?deployment.status,
        profile = %deployment.profile,
        "interrupted deployment found; purging before restart"
    );
    purge_locked(instance, deployment, progress, false).map(Some)
}

/// Run ordinary purge while the caller holds the instance lock
pub(crate) fn purge_locked(
    instance: &Instance,
    deployment: Deployment,
    progress: &dyn ProgressSink,
    force: bool,
) -> Result<ReversalOutcome, ApplyError> {
    if !force {
        guard_launch_inactive(instance)?;
    }
    tracing::info!(profile = %deployment.profile, "purging recorded deployment");
    let mut outcome = ReversalOutcome::default();
    let baseline = match Deployment::load_baseline(instance) {
        Ok(baseline) => baseline,
        Err(error) => {
            outcome.unresolved.push(ReversalIssue::new(
                Deployment::baseline_path(instance),
                error.to_string(),
            ));
            None
        }
    };
    let baseline_keys = baseline.as_ref().map(|paths| {
        paths
            .iter()
            .map(|path| logical_path_key(path))
            .collect::<BTreeSet<_>>()
    });
    let deployer = deployer_for(deployment.record.deployer);

    let cleanup_dirs = capture_overwrite(
        instance,
        &deployment.record,
        deployer.as_ref(),
        baseline_keys.as_ref(),
        &mut outcome,
    );

    let report = deployer.undeploy(&deployment.record, progress);
    outcome.removed.extend(report.removed);
    outcome.restored.extend(report.restored);
    for conflict in report.preserved_conflicts {
        push_conflict(&mut outcome, conflict);
    }
    outcome.unresolved.extend(report.unresolved);

    cleanup_new_dirs(&deployment.record.target_root, cleanup_dirs, &mut outcome);
    restore_side_effects(instance, &deployment, &mut outcome);

    if outcome.is_complete() {
        tracing::info!("reversal complete; clearing journal");
        Deployment::remove(instance)?;
        Ok(outcome)
    } else {
        tracing::warn!("reversal incomplete; keeping RecoveryFailed journal");
        let path = Deployment::path(instance);
        let failed = Deployment {
            status: Status::RecoveryFailed,
            ..deployment
        };
        failed.save(instance)?;
        Err(ApplyError::RecoveryFailed {
            path,
            outcome: Box::new(outcome),
        })
    }
}

/// Restore Plugins.txt and save redirection according to the deployment phase
fn restore_side_effects(
    instance: &Instance,
    deployment: &Deployment,
    outcome: &mut ReversalOutcome,
) {
    let interrupted = !deployment.was_committed();
    match instance.local_dir() {
        Ok(local_dir) => {
            let plugins_result = if interrupted {
                plugins::restore_plugins_txt(&local_dir, deployment.plugins_txt_backup.as_deref())
                    .map(|()| Restore::Restored)
            } else {
                plugins::restore_plugins_txt_if_ours(
                    &local_dir,
                    deployment.plugins_txt_backup.as_deref(),
                    deployment.plugins_txt_intended.as_deref(),
                )
            };
            match plugins_result {
                Ok(result) => outcome.plugins_txt = result,
                Err(error) => outcome.unresolved.push(ReversalIssue::new(
                    local_dir.join("Plugins.txt"),
                    error.to_string(),
                )),
            }
        }
        Err(error) => outcome.unresolved.push(ReversalIssue::new(
            Deployment::path(instance),
            error.to_string(),
        )),
    }

    let Some(redirect) = &deployment.save_redirect else {
        return;
    };
    let (custom_ini, _) = match save_paths(instance, &deployment.profile) {
        Ok(paths) => paths,
        Err(error) => {
            outcome.unresolved.push(ReversalIssue::new(
                Deployment::path(instance),
                error.to_string(),
            ));
            return;
        }
    };
    let save_result = if interrupted {
        saves::restore_save_redirect_unconditionally(&custom_ini, redirect.original.as_deref())
            .map(|()| Restore::Restored)
    } else {
        saves::restore_save_redirect(
            &custom_ini,
            &deployment.profile,
            redirect.original.as_deref(),
        )
    };
    match save_result {
        Ok(result) => outcome.save_redirect = result,
        Err(error) => outcome
            .unresolved
            .push(ReversalIssue::new(custom_ini, error.to_string())),
    }
}

/// Roll back a failed deploy through the same ordinary purge path
fn rollback_failed_deploy(
    instance: &Instance,
    deployment: Deployment,
    progress: &dyn ProgressSink,
) {
    tracing::warn!("deploy failed; purging interrupted deployment");
    if let Err(error) = purge_locked(instance, deployment, progress, false) {
        tracing::warn!(%error, "rollback after deploy failure was incomplete");
    }
}

fn guard_launch_inactive(instance: &Instance) -> Result<(), ApplyError> {
    if crate::launch::has_launch_marker(instance)? {
        return Err(ApplyError::LaunchActive {
            path: crate::launch::launch_marker_path(instance),
        });
    }
    Ok(())
}

/// Snapshot every path below the target without following reparse points
fn snapshot_paths(
    target_root: &Utf8Path,
    backup_root: &Utf8Path,
) -> Result<Vec<Utf8PathBuf>, ApplyError> {
    let mut paths = scan_tree(target_root, Some(backup_root))?
        .into_iter()
        .map(|entry| entry.relative)
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreeKind {
    File,
    Directory,
    Other,
}

#[derive(Debug)]
struct TreeEntry {
    absolute: Utf8PathBuf,
    relative: Utf8PathBuf,
    kind: TreeKind,
}

/// Walk a tree without entering symlinks, junctions, or other reparse points
fn scan_tree(
    root: &Utf8Path,
    excluded_root: Option<&Utf8Path>,
) -> Result<Vec<TreeEntry>, ApplyError> {
    match root.symlink_metadata() {
        Ok(metadata) if fs::is_directory(&metadata) => {}
        Ok(_) => {
            return Err(DeployError::UnsafeFileType {
                path: root.to_owned(),
            }
            .into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(crate::error::io_err(root, error).into()),
    }

    let excluded_key =
        excluded_root.and_then(|excluded| excluded.strip_prefix(root).ok().map(logical_path_key));
    let mut entries = Vec::new();
    scan_dir(root, root, excluded_key.as_deref(), &mut entries)?;
    entries.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(entries)
}

/// Scan one normal directory and recurse only into normal child directories
fn scan_dir(
    root: &Utf8Path,
    dir: &Utf8Path,
    excluded_key: Option<&str>,
    output: &mut Vec<TreeEntry>,
) -> Result<(), ApplyError> {
    let metadata = dir
        .symlink_metadata()
        .map_err(|error| crate::error::io_err(dir, error))?;
    if !fs::is_directory(&metadata) {
        return Err(DeployError::UnsafeFileType {
            path: dir.to_owned(),
        }
        .into());
    }
    let children = std::fs::read_dir(dir).map_err(|error| crate::error::io_err(dir, error))?;
    for child in children {
        let child = child.map_err(|error| crate::error::io_err(dir, error))?;
        let path = Utf8PathBuf::from_path_buf(child.path())
            .map_err(|path| DeployError::NonUtf8Path(non_utf8(&path)))?;
        let relative = path
            .strip_prefix(root)
            .expect("scanned path remains below its root")
            .to_owned();
        if excluded_key.is_some_and(|key| logical_path_key(&relative) == key) {
            continue;
        }

        let metadata = path
            .symlink_metadata()
            .map_err(|error| crate::error::io_err(&path, error))?;
        let kind = if fs::is_regular_file(&metadata) {
            TreeKind::File
        } else if fs::is_directory(&metadata) {
            TreeKind::Directory
        } else {
            TreeKind::Other
        };
        output.push(TreeEntry {
            absolute: path.clone(),
            relative,
            kind,
        });
        if kind == TreeKind::Directory {
            scan_dir(root, &path, excluded_key, output)?;
        }
    }
    Ok(())
}

/// Capture foreign replacements and newly-created residue into overwrite
fn capture_overwrite(
    instance: &Instance,
    record: &DeployRecord,
    deployer: &dyn crate::deploy::Deployer,
    baseline: Option<&BTreeSet<String>>,
    outcome: &mut ReversalOutcome,
) -> Vec<Utf8PathBuf> {
    let overwrite = instance.overwrite_dir();
    resume_pending_captures(record, &overwrite, outcome);

    let scanned = match scan_tree(&record.target_root, Some(&record.backup_root)) {
        Ok(scanned) => scanned,
        Err(error) => {
            outcome
                .unresolved
                .push(ReversalIssue::new(&record.target_root, error.to_string()));
            return Vec::new();
        }
    };
    let recorded = record
        .entries
        .iter()
        .map(|entry| (logical_path_key(&entry.relative), entry))
        .collect::<BTreeMap<_, _>>();
    let mut cleanup_dirs = Vec::new();

    for scanned in scanned {
        let key = logical_path_key(&scanned.relative);
        if scanned.kind == TreeKind::Directory {
            if should_capture_unrecorded(&scanned.relative, &key, baseline, record) {
                cleanup_dirs.push(scanned.relative);
            }
            continue;
        }

        if let Some(entry) = recorded.get(&key) {
            capture_recorded_path(
                record, entry, &scanned, deployer, baseline, &overwrite, outcome,
            );
        } else {
            capture_unrecorded_path(record, &scanned, &key, baseline, &overwrite, outcome);
        }
    }
    cleanup_dirs
}

/// Resume deterministic pending captures before inspecting the live game tree
fn resume_pending_captures(
    record: &DeployRecord,
    overwrite: &Utf8Path,
    outcome: &mut ReversalOutcome,
) {
    let capture_root = record.backup_root.join(".capture");
    let pending = match scan_tree(&capture_root, None) {
        Ok(pending) => pending,
        Err(error) => {
            outcome
                .unresolved
                .push(ReversalIssue::new(&capture_root, error.to_string()));
            return;
        }
    };

    for pending in pending {
        if pending.kind == TreeKind::Directory {
            continue;
        }
        let game_relative = game_relative_from_overwrite(&pending.relative);
        if pending.kind == TreeKind::Other {
            push_conflict(
                outcome,
                PreservedConflict {
                    path: game_relative,
                    preserved_at: pending.absolute,
                    reason: "pending capture is non-regular and was preserved".to_owned(),
                    blocking: true,
                },
            );
            continue;
        }
        deliver_pending(
            &pending.absolute,
            &game_relative,
            &pending.relative,
            overwrite,
            outcome,
        );
    }
}

/// Capture a recorded destination according to backend ownership
fn capture_recorded_path(
    record: &DeployRecord,
    entry: &DeployEntry,
    scanned: &TreeEntry,
    deployer: &dyn crate::deploy::Deployer,
    baseline: Option<&BTreeSet<String>>,
    overwrite: &Utf8Path,
    outcome: &mut ReversalOutcome,
) {
    match deployer.classify(record, entry) {
        TargetOwnership::OwnedLink | TargetOwnership::Absent => {}
        TargetOwnership::Unknown(error) => outcome
            .unresolved
            .push(ReversalIssue::new(&scanned.absolute, error.to_string())),
        TargetOwnership::Foreign => {
            let backup = record.backup_root.join(&entry.relative);
            let backup_present = match regular_file_present(&backup) {
                Ok(present) => present,
                Err(issue) => {
                    outcome.unresolved.push(issue);
                    return;
                }
            };
            let preexisting =
                baseline.is_some_and(|paths| paths.contains(&logical_path_key(&entry.relative)));
            let capture = backup_present || baseline.is_some_and(|_| !preexisting);

            if !capture {
                if baseline.is_none() {
                    push_conflict(
                        outcome,
                        PreservedConflict {
                            path: scanned.relative.clone(),
                            preserved_at: scanned.absolute.clone(),
                            reason: "legacy journal cannot prove this foreign path is new"
                                .to_owned(),
                            blocking: false,
                        },
                    );
                }
                return;
            }

            if scanned.kind == TreeKind::File {
                stage_capture(
                    record,
                    &scanned.absolute,
                    &scanned.relative,
                    overwrite,
                    outcome,
                );
            } else {
                push_conflict(
                    outcome,
                    PreservedConflict {
                        path: scanned.relative.clone(),
                        preserved_at: scanned.absolute.clone(),
                        reason: "foreign reparse or non-regular path was preserved".to_owned(),
                        blocking: true,
                    },
                );
            }
        }
    }
}

/// Capture one unrecorded path only when the baseline proves it is new
fn capture_unrecorded_path(
    record: &DeployRecord,
    scanned: &TreeEntry,
    key: &str,
    baseline: Option<&BTreeSet<String>>,
    overwrite: &Utf8Path,
    outcome: &mut ReversalOutcome,
) {
    if !should_capture_unrecorded(&scanned.relative, key, baseline, record) {
        return;
    }
    if scanned.kind == TreeKind::File {
        stage_capture(
            record,
            &scanned.absolute,
            &scanned.relative,
            overwrite,
            outcome,
        );
    } else {
        push_conflict(
            outcome,
            PreservedConflict {
                path: scanned.relative.clone(),
                preserved_at: scanned.absolute.clone(),
                reason: "new reparse or non-regular path was preserved".to_owned(),
                blocking: true,
            },
        );
    }
}

/// Decide whether an unrecorded path is new under current or legacy state
fn should_capture_unrecorded(
    relative: &Utf8Path,
    key: &str,
    baseline: Option<&BTreeSet<String>>,
    record: &DeployRecord,
) -> bool {
    match baseline {
        Some(paths) => !paths.contains(key),
        None => record
            .created_dirs
            .iter()
            .any(|created| path_is_within(relative, created)),
    }
}

/// Compare path components case-insensitively for legacy created-directory coverage
fn path_is_within(path: &Utf8Path, parent: &Utf8Path) -> bool {
    let mut path = path.components();
    parent.components().all(|component| {
        path.next()
            .is_some_and(|candidate| candidate.as_str().eq_ignore_ascii_case(component.as_str()))
    })
}

/// Move a live regular file to deterministic pending storage before delivery
fn stage_capture(
    record: &DeployRecord,
    source: &Utf8Path,
    game_relative: &Utf8Path,
    overwrite: &Utf8Path,
    outcome: &mut ReversalOutcome,
) {
    let overwrite_relative = overwrite_staging_path(game_relative);
    let pending = record
        .backup_root
        .join(".capture")
        .join(&overwrite_relative);
    match pending.symlink_metadata() {
        Ok(_) => {
            push_conflict(
                outcome,
                PreservedConflict {
                    path: game_relative.to_owned(),
                    preserved_at: source.to_owned(),
                    reason: format!("pending capture already exists at `{pending}`"),
                    blocking: true,
                },
            );
            return;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            outcome
                .unresolved
                .push(ReversalIssue::new(&pending, error.to_string()));
            return;
        }
    }

    if let Some(parent) = pending.parent()
        && let Err(error) = fs::ensure_dir(parent)
    {
        outcome
            .unresolved
            .push(ReversalIssue::new(parent, error.to_string()));
        return;
    }
    if let Err(error) = std::fs::rename(source, &pending) {
        outcome
            .unresolved
            .push(ReversalIssue::new(source, error.to_string()));
        return;
    }
    deliver_pending(
        &pending,
        game_relative,
        &overwrite_relative,
        overwrite,
        outcome,
    );
}

/// Deliver a pending capture without replacing any prior overwrite content
fn deliver_pending(
    pending: &Utf8Path,
    game_relative: &Utf8Path,
    overwrite_relative: &Utf8Path,
    overwrite: &Utf8Path,
    outcome: &mut ReversalOutcome,
) {
    let destination = overwrite.join(overwrite_relative);
    match destination.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::move_file(pending, &destination) {
                Ok(()) => outcome.captured.push(CapturedPath {
                    game_relative: game_relative.to_owned(),
                    overwrite_relative: overwrite_relative.to_owned(),
                }),
                Err(error) => outcome
                    .unresolved
                    .push(ReversalIssue::new(pending, error.to_string())),
            }
        }
        Ok(metadata) if fs::is_regular_file(&metadata) => {
            match files_equal(pending, &destination) {
                Ok(true) => match fs::remove_file_opt(pending) {
                    Ok(()) => outcome.captured.push(CapturedPath {
                        game_relative: game_relative.to_owned(),
                        overwrite_relative: overwrite_relative.to_owned(),
                    }),
                    Err(error) => outcome
                        .unresolved
                        .push(ReversalIssue::new(pending, error.to_string())),
                },
                Ok(false) => push_capture_collision(pending, game_relative, &destination, outcome),
                Err(issue) => outcome.unresolved.push(issue),
            }
        }
        Ok(_) => push_capture_collision(pending, game_relative, &destination, outcome),
        Err(error) => outcome
            .unresolved
            .push(ReversalIssue::new(&destination, error.to_string())),
    }
}

/// Record an overwrite collision while preserving both copies
fn push_capture_collision(
    pending: &Utf8Path,
    game_relative: &Utf8Path,
    destination: &Utf8Path,
    outcome: &mut ReversalOutcome,
) {
    push_conflict(
        outcome,
        PreservedConflict {
            path: game_relative.to_owned(),
            preserved_at: pending.to_owned(),
            reason: format!("overwrite destination `{destination}` already has different content"),
            blocking: true,
        },
    );
}

/// Compare two regular files byte-for-byte
fn files_equal(left: &Utf8Path, right: &Utf8Path) -> Result<bool, ReversalIssue> {
    let left_bytes =
        std::fs::read(left).map_err(|error| ReversalIssue::new(left, error.to_string()))?;
    let right_bytes =
        std::fs::read(right).map_err(|error| ReversalIssue::new(right, error.to_string()))?;
    Ok(left_bytes == right_bytes)
}

/// Probe a path without following it and require regular content when present
fn regular_file_present(path: &Utf8Path) -> Result<bool, ReversalIssue> {
    match path.symlink_metadata() {
        Ok(metadata) if fs::is_regular_file(&metadata) => Ok(true),
        Ok(_) => Err(ReversalIssue::new(
            path,
            "backup path is non-regular and was preserved",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ReversalIssue::new(path, error.to_string())),
    }
}

/// Avoid duplicate backend and capture reports for the same preserved path
fn push_conflict(outcome: &mut ReversalOutcome, conflict: PreservedConflict) {
    if outcome.preserved_conflicts.iter().any(|existing| {
        existing.path == conflict.path && existing.preserved_at == conflict.preserved_at
    }) {
        return;
    }
    outcome.preserved_conflicts.push(conflict);
}

/// Remove baseline-new normal directories after their contents are resolved
fn cleanup_new_dirs(
    target_root: &Utf8Path,
    mut directories: Vec<Utf8PathBuf>,
    outcome: &mut ReversalOutcome,
) {
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    directories.dedup();
    for relative in directories {
        let path = target_root.join(&relative);
        match path.symlink_metadata() {
            Ok(metadata) if fs::is_directory(&metadata) => {
                if let Err(error) = std::fs::remove_dir(&path)
                    && error.kind() != std::io::ErrorKind::DirectoryNotEmpty
                {
                    outcome
                        .unresolved
                        .push(ReversalIssue::new(&path, error.to_string()));
                }
            }
            Ok(_) => push_conflict(
                outcome,
                PreservedConflict {
                    path: relative,
                    preserved_at: path,
                    reason: "new directory path became non-regular and was preserved".to_owned(),
                    blocking: true,
                },
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => outcome
                .unresolved
                .push(ReversalIssue::new(&path, error.to_string())),
        }
    }
}

/// Invert overwrite staging layout back to a game-relative path
fn game_relative_from_overwrite(overwrite_relative: &Utf8Path) -> Utf8PathBuf {
    let mut components = overwrite_relative.components();
    match components.next() {
        Some(first) if first.as_str().eq_ignore_ascii_case(ROOT_DIR) => {
            components.as_path().to_owned()
        }
        _ => Utf8Path::new("Data").join(overwrite_relative),
    }
}

/// Invert rooted deployment layout into the global overwrite staging layout
fn overwrite_staging_path(game_relative: &Utf8Path) -> Utf8PathBuf {
    match strip_data_prefix(game_relative) {
        Some(under_data) if !under_data.as_str().is_empty() => under_data,
        _ => Utf8Path::new(ROOT_DIR).join(game_relative),
    }
}

/// Refuse deploy when the fixed backup root survives without a journal
fn guard_no_orphaned_backup(backup_root: &Utf8Path) -> Result<(), ApplyError> {
    if backup_root.exists() {
        return Err(ApplyError::OrphanedBackup {
            path: backup_root.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/ops.rs"]
mod tests;
