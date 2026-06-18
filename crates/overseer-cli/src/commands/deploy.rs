//! Low-level `deploy` and `purge` commands operating directly on staging directories.

use std::fs;

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use overseer_core::deploy::{DeployManifest, DeployPlan, Deployer, HardlinkDeployer, ModSource};

use crate::context::absolutize;
use crate::ui::{CliProgress, heading, success};

pub fn deploy(
    target: Utf8PathBuf,
    mods: Vec<Utf8PathBuf>,
    manifest_path: Utf8PathBuf,
) -> Result<()> {
    let target = absolutize(&target)?;
    let sources = mods
        .iter()
        .map(|p| {
            let abs = absolutize(p)?;
            let name = abs.file_name().unwrap_or("mod").to_string();
            Ok(ModSource::new(name, abs))
        })
        .collect::<Result<Vec<_>>>()?;

    let plan = DeployPlan::from_mods(&target, &sources).context("building deploy plan")?;
    heading(format!("Deploying {} files to {target}", plan.len()));

    let deployer = HardlinkDeployer::new();
    let manifest = deployer.deploy(&plan, &CliProgress).context("deploying")?;

    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, json).with_context(|| format!("writing {manifest_path}"))?;
    success(format!("Manifest written to {manifest_path}"));
    Ok(())
}

pub fn purge(manifest_path: Utf8PathBuf) -> Result<()> {
    let json =
        fs::read_to_string(&manifest_path).with_context(|| format!("reading {manifest_path}"))?;
    let manifest: DeployManifest = serde_json::from_str(&json).context("parsing manifest")?;

    let deployer = HardlinkDeployer::new();
    deployer
        .undeploy(&manifest, &CliProgress)
        .context("purging")?;
    success(format!(
        "Purged {} files from {}",
        manifest.files.len(),
        manifest.target_root
    ));
    Ok(())
}
