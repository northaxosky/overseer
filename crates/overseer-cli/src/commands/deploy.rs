//! Instance-aware `deploy` & `purge`: apply a profile to the game's Data/ directory

use crate::cli::{InstanceArgs, ProfileArgs};
use crate::ui::{CliProgress, Role, check, heading, styled, success};
use anyhow::{Context, Result};
use overseer_core::apply;
use overseer_core::restore::Restore;

pub fn deploy(target: &ProfileArgs) -> Result<()> {
    let instance = target.load_instance()?;
    let profile = target.profile_name(&instance);
    heading(format!("Deploying profile `{profile}`"));

    let deployment = apply::deploy_profile(&instance, &profile, &CliProgress)
        .with_context(|| format!("deploying profile `{profile}`"))?;

    success(format!(
        "Deployed {} files to {}",
        deployment.record.entries.len(),
        deployment.record.target_root
    ));
    Ok(())
}

pub fn purge(instance: &InstanceArgs) -> Result<()> {
    let instance = instance.load_instance()?;
    heading(format!("Purging deployment for {}", instance.root));

    let outcome = apply::purge(&instance, &CliProgress).context("purging deployment")?;

    success(purge_summary(&outcome));
    if outcome.plugins_txt == Restore::Conflict {
        println!(
            "{}",
            styled(
                Role::Warning,
                "warning: Plugins.txt changed externally and was left unchanged"
            )
        );
    }
    if outcome.save_redirect == Restore::Conflict {
        println!(
            "{}",
            styled(
                Role::Warning,
                "warning: SLocalSavePath changed externally and was left unchanged"
            )
        );
    }
    Ok(())
}

fn purge_summary(outcome: &apply::ReversalOutcome) -> String {
    format!(
        "Purged deployment: {} links removed, {} originals restored, {} files captured, {} conflicts preserved",
        outcome.removed.len(),
        outcome.restored.len(),
        outcome.captured.len(),
        outcome.preserved_conflicts.len()
    )
}

pub fn status(instance: &InstanceArgs) -> Result<()> {
    let instance = instance.load_instance()?;
    heading(format!("Status for {}", instance.root));

    match apply::status(&instance).context("reading deployment status")? {
        None => println!("  No live deployment. Run `overseer deploy --instance <dir>`."),
        Some(status) => {
            let record = &status.deployment.record;
            println!("  profile:     {}", status.deployment.profile);
            println!("  state:       {:?}", status.deployment.status);
            println!("  deployer:    {}", record.deployer);
            println!("  files:       {}", record.entries.len());
            println!("  target:      {}", record.target_root);
            let backup = if status.deployment.plugins_txt_backup.is_some() {
                "Backed up"
            } else {
                "None (no prior file)"
            };
            println!("  Plugins.txt: {backup}");

            check("All deployed files present", status.verified.is_complete());
            for missing in &status.verified.missing {
                println!(
                    "    {}",
                    styled(Role::Warning, format!("missing: {missing}"))
                );
            }
            if status.deployment.status != apply::Status::Committed {
                println!(
                    "{}",
                    styled(
                        Role::Warning,
                        "    interrupted deployment recorded; run `overseer purge` before continuing"
                    )
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purge_summary_includes_every_reported_count() {
        let mut outcome = apply::ReversalOutcome::default();
        outcome.removed.push("Data/a".into());
        outcome.restored.push("Data/b".into());
        outcome.captured.push(apply::CapturedPath {
            game_relative: "Data/c".into(),
            overwrite_relative: "c".into(),
        });
        outcome
            .preserved_conflicts
            .push(overseer_core::deploy::PreservedConflict {
                path: "Data/d".into(),
                preserved_at: "Data/d".into(),
                reason: "test".to_owned(),
                blocking: false,
            });

        assert_eq!(
            purge_summary(&outcome),
            "Purged deployment: 1 links removed, 1 originals restored, 1 files captured, 1 conflicts preserved"
        );
    }
}
