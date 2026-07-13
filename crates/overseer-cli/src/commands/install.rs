//! The `install` command: stage a mod from an archive into an instance.

use anyhow::{Context, Result, anyhow};
use camino::Utf8Path;
use overseer_core::lifecycle;

use crate::cli::InstanceArgs;
use crate::ui::{heading, success};

pub fn run(archive: String, instance: &InstanceArgs, name: Option<String>) -> Result<()> {
    let instance = instance.load_instance()?;

    // Default the mod name to the archive's file stem (CoolMod.7z -> CoolMod)
    let name = match name {
        Some(name) => name,
        None => Utf8Path::new(&archive)
            .file_stem()
            .ok_or_else(|| anyhow!("could not derive a mod name from `{archive}`; pass --name"))?
            .to_owned(),
    };

    heading(format!("Installing `{archive}` as `{name}`"));
    let report = lifecycle::install(&instance, &archive, &name)
        .with_context(|| format!("installing `{archive}` as `{name}`"))?;
    success(format!(
        "Installed `{}` from `{archive}` to {}",
        report.name,
        instance.mods_dir().join(&report.name)
    ));
    super::warn_lifecycle_residue(report.residue_warning);
    Ok(())
}
