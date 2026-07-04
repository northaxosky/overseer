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

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{Executable, InstanceConfig};
    use camino::Utf8Path;
    use tempfile::TempDir;

    fn temp_root() -> (TempDir, Utf8PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
        (dir, root)
    }

    fn touch(path: &Utf8Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, b"").expect("touch");
    }

    fn seeded_instance(root: &Utf8Path, game_dir: &Utf8Path) -> Instance {
        let mut instance = Instance::new(root.join("inst"), game_dir.to_owned());
        instance.config.executables =
            InstanceConfig::default_executables(instance.config.game, game_dir);
        instance
    }

    #[test]
    fn resolve_points_a_built_in_at_the_game_dir() {
        let (_tmp, root) = temp_root();
        let game_dir = root.join("game");
        let instance = seeded_instance(&root, &game_dir);
        touch(&game_dir.join("Fallout4.exe"));

        let target = resolve(&instance, "game").expect("resolve game");
        assert_eq!(target.program, game_dir.join("Fallout4.exe"));
        assert_eq!(target.working_dir, game_dir);
        assert!(target.args.is_empty());
    }

    #[test]
    fn resolve_uses_a_tools_own_dir_and_args() {
        let (_tmp, root) = temp_root();
        let game_dir = root.join("game");
        let tool = root.join("tools").join("FO4Edit.exe");
        touch(&tool);

        let mut instance = seeded_instance(&root, &game_dir);
        instance.config.executables.push(Executable {
            name: "FO4Edit".to_owned(),
            path: tool.clone(),
            args: vec!["-FO4".to_owned()],
        });

        let target = resolve(&instance, "FO4Edit").expect("resolve tool");
        assert_eq!(target.program, tool);
        assert_eq!(target.working_dir, tool.parent().unwrap().to_owned());
        assert_eq!(target.args, ["-FO4"]);
    }

    #[test]
    fn resolve_rejects_an_unknown_target() {
        let (_tmp, root) = temp_root();
        let instance = seeded_instance(&root, &root.join("game"));
        let err = resolve(&instance, "nope").expect_err("unknown target");
        assert!(matches!(err, LaunchError::UnknownTarget(name) if name == "nope"));
    }

    #[test]
    fn resolve_reports_a_missing_program() {
        let (_tmp, root) = temp_root();
        let game_dir = root.join("game");
        let instance = seeded_instance(&root, &game_dir);
        // Fallout4.exe is deliberately never created

        let err = resolve(&instance, "game").expect_err("missing program");
        assert!(matches!(
            err,
            LaunchError::NotInstalled { name, path }
                if name == "game" && path == game_dir.join("Fallout4.exe")
        ));
    }

    #[test]
    fn targets_lists_every_configured_name() {
        let (_tmp, root) = temp_root();
        let game_dir = root.join("game");
        let mut instance = seeded_instance(&root, &game_dir);
        instance.config.executables.push(Executable {
            name: "FO4Edit".to_owned(),
            path: game_dir.join("FO4Edit.exe"),
            args: Vec::new(),
        });

        assert_eq!(targets(&instance), ["game", "script-extender", "FO4Edit"]);
    }
}
