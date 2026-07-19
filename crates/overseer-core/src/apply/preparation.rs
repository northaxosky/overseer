//! Pure deployment preparation and current-state classification

use super::{ApplyError, Deployment, InstanceLock, Status};
use crate::deploy::{DeployPlan, VerifyReport, deployer_for, logical_path_key};
use crate::instance::{Instance, Profile};
use crate::plugins::{self, PluginLoadOrder};
use crate::saves;
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// One pure, reusable snapshot of the requested deployment
#[derive(Debug)]
pub struct PreparedDeployment {
    pub plan: DeployPlan,
    pub plugin_order: PluginLoadOrder,
    pub local_saves: bool,
    pub(crate) profile: String,
    pub(crate) local_dir: Utf8PathBuf,
    pub(crate) save_paths: Option<(Utf8PathBuf, Utf8PathBuf)>,
}

impl PreparedDeployment {
    /// Build a deployment snapshot without changing game or profile state
    pub(crate) fn build(instance: &Instance, profile_name: &str) -> Result<Self, ApplyError> {
        let mut profile = Profile::load_existing(instance, profile_name)?;
        profile.reconcile(instance)?;
        let sources = super::deploy_sources(instance, &profile);
        let plan = DeployPlan::from_rooted_mods(&instance.config.game_dir, &sources)?;
        let (_, plugin_order) = profile.resolve_plugins(instance)?;
        let local_dir = instance.local_dir()?;
        let save_paths = profile
            .local_saves
            .then(|| save_paths(instance, &profile.name))
            .transpose()?;

        Ok(Self {
            plan,
            plugin_order,
            local_saves: profile.local_saves,
            profile: profile.name,
            local_dir,
            save_paths,
        })
    }

    /// The reconciled profile represented by this snapshot
    pub fn profile(&self) -> &str {
        &self.profile
    }
}

/// How the recorded deployment compares with a prepared request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentState {
    Absent,
    Interrupted,
    RecoveryFailed,
    Broken,
    Stale,
    Current,
}

/// Consent tied to one exact observed deployment fingerprint
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedeployToken([u8; 32]);

pub(crate) struct DeploymentObservation {
    pub(crate) state: DeploymentState,
    pub(crate) token: Option<RedeployToken>,
    pub(crate) deployment: Option<Deployment>,
}

/// Classify a requested deployment under the instance lock
pub fn deployment_state(
    instance: &Instance,
    profile_name: &str,
) -> Result<DeploymentState, ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    let prepared = PreparedDeployment::build(instance, profile_name)?;
    deployment_state_locked(instance, &prepared)
}

/// Classify a prepared deployment while the caller owns the instance lock
pub(crate) fn deployment_state_locked(
    instance: &Instance,
    prepared: &PreparedDeployment,
) -> Result<DeploymentState, ApplyError> {
    Ok(observe_deployment_locked(instance, prepared)?.state)
}

/// Classify the recorded deployment and derive its consent token
pub(crate) fn observe_deployment_locked(
    instance: &Instance,
    prepared: &PreparedDeployment,
) -> Result<DeploymentObservation, ApplyError> {
    if !Deployment::exists(instance) {
        return Ok(DeploymentObservation {
            state: DeploymentState::Absent,
            token: None,
            deployment: None,
        });
    }

    let deployment = Deployment::load(instance)?;
    let state = match deployment.status {
        Status::InProgress => DeploymentState::Interrupted,
        Status::RecoveryFailed => DeploymentState::RecoveryFailed,
        Status::Committed => {
            let verified = deployer_for(deployment.record.deployer).verify(&deployment.record);
            let live_plugins = plugins::read_plugins_txt(&prepared.local_dir)?;
            let live_redirect = if prepared.local_saves || deployment.save_redirect.is_some() {
                let custom_ini = match &prepared.save_paths {
                    Some(paths) => paths.0.clone(),
                    None => save_paths(instance, &deployment.profile)?.0,
                };
                Some(saves::read_save_redirect(&custom_ini)?)
            } else {
                None
            };
            let token = fingerprint(
                &deployment,
                &verified,
                live_plugins.as_deref(),
                live_redirect.as_ref(),
            );

            let state = if !deployment_structure_matches(instance, prepared, &deployment) {
                DeploymentState::Stale
            } else if !verified.is_complete() {
                DeploymentState::Broken
            } else if live_state_matches(prepared, live_plugins.as_deref(), live_redirect.as_ref())
            {
                DeploymentState::Current
            } else {
                DeploymentState::Stale
            };

            return Ok(DeploymentObservation {
                state,
                token: Some(token),
                deployment: Some(deployment),
            });
        }
    };

    Ok(DeploymentObservation {
        state,
        token: None,
        deployment: Some(deployment),
    })
}

/// Whether the recorded deployment logically matches the prepared profile intent
fn deployment_structure_matches(
    instance: &Instance,
    prepared: &PreparedDeployment,
    deployment: &Deployment,
) -> bool {
    deployment.profile == prepared.profile
        && deployment.record.deployer == instance.config.deployer
        && deployment.record.target_root == instance.config.game_dir
        && logical_record_map(&deployment.record.entries) == logical_plan_map(&prepared.plan)
        && plugins_match(
            &prepared.plugin_order,
            deployment.plugins_txt_intended.as_deref(),
        )
        && prepared.local_saves == deployment.save_redirect.is_some()
}

/// Whether live Plugins.txt and save redirect match the prepared intent
fn live_state_matches(
    prepared: &PreparedDeployment,
    live_plugins: Option<&[u8]>,
    live_redirect: Option<&Option<String>>,
) -> bool {
    if !plugins_match(&prepared.plugin_order, live_plugins) {
        return false;
    }
    if !prepared.local_saves {
        return true;
    }
    live_redirect.is_some_and(|value| {
        value.as_deref() == Some(saves::save_redirect_value(&prepared.profile).as_str())
    })
}

/// Compare a decoded Plugins.txt against the expected load order
fn plugins_match(expected: &PluginLoadOrder, actual: Option<&[u8]>) -> bool {
    actual.is_some_and(|bytes| {
        plugins::decode_plugins_txt(bytes).as_slice() == expected.plugins.as_slice()
    })
}

/// Normalized target->sources map for a deployment record's entries
fn logical_record_map(entries: &[crate::deploy::DeployEntry]) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::<String, Vec<String>>::new();
    for entry in entries {
        map.entry(normalized_path_key(&entry.relative))
            .or_default()
            .push(normalized_path_key(&entry.source));
    }
    for sources in map.values_mut() {
        sources.sort();
    }
    map
}

/// Normalized target->sources map for a deploy plan's files
fn logical_plan_map(plan: &DeployPlan) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::<String, Vec<String>>::new();
    for file in plan.files() {
        map.entry(normalized_path_key(&file.relative))
            .or_default()
            .push(normalized_path_key(&file.source));
    }
    for sources in map.values_mut() {
        sources.sort();
    }
    map
}

/// Backend-agnostic path key with separators folded to '/'
fn normalized_path_key(path: &Utf8Path) -> String {
    logical_path_key(path).replace('\\', "/")
}

/// Stable SHA-256 over the observed deployment and live state for a consent token
fn fingerprint(
    deployment: &Deployment,
    verified: &VerifyReport,
    live_plugins: Option<&[u8]>,
    live_redirect: Option<&Option<String>>,
) -> RedeployToken {
    let mut digest = Sha256::new();
    digest_value(&mut digest, deployment.profile.as_bytes());
    digest_value(&mut digest, &[deployment.status as u8]);
    match deployment.committed {
        Some(committed) => digest.update([1, committed as u8]),
        None => digest.update([0]),
    }
    digest_value(&mut digest, &[deployment.record.deployer as u8]);
    digest_path(&mut digest, &deployment.record.target_root);
    digest_path(&mut digest, &deployment.record.backup_root);
    for entry in &deployment.record.entries {
        digest_path(&mut digest, &entry.relative);
        digest_path(&mut digest, &entry.source);
    }
    for path in &deployment.record.created_dirs {
        digest_path(&mut digest, path);
    }
    digest_option_bytes(&mut digest, deployment.plugins_txt_backup.as_deref());
    digest_option_bytes(&mut digest, deployment.plugins_txt_intended.as_deref());
    match &deployment.save_redirect {
        Some(redirect) => {
            digest.update([1]);
            digest_option_bytes(&mut digest, redirect.original.as_deref().map(str::as_bytes));
        }
        None => digest.update([0]),
    }
    digest_value(&mut digest, &verified.expected.to_le_bytes());
    for path in &verified.missing {
        digest_path(&mut digest, path);
    }
    digest_option_bytes(&mut digest, live_plugins);
    match live_redirect {
        Some(value) => {
            digest.update([1]);
            digest_option_bytes(&mut digest, value.as_deref().map(str::as_bytes));
        }
        None => digest.update([0]),
    }
    RedeployToken(digest.finalize().into())
}

/// Feed a normalized path key into the digest
fn digest_path(digest: &mut Sha256, path: &Utf8Path) {
    digest_value(digest, normalized_path_key(path).as_bytes());
}

/// Feed presence-tagged optional bytes into the digest
fn digest_option_bytes(digest: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            digest.update([1]);
            digest_value(digest, value);
        }
        None => digest.update([0]),
    }
}

/// Feed length-prefixed bytes into the digest to avoid boundary collisions
fn digest_value(digest: &mut Sha256, value: &[u8]) {
    digest.update(value.len().to_le_bytes());
    digest.update(value);
}

/// Compute this profile's custom INI and saves directory
pub(crate) fn save_paths(
    instance: &Instance,
    profile: &str,
) -> Result<(Utf8PathBuf, Utf8PathBuf), ApplyError> {
    let ini_dir = instance.ini_dir()?;
    let stem = instance.config.game.ini_stem();
    let custom_ini = ini_dir.join(format!("{stem}Custom.ini"));
    let saves_dir = instance.saves_dir(profile)?;
    Ok((custom_ini, saves_dir))
}

#[cfg(test)]
#[path = "tests/preparation.rs"]
mod tests;
