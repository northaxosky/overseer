//! `overseer conflicts`: files provided by more than one enabled mod

use crate::cli::ProfileArgs;
use crate::ui::{Role, heading, styled};
use anyhow::{Context, Result};
use overseer_core::apply::deployment_sources;
use overseer_core::deploy::ConflictSnapshot;

pub fn run(target: &ProfileArgs) -> Result<()> {
    let (instance, profile) = target.load_context()?;
    let snapshot = ConflictSnapshot::build(&deployment_sources(&instance, &profile))
        .context("detecting conflicts")?;

    if snapshot.is_empty() {
        println!("No file conflicts among the enabled mods.");
        return Ok(());
    }

    heading(format!(
        "{} conflicted file(s) in profile `{}`",
        snapshot.len(),
        profile.name
    ));

    for conflict in snapshot.conflicts() {
        let Some((winner, overridden)) = conflict.providers.split_last() else {
            continue;
        };
        println!("  {}", conflict.destination);
        println!(
            "    winner:     {}",
            styled(Role::Success, winner.origin.display_name())
        );
        for loser in overridden {
            println!(
                "    overridden: {}",
                styled(Role::Muted, loser.origin.display_name())
            );
        }
    }
    Ok(())
}
