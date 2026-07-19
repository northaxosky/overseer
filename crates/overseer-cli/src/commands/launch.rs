//! `overseer launch ...`: run a launch target, or list the available ones.

use anyhow::{Context, Result};
use overseer_core::instance::Instance;
use overseer_core::launch;

use crate::cli::InstanceArgs;
use crate::ui::{print_launch_targets, success};

pub fn run(name: Option<String>, clear: bool, instance: &InstanceArgs) -> Result<()> {
    let instance = instance.load_instance()?;
    if clear {
        let message = if launch::clear_launch_marker(&instance)? {
            "Cleared stale launch marker"
        } else {
            "No launch marker was present"
        };
        success(message);
        return Ok(());
    }
    match name {
        Some(name) => {
            let handle =
                launch::launch(&instance, &name).with_context(|| format!("launching `{name}`"))?;
            handle.detach();
            success(format!("Launched `{name}`"));
        }
        None => list(&instance),
    }
    Ok(())
}

fn list(instance: &Instance) {
    print_launch_targets(&launch::tools(instance));
}
