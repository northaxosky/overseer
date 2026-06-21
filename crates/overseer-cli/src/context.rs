//! Shared command helpers: path handling and opening/reconciling an instance.

use anyhow::{Context, Result};
use camino::Utf8Path;
use overseer_core::instance::{Instance, Profile};

pub use overseer_frontend::absolutize;

/// Open an existing instance by loading its `overseer.toml`
pub fn open_instance(instance_dir: &Utf8Path) -> Result<Instance> {
    Instance::load(instance_dir).with_context(|| format!("opening instance at {instance_dir}"))
}

/// Load a profile and reconcile it against what's installed, saving only if it changed.
pub fn load_reconciled(instance: &Instance, profile: &str) -> Result<Profile> {
    let mut p =
        Profile::load(instance, profile).with_context(|| format!("loading profile `{profile}`"))?;
    if p.reconcile(instance)
        .context("reconciling with installed mods")?
    {
        p.save(instance).context("saving reconciled profile")?;
    }
    Ok(p)
}
