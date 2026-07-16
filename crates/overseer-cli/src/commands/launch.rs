//! `overseer launch ...`: run a launch target, or list the available ones.

use anyhow::{Context, Result};
use overseer_core::instance::Instance;
use overseer_core::launch;

use crate::cli::InstanceArgs;
use crate::ui::{print_launch_targets, success};

pub fn run(name: Option<String>, instance: &InstanceArgs) -> Result<()> {
    let instance = instance.load_instance()?;
    match name {
        Some(name) => {
            launch::launch(&instance, &name).with_context(|| format!("launching `{name}`"))?;
            success(format!("Launched `{name}`"));
        }
        None => list(&instance),
    }
    Ok(())
}

fn list(instance: &Instance) {
    print_launch_targets(&launch::tools(instance));
}
