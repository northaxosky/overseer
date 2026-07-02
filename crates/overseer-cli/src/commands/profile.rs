//! `overseer profile ...` subcommands: profile-level settings.

use anyhow::{Context, Result};

use crate::cli::{ProfileArgs, ProfileCommand, Toggle};
use crate::context::open_instance;
use crate::ui::success;
use camino::Utf8Path;
use overseer_core::instance::Profile;

pub fn run(command: ProfileCommand) -> Result<()> {
    match command {
        ProfileCommand::Saves { state, target } => saves(&target, state),
        ProfileCommand::New { name, instance } => new_profile(&instance, &name),
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

/// Create a new, empty profile in the instance
fn new_profile(instance_dir: &Utf8Path, name: &str) -> Result<()> {
    let instance = open_instance(instance_dir)?;
    instance
        .create_profile(name)
        .with_context(|| format!("creating profile `{name}`"))?;
    success(format!("Created profile `{name}`"));
    Ok(())
}
