//! Shared command helpers: path handling and opening/reconciling an instance.

use anyhow::{Context, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::instance::{Instance, Profile};

/// Resolve a possibly-relative path against the current working directory.
pub fn absolutize(path: &Utf8Path) -> Result<Utf8PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    let cwd = std::env::current_dir()?;
    let cwd = Utf8PathBuf::from_path_buf(cwd).map_err(|_| anyhow!("cwd is not valid UTF-8"))?;
    Ok(cwd.join(path))
}

/// Open an instance rooted at `instance_dir`.
///
/// The game directory is a placeholder for now (install/list/plugins don't use it); it
/// will come from a persisted instance config once deploy-to-a-real-`Data/` lands.
pub fn open_instance(instance_dir: &Utf8Path) -> Instance {
    Instance::new(instance_dir, instance_dir.join("game"))
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
