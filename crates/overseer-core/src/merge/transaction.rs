//! The list-driven merge transaction: resolve a plugin list, merge archives into mod, back up source

use super::{MergeConflict, MergeCounts, MergeOptions, MergeSource, merge};
use crate::apply::{ApplyError, Deployment, InstanceLock};
use crate::error::IoError;
use crate::fs;
use crate::instance::{Instance, InstanceError, Profile};
use crate::plugins::{PluginError, PluginLoadOrder};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// A request to merge a named set of plugins' archives
#[derive(Debug, Clone)]
pub struct MergeRequest {
    /// Output basename shared by the mod, its archives, and their carriers
    pub name: String,
    /// The plugin filenames to merge
    pub plugins: Vec<String>,
    /// Uncompressed byte cap per texture group before its split
    pub texture_group_bytes: u64,
}

/// One requested plugin resolved to its source archives in `Data/`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedItem {
    /// The plugin filename
    pub plugin: String,
    /// Its `<stem> - Main.ba2` in `Data/`, if present
    pub main: Option<Utf8PathBuf>,
    /// Its `<stem> - Textures.ba2` in `Data/`, if present
    pub textures: Option<Utf8PathBuf>,
    /// Its index among active plugins
    pub rank: usize,
}

/// How a request plugin list partitions against a profile and its `Data/`
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedPlan {
    /// Active plugins with at least one BA2
    pub items: Vec<ResolvedItem>,
    /// Requested plugins that are inactive in the profile
    pub inactive: Vec<String>,
    /// Active plugins with no BA2s and not owned by a prior merge
    pub orphaned: Vec<String>,
    /// Active plugins whose BA2s a prior merge already consumed, with that merge's name
    pub already_merged: Vec<(String, String)>,
    /// Requested plugins absent from the load order
    pub missing: Vec<String>,
}

/// A source archive moved aside so a merge can be reversed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBackup {
    /// Where the archive lived in `Data/`
    pub original: Utf8PathBuf,
    /// Where it was moved for safekeeping
    pub backup: Utf8PathBuf,
}

/// The on-disk record of a committed merge; doubles as the registry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The merge name = mod name = output basename
    pub name: String,
    /// Carrier plugin basenames the merge emitted
    pub carriers: Vec<String>,
    /// The plugins whose archives were merged
    pub plugins: Vec<String>,
    /// The moved source archives
    pub sources: Vec<SourceBackup>,
    /// Creation time in seconds since the Unix epoch
    pub created_at: u64,
}

/// What a completed merge produced, for the caller to report
#[derive(Debug)]
pub struct MergeReport {
    /// The merge name
    pub name: String,
    /// The managed mod directory the merge was materialized into
    pub mod_dir: Utf8PathBuf,
    /// Counts of the archives produced
    pub archives: MergeCounts,
    /// Carrier plugin basenames emitted
    pub carriers: Vec<String>,
    /// Path clashes resolved by rank
    pub conflicts: Vec<MergeConflict>,
    /// How many source archives were moved to backup
    pub sources_removed: usize,
    /// The resolution the merge ran on
    pub plan: ResolvedPlan,
}

/// Why a merge transaction could not complete
#[derive(Debug, Error)]
pub enum MergeTxnError {
    /// The pure merge engine failed
    #[error(transparent)]
    Merge(#[from] super::MergeError),

    /// An instance or profile operation failed
    #[error(transparent)]
    Instance(#[from] InstanceError),

    /// Reading the load order failed
    #[error(transparent)]
    Plugins(#[from] PluginError),

    /// A filesystem operation failed
    #[error(transparent)]
    Io(#[from] IoError),

    /// Locking the instance or probing deployment failed
    #[error(transparent)]
    Apply(#[from] ApplyError),

    /// The instance is deployed; merging needs an undeployed instance
    #[error("instance is deployed; purge before merging")]
    Deployed,

    /// A merge with this name already exists
    #[error("a merge named `{0}` already exists")]
    NameExists(String),

    /// No merge with this name exists to restore
    #[error("no merge named `{0}`")]
    NoSuchMerge(String),

    /// The plugin list resolved to nothing mergeable
    #[error("no mergeable archives in the plugin list")]
    NothingToMerge,

    /// The merge name is not a usable plugin/mod base name
    #[error("invalid merge name `{0}`")]
    InvalidName(String),

    /// A carrier plugin name would clash with an existing plugin or merge
    #[error("carrier plugin `{0}` clashes with an existing plugin or merge")]
    CarrierCollision(String),

    /// Restore found originals occupying a source slot with different content
    #[error("restore conflict {0:?} differ from their backups")]
    RestoreConflict(Vec<Utf8PathBuf>),
}

/// Resolve `requested` against a profile's load order and `Data/`, without mutating anything
pub fn resolve(
    instance: &Instance,
    profile: &Profile,
    requested: &[String],
) -> Result<ResolvedPlan, MergeTxnError> {
    let data_dir = instance.config.game_dir.join(crate::deploy::DATA_DIR);
    let present = data_filenames(&data_dir)?;
    let order = PluginLoadOrder::load(instance, &profile.name)?;
    let owned = owned_plugins(instance)?;

    let active_rank: HashMap<String, usize> = order
        .plugins
        .iter()
        .filter(|entry| entry.active)
        .enumerate()
        .map(|(rank, entry)| (entry.name.to_ascii_lowercase(), rank))
        .collect();

    let mut seen = HashSet::new();
    let mut plan = ResolvedPlan::default();
    for req in requested {
        let key = req.to_ascii_lowercase();
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some(entry) = order
            .plugins
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(req))
        else {
            plan.missing.push(req.clone());
            continue;
        };
        if !entry.active {
            plan.inactive.push(entry.name.clone());
            continue;
        }
        let stem = plugin_stem(&entry.name);
        let main = ba2_in_data(&data_dir, &present, stem, "Main");
        let textures = ba2_in_data(&data_dir, &present, stem, "Textures");
        if main.is_none() && textures.is_none() {
            match owned.get(&key) {
                Some(owner) => plan
                    .already_merged
                    .push((entry.name.clone(), owner.clone())),
                None => plan.orphaned.push(entry.name.clone()),
            }
        } else {
            let rank = active_rank.get(&key).copied().unwrap_or(0);
            plan.items.push(ResolvedItem {
                plugin: entry.name.clone(),
                main,
                textures,
                rank,
            });
        }
    }
    Ok(plan)
}

/// Merge `req`'s plugins into a managed mod, moving the sources to a reversible backup
pub fn run(
    instance: &Instance,
    profile: &Profile,
    req: &MergeRequest,
) -> Result<MergeReport, MergeTxnError> {
    let _lock = InstanceLock::acquire(instance)?;
    if Deployment::exists(instance) {
        return Err(MergeTxnError::Deployed);
    }
    let manifest_path = manifest_path(instance, &req.name);
    if manifest_path.exists() {
        return Err(MergeTxnError::NameExists(req.name.clone()));
    }
    validate_name(instance, &req.name)?;

    let plan = resolve(instance, profile, &req.plugins)?;
    if plan.items.is_empty() {
        return Err(MergeTxnError::NothingToMerge);
    }
    let mut sources = Vec::new();
    for item in &plan.items {
        if let Some(main) = &item.main {
            sources.push(MergeSource {
                archive: main.clone(),
                override_rank: item.rank,
            });
        }
        if let Some(textures) = &item.textures {
            sources.push(MergeSource {
                archive: textures.clone(),
                override_rank: item.rank,
            });
        }
    }

    let staging = staging_dir(instance, &req.name);
    fs::remove_dir_all_opt(&staging)?;
    fs::remove_dir_all_opt(&backup_dir(instance, &req.name))?;
    fs::ensure_dir(&staging)?;
    let opts = MergeOptions {
        basename: req.name.clone(),
        texture_group_bytes: req.texture_group_bytes,
    };
    let out = merge(&sources, &staging, &opts, instance.config.game)?;

    let carriers: Vec<String> = out
        .carriers
        .iter()
        .filter_map(|p| p.file_stem().map(str::to_owned))
        .collect();
    check_carrier_collisions(instance, &carriers)?;

    let backup_dir = backup_dir(instance, &req.name);
    let source_backups: Vec<SourceBackup> = sources
        .iter()
        .map(|s| SourceBackup {
            original: s.archive.clone(),
            backup: backup_dir.join(s.archive.file_name().unwrap_or_default()),
        })
        .collect();
    let manifest = Manifest {
        name: req.name.clone(),
        carriers: carriers.clone(),
        plugins: plan.items.iter().map(|i| i.plugin.clone()).collect(),
        sources: source_backups.clone(),
        created_at: now_epoch(),
    };

    // the manifest write is the restore-commit point; any later error rolls back
    write_manifest(&manifest_path, &manifest)?;
    let mod_dir = match commit(instance, req, &staging, &source_backups) {
        Ok(dir) => dir,
        Err(e) => {
            let _ = restore_locked(instance, &req.name);
            return Err(e);
        }
    };

    Ok(MergeReport {
        name: req.name.clone(),
        mod_dir,
        archives: out.counts(),
        carriers,
        conflicts: out.conflicts,
        sources_removed: source_backups.len(),
        plan,
    })
}

/// Materialize the staged mod, register it in every profile, and move the sources to backup
fn commit(
    instance: &Instance,
    req: &MergeRequest,
    staging: &Utf8Path,
    source_backups: &[SourceBackup],
) -> Result<Utf8PathBuf, MergeTxnError> {
    let mod_dir = instance.mods_dir().join(&req.name);
    if mod_dir.exists() {
        return Err(MergeTxnError::NameExists(req.name.clone()));
    }
    fs::ensure_dir(&instance.mods_dir())?;
    fs::rename(staging, &mod_dir)?;
    for profile_name in instance.profiles()? {
        let mut profile = Profile::load(instance, &profile_name)?;
        if !profile.contains(&req.name) {
            profile.add(&req.name, true)?;
            profile.save(instance)?;
        }
    }
    for source in source_backups {
        fs::move_file(&source.original, &source.backup)?;
    }
    Ok(mod_dir)
}

/// Undo a committed merge: restore the sources, drop the mod, and delete the manifest
pub fn restore(instance: &Instance, name: &str) -> Result<(), MergeTxnError> {
    validate_name_syntax(name)?;
    let _lock = InstanceLock::acquire(instance)?;
    if Deployment::exists(instance) {
        return Err(MergeTxnError::Deployed);
    }
    restore_locked(instance, name)
}

/// The reversal steps, assuming the caller already holds the instance lock and checked deployment
fn restore_locked(instance: &Instance, name: &str) -> Result<(), MergeTxnError> {
    let manifest_path = manifest_path(instance, name);
    let Some(manifest) = read_manifest_opt(&manifest_path)? else {
        return Err(MergeTxnError::NoSuchMerge(name.to_owned()));
    };

    // an original slot refilled while its backup still exists is a conflict we never overwrite
    let conflicts: Vec<Utf8PathBuf> = manifest
        .sources
        .iter()
        .filter(|s| s.original.exists() && s.backup.exists())
        .map(|s| s.original.clone())
        .collect();
    if !conflicts.is_empty() {
        return Err(MergeTxnError::RestoreConflict(conflicts));
    }

    for source in &manifest.sources {
        if !source.original.exists() && source.backup.exists() {
            fs::move_file(&source.backup, &source.original)?;
        }
    }
    for profile_name in instance.profiles()? {
        let mut profile = Profile::load(instance, &profile_name)?;
        if profile.contains(name) {
            profile.remove(name)?;
            profile.save(instance)?;
        }
    }
    fs::remove_dir_all_opt(&instance.mods_dir().join(name))?;
    fs::remove_dir_all_opt(&staging_dir(instance, name))?;
    fs::remove_file_opt(&manifest_path)?;
    fs::remove_dir_all_opt(&backup_dir(instance, name))?;
    Ok(())
}

/// Every committed merge's manifest, in filename order
pub fn manifests(instance: &Instance) -> Result<Vec<Manifest>, MergeTxnError> {
    let dir = merges_dir(instance);
    let mut out = Vec::new();
    let Some(entries) = fs::read_dir_opt(&dir)? else {
        return Ok(out);
    };
    let mut paths: Vec<Utf8PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| crate::error::io_err(&dir, e))?;
        if let Ok(path) = Utf8PathBuf::from_path_buf(entry.path())
            && path.extension() == Some("json")
        {
            paths.push(path);
        }
    }
    paths.sort();
    for path in paths {
        if let Some(manifest) = read_manifest_opt(&path)? {
            out.push(manifest);
        }
    }
    Ok(out)
}

// ────────────────────────────────────────────────────────────────────────
// Paths, manifests, validation
// ────────────────────────────────────────────────────────────────────────

/// The instance's merge store: manifests and source backups
fn merges_dir(instance: &Instance) -> Utf8PathBuf {
    instance.root.join("merges")
}
fn manifest_path(instance: &Instance, name: &str) -> Utf8PathBuf {
    merges_dir(instance).join(format!("{name}.json"))
}
fn backup_dir(instance: &Instance, name: &str) -> Utf8PathBuf {
    merges_dir(instance).join(name)
}
fn staging_dir(instance: &Instance, name: &str) -> Utf8PathBuf {
    merges_dir(instance).join(format!("{name}.staging"))
}

/// Write a manifest atomically as pretty JSON
fn write_manifest(path: &Utf8Path, manifest: &Manifest) -> Result<(), MergeTxnError> {
    let json = serde_json::to_vec_pretty(manifest).map_err(|e| {
        crate::error::io_err(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        )
    })?;
    fs::write_atomic(path, &json)?;
    Ok(())
}

/// Read a manifest, returning `Ok(None)` when the file is absent
fn read_manifest_opt(path: &Utf8Path) -> Result<Option<Manifest>, MergeTxnError> {
    let Some(bytes) = fs::read_opt(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_slice(&bytes).map_err(|e| {
        crate::error::io_err(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        )
    })?;
    Ok(Some(manifest))
}

/// Lowercased plugin filename -> the merge that already consumed its archives
fn owned_plugins(instance: &Instance) -> Result<HashMap<String, String>, MergeTxnError> {
    let mut owned = HashMap::new();
    for manifest in manifests(instance)? {
        for plugin in &manifest.plugins {
            owned.insert(plugin.to_ascii_lowercase(), manifest.name.clone());
        }
    }
    Ok(owned)
}

/// Reject a name that is not a plain basename usable as a mod/plugin stem
fn validate_name_syntax(name: &str) -> Result<(), MergeTxnError> {
    let invalid = || MergeTxnError::InvalidName(name.to_owned());
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
        return Err(invalid());
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".esp") || lower.ends_with(".esm") || lower.ends_with(".esl") {
        return Err(invalid());
    }
    Ok(())
}

/// Reject a merge name that is not a plain basename or clashes with a mod
fn validate_name(instance: &Instance, name: &str) -> Result<(), MergeTxnError> {
    validate_name_syntax(name)?;
    for installed in instance.installed_mods()? {
        if installed.name.eq_ignore_ascii_case(name) {
            return Err(MergeTxnError::NameExists(name.to_owned()));
        }
    }
    Ok(())
}

/// Reject carrier names clashing with a `Data/` plugin stem, another merge, or each other
fn check_carrier_collisions(instance: &Instance, carriers: &[String]) -> Result<(), MergeTxnError> {
    let data_dir = instance.config.game_dir.join(crate::deploy::DATA_DIR);
    let present = data_filenames(&data_dir)?;
    let mut taken: HashSet<String> = present
        .iter()
        .filter_map(|name| plugin_stem_of(name))
        .collect();
    for manifest in manifests(instance)? {
        for carrier in manifest.carriers {
            taken.insert(carrier.to_ascii_lowercase());
        }
    }
    for carrier in carriers {
        if !taken.insert(carrier.to_ascii_lowercase()) {
            return Err(MergeTxnError::CarrierCollision(carrier.clone()));
        }
    }
    Ok(())
}

/// The lowercased stem of a plugin filename, or `None` when it is not a plugin
fn plugin_stem_of(lower_name: &str) -> Option<String> {
    [".esp", ".esm", ".esl"]
        .iter()
        .find_map(|ext| lower_name.strip_suffix(ext))
        .map(str::to_owned)
}

/// The lowercased filenames directly inside `data_dir`; a missing directory is empty
fn data_filenames(data_dir: &Utf8Path) -> Result<HashSet<String>, MergeTxnError> {
    let mut names = HashSet::new();
    let Some(entries) = fs::read_dir_opt(data_dir)? else {
        return Ok(names);
    };
    for entry in entries {
        let entry = entry.map_err(|e| crate::error::io_err(data_dir, e))?;
        if let Some(name) = entry.file_name().to_str() {
            names.insert(name.to_ascii_lowercase());
        }
    }
    Ok(names)
}

/// The `Data/` path to `<stem> - <kind>.ba2` when present, matched case-insensitively
fn ba2_in_data(
    data_dir: &Utf8Path,
    present: &HashSet<String>,
    stem: &str,
    kind: &str,
) -> Option<Utf8PathBuf> {
    let name = format!("{stem} - {kind}.ba2");
    present
        .contains(&name.to_ascii_lowercase())
        .then(|| data_dir.join(name))
}

/// A plugin filename with its extension removed
fn plugin_stem(plugin: &str) -> &str {
    Utf8Path::new(plugin).file_stem().unwrap_or(plugin)
}

/// Seconds since the Unix epoch, or 0 if the clock is before it
fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "tests/transaction.rs"]
mod tests;
