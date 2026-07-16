//! `overseer exe ...`: manage an instance's launch targets (executables).

use anyhow::{Context, Result, bail};
use camino::Utf8PathBuf;
use overseer_core::instance::Executable;

use crate::cli::{ExeCommand, InstanceArgs};
use crate::context::absolutize;
use crate::ui::{print_launch_targets, success};

pub fn run(command: ExeCommand) -> Result<()> {
    match command {
        ExeCommand::List { instance } => list(&instance),
        ExeCommand::Add {
            name,
            path,
            args,
            instance,
        } => add(name, path, args, &instance),
        ExeCommand::Remove { name, instance } => remove(name, &instance),
    }
}

fn list(instance: &InstanceArgs) -> Result<()> {
    let instance = instance.load_instance()?;
    let exes = &instance.config.executables;
    if exes.is_empty() {
        println!("No launch targets configured");
        return Ok(());
    }
    print_launch_targets(exes);
    Ok(())
}

fn add(name: String, path: Utf8PathBuf, args: Vec<String>, instance: &InstanceArgs) -> Result<()> {
    let mut instance = instance.load_instance()?;
    if instance.config.executables.iter().any(|e| e.name == name) {
        bail!("a launch target named `{name}` already exists");
    }

    let path = absolutize(&path)?;
    let exe = Executable { name, path, args };
    let msg = format!("Added launch target `{}`", exe.name);
    instance.config.executables.push(exe);
    instance.save().context("saving instance config")?;
    success(msg);
    Ok(())
}

fn remove(name: String, instance: &InstanceArgs) -> Result<()> {
    let mut instance = instance.load_instance()?;
    let before = instance.config.executables.len();
    instance.config.executables.retain(|e| e.name != name);
    if instance.config.executables.len() == before {
        bail!("no launch target named `{name}`");
    }

    instance.save().context("saving instance config")?;
    success(format!("Removed launch target `{name}`"));
    Ok(())
}
