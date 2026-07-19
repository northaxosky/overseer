//! Resolving and running launch targets through the instance's deployment backend

mod marker;

use crate::apply::{ApplyError, InstanceLock};
use crate::deploy::{DeployError, LaunchHandle, LaunchTarget, deployer_for};
use crate::instance::Instance;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

static NEXT_LAUNCH_ID: AtomicU64 = AtomicU64::new(1);

/// The source of a resolved launch tool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Game,
    ScriptExtender,
    User,
}

/// Whether a tool's program can be launched
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAvailability {
    Ready,
    Missing,
    NotFile,
    Inaccessible,
}

/// A resolved launch tool: a derived game/F4SE target or a user tool
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub key: String,
    pub kind: ToolKind,
    pub name: String,
    pub program: Utf8PathBuf,
    pub args: Vec<String>,
    pub output_mod: Option<String>,
    pub availability: ToolAvailability,
}

/// Context persisted while a launched game may still be running
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchMarker {
    pub launch_id: u64,
    pub tool: String,
    pub profile: String,
    pub timestamp: u64,
    pub launcher_pid: u32,
}

/// Errors from resolving or running a launch target
#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("no launch target named `{0}`")]
    UnknownTarget(String),

    #[error("launch target `{0}` is ambiguous")]
    Ambiguous(String),

    #[error("launch target `{key}` at `{path}` is not launchable: {reason}")]
    NotLaunchable {
        key: String,
        path: Utf8PathBuf,
        reason: String,
    },

    #[error(transparent)]
    Backend(#[from] DeployError),

    #[error(transparent)]
    Apply(#[from] ApplyError),
}

/// Resolve all derived and user configured tools for this instance
pub fn tools(instance: &Instance) -> Vec<Tool> {
    let game_dir = &instance.config.game_dir;
    let game = instance.config.game;
    let derived = [
        (
            "game",
            ToolKind::Game,
            "Game",
            game_dir.join(game.executable()),
        ),
        (
            "script-extender",
            ToolKind::ScriptExtender,
            "Script Extender (F4SE)",
            game_dir.join(game.script_extender_loader()),
        ),
    ];
    derived
        .into_iter()
        .map(|(key, kind, name, program)| Tool {
            key: key.to_owned(),
            kind,
            name: name.to_owned(),
            availability: availability(&program),
            program,
            args: script_extender_args(kind),
            output_mod: None,
        })
        .chain(instance.config.tools.iter().map(|user| Tool {
            key: user.id.to_string(),
            kind: ToolKind::User,
            name: user.name.clone(),
            availability: availability(&user.path),
            program: user.path.clone(),
            args: user.args.clone(),
            output_mod: user.output_mod.clone(),
        }))
        .collect()
}

/// Extra args a derived tool needs; F4SE must wait for the game so its exit tracks the session
fn script_extender_args(kind: ToolKind) -> Vec<String> {
    match kind {
        ToolKind::ScriptExtender => vec!["-waitforclose".to_owned()],
        ToolKind::Game | ToolKind::User => Vec::new(),
    }
}

/// Inspect whether a program path points to a launchable file
pub fn availability(program: &Utf8Path) -> ToolAvailability {
    match fs::metadata(program) {
        Ok(metadata) if metadata.is_file() => ToolAvailability::Ready,
        Ok(_) => ToolAvailability::NotFile,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ToolAvailability::Missing,
        Err(_) => ToolAvailability::Inaccessible,
    }
}

/// Resolve a launch target by stable key or unambigous display name
fn resolve(instance: &Instance, key: &str) -> Result<Tool, LaunchError> {
    let all = tools(instance);
    let key_matches: Vec<&Tool> = all.iter().filter(|tool| tool.key == key).collect();
    let tool = match key_matches.as_slice() {
        [tool] => (*tool).clone(),
        [] => {
            let name_matches: Vec<&Tool> = all
                .iter()
                .filter(|tool| tool.name.eq_ignore_ascii_case(key))
                .collect();
            match name_matches.as_slice() {
                [tool] => (*tool).clone(),
                [] => return Err(LaunchError::UnknownTarget(key.to_owned())),
                _ => return Err(LaunchError::Ambiguous(key.to_owned())),
            }
        }
        _ => return Err(LaunchError::Ambiguous(key.to_owned())),
    };

    let reason = match tool.availability {
        ToolAvailability::Ready => None,
        ToolAvailability::Missing => Some("program is missing"),
        ToolAvailability::NotFile => Some("program path is not a file"),
        ToolAvailability::Inaccessible => Some("program path is inaccessible"),
    };
    if let Some(reason) = reason {
        return Err(LaunchError::NotLaunchable {
            key: tool.key,
            path: tool.program,
            reason: reason.to_owned(),
        });
    }
    let working_dir = tool
        .program
        .parent()
        .unwrap_or(instance.config.game_dir.as_path());
    match fs::metadata(working_dir) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(LaunchError::NotLaunchable {
                key: tool.key,
                path: working_dir.to_owned(),
                reason: "working directory path is not a directory".to_owned(),
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(LaunchError::NotLaunchable {
                key: tool.key,
                path: working_dir.to_owned(),
                reason: "working directory is missing".to_owned(),
            });
        }
        Err(_) => {
            return Err(LaunchError::NotLaunchable {
                key: tool.key,
                path: working_dir.to_owned(),
                reason: "working directory is inaccessible".to_owned(),
            });
        }
    }
    Ok(tool)
}

fn launch_target(instance: &Instance, tool: Tool) -> LaunchTarget {
    let working_dir = tool
        .program
        .parent()
        .unwrap_or(instance.config.game_dir.as_path())
        .to_owned();
    LaunchTarget {
        program: tool.program,
        args: tool.args,
        working_dir,
    }
}

/// Run a launch target using the instance's configured default profile
pub fn launch(instance: &Instance, key: &str) -> Result<Box<dyn LaunchHandle>, LaunchError> {
    let profile = instance.config.default_profile.clone();
    launch_tracked(instance, key, &profile).map(|(handle, _)| handle)
}

/// Run a launch target and persist its profile context
pub fn launch_for_profile(
    instance: &Instance,
    key: &str,
    profile: &str,
) -> Result<Box<dyn LaunchHandle>, LaunchError> {
    launch_tracked(instance, key, profile).map(|(handle, _)| handle)
}

/// Run a launch target and return its exact marker identity
pub fn launch_tracked(
    instance: &Instance,
    key: &str,
    profile: &str,
) -> Result<(Box<dyn LaunchHandle>, LaunchMarker), LaunchError> {
    let tool = resolve(instance, key)?;
    let target = launch_target(instance, tool.clone());
    let _lock = InstanceLock::acquire(instance)?;
    if marker::exists(instance)? {
        return Err(ApplyError::LaunchActive {
            path: marker::path(instance),
        }
        .into());
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let marker = LaunchMarker {
        launch_id: NEXT_LAUNCH_ID.fetch_add(1, Ordering::Relaxed),
        tool: tool.name,
        profile: profile.to_owned(),
        timestamp,
        launcher_pid: std::process::id(),
    };
    marker::write(instance, &marker)?;
    match deployer_for(instance.config.deployer).launch(&target) {
        Ok(handle) => Ok((handle, marker)),
        Err(error) => {
            if let Err(cleanup) = marker::remove_locked(instance) {
                tracing::warn!(%cleanup, "failed to remove marker after launch failure");
            }
            Err(error.into())
        }
    }
}

/// Return the fixed marker path for an instance
pub fn launch_marker_path(instance: &Instance) -> Utf8PathBuf {
    marker::path(instance)
}

/// Report whether a launch marker is present
pub fn has_launch_marker(instance: &Instance) -> Result<bool, ApplyError> {
    marker::exists(instance)
}

/// Clear a stale or completed launch marker under the instance lock
pub fn clear_launch_marker(instance: &Instance) -> Result<bool, ApplyError> {
    marker::clear(instance)
}

/// Clear a marker only when it still matches the expected launch
pub fn clear_launch_marker_if(
    instance: &Instance,
    expected: &LaunchMarker,
) -> Result<bool, ApplyError> {
    marker::clear_if(instance, expected)
}

/// Every launch target for this instance
pub fn targets(instance: &Instance) -> Vec<Tool> {
    tools(instance)
}

#[cfg(test)]
#[path = "tests/launch.rs"]
mod tests;
