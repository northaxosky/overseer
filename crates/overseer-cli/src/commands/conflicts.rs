//! `overseer conflicts`: files provided by more than one enabled mod

use crate::cli::ProfileArgs;
use crate::context::{load_reconciled, open_instance};
use crate::ui::{Role, heading, styled};
use anyhow::{Context, Result};
use overseer_core::deploy::detect_conflicts;

pub fn run(target: &ProfileArgs) -> Result<()> {
    let instance = open_instance(&target.instance)?;
    let profile = load_reconciled(&instance, &target.profile)?;
    let conflicts =
        detect_conflicts(&profile.deploy_sources(&instance)).context("detecting conflicts")?;

    if conflicts.is_empty() {
        println!("No file conflicts among the enabled mods.");
        return Ok(());
    }

    heading(format!(
        "{} conflicted file(s) in profile `{}`",
        conflicts.len(),
        profile.name
    ));

    for conflict in &conflicts {
        let (winner, overridden) = conflict
            .providers
            .split_last()
            .expect("a conflict has at least two providers");
        println!("  {}", conflict.relative);
        println!("    winner:     {}", styled(Role::Success, winner));
        for loser in overridden {
            println!("    overridden: {}", styled(Role::Muted, loser));
        }
    }
    Ok(())
}
