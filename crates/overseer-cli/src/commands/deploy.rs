//! Instance-aware `deploy` & `purge`: apply a profile to the game's Data/ directory

use crate::cli::ProfileArgs;
use crate::context::{absolutize, open_instance};
use crate::ui::{CliProgress, check, heading, success};
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
        deployment.record.entries.len(),
        deployment.record.target_root
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

pub fn status(instance_dir: Utf8PathBuf) -> Result<()> {
    let instance = open_instance(&absolutize(&instance_dir)?)?;
    heading(format!("Status for {}", instance.root));

    match apply::status(&instance).context("Reading deployment status")? {
        None => println!("  No live deployment. Run `overseer deploy --instance <dir>`"),
        Some(status) => {
            let record = &status.deployment.record;
            println!("  profile:     {}", status.deployment.profile);
            println!("  deployer:    {}", record.deployer);
            println!("  files:       {}", record.entries.len());
            println!("  target:      {}", record.target_root);
            let backup = if status.deployment.plugins_txt_backup.is_some() {
                "Backed up"
            } else {
                "None (no prior file)"
            };
            println!("  Plugins.txt: {backup}");

            check("All deployed files present", status.verified.is_ok());
            for missing in &status.verified.missing {
                println!("    missing: {missing}");
            }
        }
    }
    Ok(())
}
