//! `overseer saves`: list or delete a profile's save files

use crate::cli::{ProfileArgs, SaveCommand};
use crate::context::open_instance;
use crate::ui::{heading, list_item, success};
use anyhow::{Context, Result};
use overseer_core::saves;

pub fn run(command: SaveCommand) -> Result<()> {
    match command {
        SaveCommand::List { target } => list(&target),
        SaveCommand::Delete { file, target } => delete(&target, &file),
    }
}

fn list(target: &ProfileArgs) -> Result<()> {
    let instance = open_instance(&target.instance)?;
    let dir = instance
        .saves_dir(&target.profile)
        .context("resolving the saves directory")?;
    let saves = saves::list_saves(&dir).context("listing saves")?;

    if saves.is_empty() {
        println!("No saves for profile `{}`.", target.profile);
        return Ok(());
    }

    heading(format!(
        "{} save(s) for profile `{}` (newest first)",
        saves.len(),
        target.profile
    ));
    for (i, save) in saves.iter().enumerate() {
        let detail = match &save.meta {
            Some(m) => {
                format!(
                    "  {} · L{} · {} · {}",
                    m.character, m.level, m.location, m.game_date
                )
            }
            None => "  (header unreadable)".to_owned(),
        };
        list_item(i + 1, true, &save.file_name, &detail);
    }
    Ok(())
}

fn delete(target: &ProfileArgs, file: &str) -> Result<()> {
    let instance = open_instance(&target.instance)?;
    let dir = instance
        .saves_dir(&target.profile)
        .context("resolving the saves directory")?;
    saves::delete_save(&dir.join(file)).with_context(|| format!("deleting save `{file}`"))?;
    success(format!("Deleted save `{file}` and its co-save"));
    Ok(())
}
