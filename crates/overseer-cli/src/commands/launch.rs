//! `overseer launch ...`: run a launch target, or list the available ones.

use anyhow::{Context, Result};
use overseer_core::instance::Instance;
use overseer_core::launch;

use crate::cli::InstanceArgs;
use crate::ui::{Role, heading, styled, success};

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
    let exes = &instance.config.executables;
    if exes.is_empty() {
        println!("No launch targets configured.");
        return;
    }
    heading(format!("{} launch targets", exes.len()));
    for exe in exes {
        let status = if exe.path.exists() {
            styled(Role::Success, "installed")
        } else {
            styled(Role::Warning, "missing")
        };
        println!("  {} [{status}]", exe.name);
        println!("      {}", styled(Role::Muted, &exe.path));
        if !exe.args.is_empty() {
            println!(
                "      {}",
                styled(Role::Muted, format!("args: {}", exe.args.join(" ")))
            );
        }
    }
}
