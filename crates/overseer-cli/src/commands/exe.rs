//! `overseer exe ...`: manage an instance's launch targets (executables).

use anyhow::{Context, Result, bail};
use camino::Utf8PathBuf;
use overseer_core::instance::Executable;

use crate::cli::ExeCommand;
use crate::context::{absolutize, open_instance};
use crate::ui::{Role, heading, styled, success};

pub fn run(command: ExeCommand) -> Result<()> {
    match command {
        ExeCommand::List { instance } => list(instance),
        ExeCommand::Add {
            name,
            path,
            args,
            instance,
        } => add(name, path, args, instance),
        ExeCommand::Remove { name, instance } => remove(name, instance),
    }
}

fn list(instance: Utf8PathBuf) -> Result<()> {
    let instance = open_instance(&instance)?;
    let exes = &instance.config.executables;
    if exes.is_empty() {
        println!("No launch targets configured.");
        return Ok(());
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
    Ok(())
}

fn add(name: String, path: Utf8PathBuf, args: Vec<String>, instance: Utf8PathBuf) -> Result<()> {
    let mut instance = open_instance(&instance)?;
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

fn remove(name: String, instance: Utf8PathBuf) -> Result<()> {
    let mut instance = open_instance(&instance)?;
    let before = instance.config.executables.len();
    instance.config.executables.retain(|e| e.name != name);
    if instance.config.executables.len() == before {
        bail!("no launch target named `{name}`");
    }

    instance.save().context("saving instance config")?;
    success(format!("Removed launch target `{name}`"));
    Ok(())
}
