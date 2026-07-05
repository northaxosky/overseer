//! Tests for launch target resolution

use super::*;
use crate::instance::{Executable, InstanceConfig};
use crate::test_support::{temp, touch};
use camino::Utf8Path;

fn seeded_instance(root: &Utf8Path, game_dir: &Utf8Path) -> Instance {
    let mut instance = Instance::new(root.join("inst"), game_dir.to_owned());
    instance.config.executables =
        InstanceConfig::default_executables(instance.config.game, game_dir);
    instance
}

#[test]
fn resolve_points_a_built_in_at_the_game_dir() {
    let (_tmp, root) = temp();
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
    let (_tmp, root) = temp();
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
    let (_tmp, root) = temp();
    let instance = seeded_instance(&root, &root.join("game"));
    let err = resolve(&instance, "nope").expect_err("unknown target");
    assert!(matches!(err, LaunchError::UnknownTarget(name) if name == "nope"));
}

#[test]
fn resolve_reports_a_missing_program() {
    let (_tmp, root) = temp();
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
    let (_tmp, root) = temp();
    let game_dir = root.join("game");
    let mut instance = seeded_instance(&root, &game_dir);
    instance.config.executables.push(Executable {
        name: "FO4Edit".to_owned(),
        path: game_dir.join("FO4Edit.exe"),
        args: Vec::new(),
    });

    assert_eq!(targets(&instance), ["game", "script-extender", "FO4Edit"]);
}
