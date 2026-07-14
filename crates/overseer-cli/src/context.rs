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

/// Load a profile and reconcile it in memory; reads must never persist a transient drop
pub fn load_reconciled(instance: &Instance, profile: &str) -> Result<Profile> {
    let mut p = Profile::load_existing(instance, profile)
        .with_context(|| format!("loading profile `{profile}`"))?;
    p.reconcile(instance)
        .context("reconciling with installed mods")?;
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

    /// Resolve the requested profile, falling back to the instance's configured default
    pub fn profile_name(&self, instance: &Instance) -> String {
        self.profile
            .clone()
            .unwrap_or_else(|| instance.config.default_profile.clone())
    }

    /// Open this target's instance, then load and reconcile its profile
    pub fn load_context(&self) -> Result<(Instance, Profile)> {
        let instance = self.load_instance()?;
        let name = self.profile_name(&instance);
        let profile = load_reconciled(&instance, &name)?;
        Ok((instance, profile))
    }

    /// Open this target's instance, then load its profile without mod-list reconcile
    pub fn load_profile(&self) -> Result<(Instance, Profile)> {
        let instance = self.load_instance()?;
        let name = self.profile_name(&instance);
        let profile = Profile::load_existing(&instance, &name)
            .with_context(|| format!("loading profile `{name}`"))?;
        Ok((instance, profile))
    }
}
