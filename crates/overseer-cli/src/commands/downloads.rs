//! `overseer downloads`: list the installable archives in the instance's downloads/ dir

use crate::cli::InstanceArgs;
use crate::ui::{heading, list_item};
use anyhow::{Context, Result};
use overseer_core::install::list_downloads;

pub fn run(instance: &InstanceArgs) -> Result<()> {
    let instance = instance.load_instance()?;
    let downloads = list_downloads(&instance).context("listing downloads")?;

    if downloads.is_empty() {
        println!("No archives in the downloads/ directory.");
        return Ok(());
    }

    heading(format!("{} installable archive(s)", downloads.len()));
    for (i, entry) in downloads.iter().enumerate() {
        let suffix = if entry.installed { "  (installed)" } else { "" };
        list_item(i + 1, !entry.installed, &entry.name, suffix);
    }
    Ok(())
}
