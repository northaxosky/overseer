//! `overseer downloads`: list the installable archives in the instance's downloads/ dir

use crate::context::open_instance;
use crate::ui::{heading, list_item};
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use overseer_core::install::list_downloads;

pub fn run(instance_dir: Utf8PathBuf) -> Result<()> {
    let instance = open_instance(&instance_dir)?;
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
