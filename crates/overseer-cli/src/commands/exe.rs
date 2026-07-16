//! `overseer exe ...`: manage an instance's launch targets (tools).

use anyhow::{Context, Result, bail};
use camino::Utf8PathBuf;
use overseer_core::launch::{self, ToolKind};

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
    print_launch_targets(&launch::tools(&instance));
    Ok(())
}

fn add(name: String, path: Utf8PathBuf, args: Vec<String>, instance: &InstanceArgs) -> Result<()> {
    let mut instance = instance.load_instance()?;
    let path = absolutize(&path)?;
    let id = instance
        .config
        .add_tool(name.clone(), path, args)
        .map_err(anyhow::Error::msg)?;
    instance.save().context("saving instance config")?;
    success(format!("Added launch target `{name}` ({id})"));
    Ok(())
}

fn remove(name: String, instance: &InstanceArgs) -> Result<()> {
    let mut instance = instance.load_instance()?;
    let tool = launch::tools(&instance);
    let key_matches: Vec<_> = tool.iter().filter(|tool| tool.key == name).collect();
    let target = match key_matches.as_slice() {
        [target] => *target,
        [] => {
            let name_matches: Vec<_> = tool
                .iter()
                .filter(|tool| tool.name.eq_ignore_ascii_case(&name))
                .collect();
            match name_matches.as_slice() {
                [target] => *target,
                [] => bail!("no launch target named `{name}`"),
                _ => bail!("launch target `{name}` is ambiguous"),
            }
        }
        _ => bail!("launch target `{name}` is ambiguous"),
    };
    if target.kind != ToolKind::User {
        bail!(
            "the derived launch target `{}` cannot be removed",
            target.name
        );
    }
    let removed = instance
        .config
        .remove_tool(&target.key)
        .map_err(anyhow::Error::msg)?;

    instance.save().context("saving instance config")?;
    success(format!("Removed launch target `{}`", removed.name));
    Ok(())
}
