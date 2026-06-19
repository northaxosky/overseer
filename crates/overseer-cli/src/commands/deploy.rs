//! Instance-aware `deploy` & `purge`: apply a profile to the game's Data/ directory

use crate::cli::ProfileArgs;
use crate::context::{absolutize, open_instance};
use crate::ui::{CliProgress, heading, success};
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use overseer_core::apply;

pub fn deploy(target: ProfileArgs) -> Result<()> {
    let instance = open_instance(&absolutize(&target.instance)?)?;
    heading(format!("Deploying profile `{}`", target.profile));

    let deployment = apply::deploy_profile(&instance, &target.profile, &CliProgress)
        .with_context(|| format!("Deploying profile `{}`", target.profile))?;

    success(format!(
        "Deployed {} files to {}",
        deployment.manifest.files.len(),
        deployment.manifest.target_root
    ));
    Ok(())
}

pub fn purge(instance_dir: Utf8PathBuf) -> Result<()> {
    let instance = open_instance(&absolutize(&instance_dir)?)?;
    heading(format!("Purging deployment for {}", instance.root));

    apply::purge(&instance, &CliProgress).context("Purging deployment")?;

    success("Purged the live deployment");
    Ok(())
}
