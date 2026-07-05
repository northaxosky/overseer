//! Shared command helpers: path handling and opening/reconciling an instance.

use anyhow::{Context, Result};
use camino::Utf8Path;
use overseer_core::instance::{Instance, Profile};

use crate::cli::{InstanceArgs, ProfileArgs};

pub use overseer_frontend::absolutize;

/// Open an existing instance by loading its `overseer.toml`
pub fn open_instance(instance_dir: &Utf8Path) -> Result<Instance> {
    let instance_dir = absolutize(instance_dir)?;
    Instance::load(&instance_dir).with_context(|| format!("opening instance at {instance_dir}"))
}

/// Load a profile and reconcile it against what's installed, saving only if it changed
pub fn load_reconciled(instance: &Instance, profile: &str) -> Result<Profile> {
    let mut p = Profile::load_existing(instance, profile)
        .with_context(|| format!("loading profile `{profile}`"))?;
    if p.reconcile(instance)
        .context("reconciling with installed mods")?
    {
        p.save(instance).context("saving reconciled profile")?;
    }
    Ok(p)
}

impl InstanceArgs {
    /// Open this instance with a normalized path
    pub fn load_instance(&self) -> Result<Instance> {
        open_instance(&self.instance)
    }
}

impl ProfileArgs {
    /// Open this target's instance with a normalized path
    pub fn load_instance(&self) -> Result<Instance> {
        self.instance.load_instance()
    }

    /// Open this target's instance, then load and reconcile its profile
    pub fn load_context(&self) -> Result<(Instance, Profile)> {
        let instance = self.load_instance()?;
        let profile = load_reconciled(&instance, &self.profile)?;
        Ok((instance, profile))
    }

    /// Open this target's instance, then load its profile without mod-list reconcile
    pub fn load_profile(&self) -> Result<(Instance, Profile)> {
        let instance = self.load_instance()?;
        let profile = Profile::load_existing(&instance, &self.profile)
            .with_context(|| format!("loading profile `{}`", self.profile))?;
        Ok((instance, profile))
    }
}
