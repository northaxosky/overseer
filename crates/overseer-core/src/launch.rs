//! Resolving and running launch targets through the instance's deployment backend

use crate::deploy::{DeployError, LaunchTarget, deployer_for};
use crate::instance::Instance;
use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors from resolving or running a launch target
#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("no launch target named `{0}`")]
    UnknownTarget(String),

    #[error("`{name}` is not present at `{path}`")]
    NotInstalled { name: String, path: Utf8PathBuf },

    #[error(transparent)]
    Backend(#[from] DeployError),
}

/// Run a launch target by name through the instance's deployer
pub fn launch(instance: &Instance, name: &str) -> Result<(), LaunchError> {
    let target = resolve(instance, name)?;
    deployer_for(instance.config.deployer).launch(&target)?;
    Ok(())
}

/// The name of every launch target configured for this instance
pub fn targets(instance: &Instance) -> Vec<String> {
    instance
        .config
        .executables
        .iter()
        .map(|e| e.name.clone())
        .collect()
}

fn resolve(instance: &Instance, name: &str) -> Result<LaunchTarget, LaunchError> {
    let exe = instance
        .config
        .executables
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| LaunchError::UnknownTarget(name.to_owned()))?;

    if !exe.path.exists() {
        return Err(LaunchError::NotInstalled {
            name: name.to_owned(),
            path: exe.path.clone(),
        });
    }

    let game_dir = instance.config.game_dir.as_path();
    Ok(LaunchTarget {
        working_dir: exe.path.parent().unwrap_or(game_dir).to_owned(),
        program: exe.path.clone(),
        args: exe.args.clone(),
    })
}

#[cfg(test)]
#[path = "tests/launch.rs"]
mod tests;
