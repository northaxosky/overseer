//! Tests for launch target resolution

use super::*;
use crate::instance::{ToolMutationError, UserTool};
use crate::test_support::{temp, touch};

#[test]
fn tools_always_include_derived_targets_then_users() {
    let (_tmp, root) = temp();
    let game_dir = root.join("game");
    let ready = root.join("tools/ready.exe");
    let directory = root.join("tools/not-file");
    touch(&ready);
    std::fs::create_dir_all(&directory).unwrap();
    let mut instance = Instance::new(root.join("inst"), &game_dir);
    instance.config.tools = vec![
        UserTool::new("Ready", &ready, Vec::new()),
        UserTool::new("Directory", &directory, Vec::new()),
        UserTool::new("Missing", root.join("missing.exe"), Vec::new()),
    ];

    let all = tools(&instance);
    assert_eq!(all[0].key, "game");
    assert_eq!(all[0].kind, ToolKind::Game);
    assert_eq!(all[0].program, game_dir.join("Fallout4.exe"));
    assert_eq!(all[1].key, "script-extender");
    assert_eq!(all[1].kind, ToolKind::ScriptExtender);
    assert_eq!(all[1].program, game_dir.join("f4se_loader.exe"));
    assert_eq!(all[2].availability, ToolAvailability::Ready);
    assert_eq!(all[3].availability, ToolAvailability::NotFile);
    assert_eq!(all[4].availability, ToolAvailability::Missing);
}

#[test]
fn resolve_supports_keys_and_unambiguous_legacy_names() {
    let (_tmp, root) = temp();
    let program = root.join("tools/FO4Edit.exe");
    touch(&program);
    let mut instance = Instance::new(root.join("inst"), root.join("game"));
    instance.config.tools = vec![UserTool::new("FO4Edit", &program, vec!["-FO4".to_owned()])];

    let by_key = resolve(&instance, "fo4edit").unwrap();
    let by_name = resolve(&instance, "fo4EDIT").unwrap();
    assert_eq!(by_key, by_name);
    assert_eq!(by_key.program, program);
    assert_eq!(by_key.args, ["-FO4"]);
}

#[test]
fn resolve_rejects_ambiguous_names_and_duplicate_keys() {
    let (_tmp, root) = temp();
    let one = root.join("one.exe");
    let two = root.join("two.exe");
    touch(&one);
    touch(&two);
    let mut instance = Instance::new(root.join("inst"), root.join("game"));
    let first = UserTool::new("Same", &one, Vec::new());
    let mut second = UserTool::new("same", &two, Vec::new());
    second.id = first.id.clone();
    instance.config.tools = vec![first, second];

    assert!(matches!(
        resolve(&instance, "Same"),
        Err(LaunchError::Ambiguous(_))
    ));
    assert!(matches!(
        resolve(&instance, "same"),
        Err(LaunchError::Ambiguous(_))
    ));
}

#[test]
fn resolve_preflights_program_and_working_directory() {
    let (_tmp, root) = temp();
    let instance = Instance::new(root.join("inst"), root.join("missing-game"));
    assert!(matches!(
        resolve(&instance, "game"),
        Err(LaunchError::NotLaunchable { reason, .. }) if reason.contains("missing")
    ));

    let mut no_parent = Instance::new(root.join("inst"), root.join("game"));
    no_parent.config.tools = vec![UserTool::new("Bare", "Cargo.toml", Vec::new())];
    assert!(matches!(
        resolve(&no_parent, "bare"),
        Err(LaunchError::NotLaunchable { reason, .. })
            if reason.contains("working directory")
    ));
}

#[test]
fn derived_game_builds_the_spawn_target_with_behavior_parity() {
    let (_tmp, root) = temp();
    let game_dir = root.join("game");
    touch(&game_dir.join("Fallout4.exe"));
    let instance = Instance::new(root.join("inst"), &game_dir);
    let tool = resolve(&instance, "game").unwrap();
    let target = launch_target(&instance, tool);
    assert_eq!(target.program, game_dir.join("Fallout4.exe"));
    assert_eq!(target.working_dir, game_dir);
    assert!(target.args.is_empty());
}

#[test]
fn derived_script_extender_waits_for_the_game_to_close() {
    let (_tmp, root) = temp();
    let game_dir = root.join("game");
    touch(&game_dir.join("f4se_loader.exe"));
    let instance = Instance::new(root.join("inst"), &game_dir);
    let tool = resolve(&instance, "script-extender").unwrap();
    let target = launch_target(&instance, tool);
    assert_eq!(target.args, ["-waitforclose"]);
}

#[test]
fn core_mutations_refuse_derived_tools() {
    let mut instance = Instance::new("C:/inst", "C:/game");
    assert_eq!(
        instance.config.remove_tool("game").unwrap_err(),
        ToolMutationError::Derived("game".to_owned())
    );
    assert_eq!(
        instance
            .config
            .rename_tool("script-extender", "Other".to_owned())
            .unwrap_err(),
        ToolMutationError::Derived("script-extender".to_owned())
    );
    assert_eq!(
        instance
            .config
            .set_tool_args("game", vec!["-x".to_owned()])
            .unwrap_err(),
        ToolMutationError::Derived("game".to_owned())
    );

    instance.config.tools = vec![UserTool::new("FO4Edit", "C:/tool.exe", Vec::new())];
    assert_eq!(
        instance
            .config
            .rename_tool("fo4edit", "Game".to_owned())
            .unwrap_err(),
        ToolMutationError::DuplicateName("Game".to_owned())
    );
}

#[test]
fn marker_records_context_and_clear_removes_it() {
    let (_tmp, root) = temp();
    let instance = Instance::new(root.join("instance"), root.join("game"));
    let expected = LaunchMarker {
        launch_id: 3,
        tool: "Script Extender".to_owned(),
        profile: "Survival".to_owned(),
        timestamp: 42,
        launcher_pid: 7,
    };
    {
        let _lock = InstanceLock::acquire(&instance).expect("lock");
        marker::write(&instance, &expected).expect("write marker");
    }

    assert!(has_launch_marker(&instance).expect("marker query"));
    let actual: LaunchMarker =
        serde_json::from_slice(&std::fs::read(launch_marker_path(&instance)).expect("read marker"))
            .expect("decode marker");
    assert_eq!(actual, expected);
    let foreign = LaunchMarker {
        launch_id: 4,
        ..expected.clone()
    };
    assert!(!clear_launch_marker_if(&instance, &foreign).expect("foreign marker stays"));
    assert!(has_launch_marker(&instance).expect("marker query"));
    assert!(clear_launch_marker_if(&instance, &expected).expect("owned marker clears"));
    assert!(!has_launch_marker(&instance).expect("marker query"));
    assert!(!clear_launch_marker(&instance).expect("idempotent clear"));
}

#[test]
fn failed_launch_removes_its_provisional_marker() {
    let (_tmp, root) = temp();
    let program = root.join("not-an-executable.txt");
    touch(&program);
    let mut instance = Instance::new(root.join("instance"), root.join("game"));
    instance.config.tools = vec![UserTool::new("Bad", program, Vec::new())];

    let Err(_) = launch(&instance, "bad") else {
        panic!("invalid executable must fail");
    };

    assert!(!has_launch_marker(&instance).expect("marker query"));
}

#[test]
fn existing_marker_rejects_a_second_core_launch() {
    let (_tmp, root) = temp();
    let program = Utf8PathBuf::from_path_buf(std::env::current_exe().expect("current exe"))
        .expect("utf8 exe");
    let mut instance = Instance::new(root.join("instance"), root.join("game"));
    instance.config.tools = vec![UserTool::new("Self", program, Vec::new())];
    {
        let _lock = InstanceLock::acquire(&instance).expect("lock");
        marker::write(
            &instance,
            &LaunchMarker {
                launch_id: 1,
                tool: "Other".to_owned(),
                profile: "Default".to_owned(),
                timestamp: 1,
                launcher_pid: 1,
            },
        )
        .expect("marker");
    }

    let Err(error) = launch(&instance, "self") else {
        panic!("marker must reject a second launch");
    };

    assert!(matches!(
        error,
        LaunchError::Apply(ApplyError::LaunchActive { .. })
    ));
}
