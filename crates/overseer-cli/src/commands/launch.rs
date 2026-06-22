//! `overseer launch ...`: run a launch target, or list the available ones.

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use overseer_core::instance::Instance;
use overseer_core::launch;

use crate::context::open_instance;
use crate::ui::{heading, success};

pub fn run(name: Option<String>, instance: Utf8PathBuf) -> Result<()> {
    let instance = open_instance(&instance)?;
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
    let targets = launch::targets(instance);
    if targets.is_empty() {
        println!("No launch targets configured.");
        return;
    }
    heading(format!("{} launch targets", targets.len()));
    for (i, name) in targets.iter().enumerate() {
        println!("{:>3}. {name}", i + 1);
    }
}
