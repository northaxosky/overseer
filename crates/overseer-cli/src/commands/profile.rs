//! `overseer profile ...` subcommands: profile-level settings.

use anyhow::{Context, Result};

use crate::cli::{ProfileArgs, ProfileCommand, Toggle};
use crate::context::open_instance;
use crate::ui::success;
use overseer_core::instance::Profile;

pub fn run(command: ProfileCommand) -> Result<()> {
    match command {
        ProfileCommand::Saves { state, target } => saves(&target, state),
    }
}

/// Show or set the profile's `LocalSaves` flag, writing it back as-is without mod-list reconcile.
fn saves(target: &ProfileArgs, state: Option<Toggle>) -> Result<()> {
    let instance = open_instance(&target.instance)?;
    let mut profile = Profile::load(&instance, &target.profile)
        .with_context(|| format!("loading profile `{}`", target.profile))?;

    match state {
        Some(toggle) => {
            profile.local_saves = matches!(toggle, Toggle::On);
            profile.save(&instance).context("saving profile")?;
            success(format!(
                "Local saves {} for profile `{}`",
                if profile.local_saves {
                    "enabled"
                } else {
                    "disabled"
                },
                profile.name
            ));
        }
        None => {
            println!(
                "Local saves: {} (profile `{}`)",
                if profile.local_saves { "on" } else { "off" },
                profile.name
            );
        }
    }
    Ok(())
}
