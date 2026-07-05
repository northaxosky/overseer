//! The `install` command: stage a mod from an archive into an instance.

use anyhow::{Context, Result, anyhow};
use camino::Utf8PathBuf;

use crate::cli::InstanceArgs;
use crate::context::absolutize;
use crate::ui::{heading, success};

pub fn run(archive: Utf8PathBuf, instance: &InstanceArgs, name: Option<String>) -> Result<()> {
    let archive = absolutize(&archive)?;
    let instance = instance.load_instance()?;

    // Default the mod name to the archive's file stem (CoolMod.7z -> CoolMod)
    let name = match name {
        Some(name) => name,
        None => archive
            .file_stem()
            .ok_or_else(|| anyhow!("could not derive a mod name from `{archive}`; pass --name"))?
            .to_owned(),
    };

    heading(format!("Installing {archive} as `{name}`"));
    let installed = overseer_core::install::install(&instance, &archive, &name)
        .with_context(|| format!("installing {archive}"))?;
    success(format!(
        "Installed `{}` to {}",
        installed.name,
        instance.mods_dir().join(&installed.name)
    ));
    Ok(())
}
