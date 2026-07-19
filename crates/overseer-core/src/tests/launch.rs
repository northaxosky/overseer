//! Tests for launch target resolution

use super::*;
use crate::apply::{Deployment, Status, deploy_profile};
use crate::deploy::{NullSink, ProgressEvent, ProgressSink};
use crate::instance::{ToolMutationError, UserTool};
use crate::test_support::{install_mod, install_plugin, save_profile, temp, temp_instance, touch};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

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

#[derive(Default)]
struct CountingSink {
    deployed: AtomicUsize,
    removed: AtomicUsize,
}

impl ProgressSink for CountingSink {
    fn on_event(&self, event: ProgressEvent<'_>) {
        match event {
            ProgressEvent::Deployed { .. } => {
                self.deployed.fetch_add(1, AtomicOrdering::Relaxed);
            }
            ProgressEvent::Removed { .. } => {
                self.removed.fetch_add(1, AtomicOrdering::Relaxed);
            }
            ProgressEvent::Started { .. } | ProgressEvent::Finished => {}
        }
    }
}

fn launch_fixture() -> (tempfile::TempDir, Instance) {
    let (temp, mut instance) = temp_instance();
    install_mod(&instance, "Base", &[("Textures/base.dds", "base")]);
    save_profile(&instance, "Default", &[("Base", true)]);
    let program = Utf8PathBuf::from_path_buf(std::env::current_exe().expect("current exe"))
        .expect("utf8 current exe");
    instance.config.tools = vec![UserTool::new(
        "Test Runner",
        program,
        vec!["--list".to_owned()],
    )];
    (temp, instance)
}

fn finish_launch(instance: &Instance, outcome: PrepareOutcome) {
    let PrepareOutcome::Launched { handle, .. } = outcome else {
        panic!("expected launched outcome")
    };
    handle.detach();
    clear_launch_marker(instance).expect("clear launch marker");
}

fn set_deployment_status(instance: &Instance, status: Status) {
    let mut deployment = Deployment::load(instance).expect("load deployment");
    deployment.status = status;
    deployment.committed = Some(status == Status::Committed);
    deployment.save(instance).expect("save deployment");
}

#[test]
fn absent_deployment_is_created_and_launched_under_one_lock() {
    let (_temp, instance) = launch_fixture();
    let progress = CountingSink::default();

    let outcome = prepare_and_launch(&instance, "Default", "test-runner", None, &progress)
        .expect("prepare and launch");

    assert_eq!(progress.deployed.load(AtomicOrdering::Relaxed), 1);
    assert!(Deployment::exists(&instance));
    assert!(
        instance
            .config
            .game_dir
            .join("Data/Textures/base.dds")
            .exists()
    );
    finish_launch(&instance, outcome);
}

#[test]
fn current_deployment_launches_without_deploying_again() {
    let (_temp, instance) = launch_fixture();
    deploy_profile(&instance, "Default", &NullSink).expect("seed deployment");
    let progress = CountingSink::default();

    let outcome = prepare_and_launch(&instance, "Default", "test-runner", None, &progress)
        .expect("prepare and launch");

    assert_eq!(progress.deployed.load(AtomicOrdering::Relaxed), 0);
    assert_eq!(progress.removed.load(AtomicOrdering::Relaxed), 0);
    finish_launch(&instance, outcome);
}

#[test]
fn windows_1252_plugin_names_remain_current_across_launches() {
    let (_temp, instance) = launch_fixture();
    install_plugin(&instance, "Localized", "Café.esp");
    save_profile(&instance, "Default", &[("Base", true), ("Localized", true)]);
    let first_progress = CountingSink::default();

    let first = prepare_and_launch(&instance, "Default", "test-runner", None, &first_progress)
        .expect("first launch");
    assert_eq!(first_progress.deployed.load(AtomicOrdering::Relaxed), 2);
    finish_launch(&instance, first);

    let plugins_txt = instance.local_dir().expect("local dir").join("Plugins.txt");
    assert_eq!(
        std::fs::read(plugins_txt).expect("read plugins"),
        b"*Caf\xE9.esp\n"
    );
    assert_eq!(
        crate::apply::deployment_state(&instance, "Default").expect("deployment state"),
        crate::apply::DeploymentState::Current
    );

    let second_progress = CountingSink::default();
    let second = prepare_and_launch(&instance, "Default", "test-runner", None, &second_progress)
        .expect("second launch");
    assert_eq!(second_progress.deployed.load(AtomicOrdering::Relaxed), 0);
    assert_eq!(second_progress.removed.load(AtomicOrdering::Relaxed), 0);
    finish_launch(&instance, second);
}

#[test]
fn interrupted_deployment_recovers_redeploys_and_launches_without_self_conflict() {
    let (_temp, instance) = launch_fixture();
    deploy_profile(&instance, "Default", &NullSink).expect("seed deployment");
    set_deployment_status(&instance, Status::InProgress);
    let progress = CountingSink::default();

    let outcome = prepare_and_launch(&instance, "Default", "test-runner", None, &progress)
        .expect("single locked operation");

    assert!(progress.removed.load(AtomicOrdering::Relaxed) > 0);
    assert!(progress.deployed.load(AtomicOrdering::Relaxed) > 0);
    assert_eq!(
        Deployment::load(&instance).expect("deployment").status,
        Status::Committed
    );
    finish_launch(&instance, outcome);
}

#[test]
fn recovery_failed_and_unreadable_journals_need_recovery_without_purge() {
    let (_temp, instance) = launch_fixture();
    deploy_profile(&instance, "Default", &NullSink).expect("seed deployment");
    set_deployment_status(&instance, Status::RecoveryFailed);

    let outcome = prepare_and_launch(&instance, "Default", "test-runner", None, &NullSink)
        .expect("recovery outcome");
    assert!(matches!(outcome, PrepareOutcome::NeedsRecovery { .. }));
    assert_eq!(
        Deployment::load(&instance).expect("journal remains").status,
        Status::RecoveryFailed
    );

    std::fs::write(Deployment::path(&instance), b"{not json").expect("corrupt journal");
    let outcome = prepare_and_launch(&instance, "Default", "test-runner", None, &NullSink)
        .expect("unreadable outcome");
    assert!(matches!(outcome, PrepareOutcome::NeedsRecovery { .. }));
    assert!(Deployment::exists(&instance));
}

#[test]
fn matching_consent_redeploys_stale_state_and_launches() {
    let (_temp, instance) = launch_fixture();
    deploy_profile(&instance, "Default", &NullSink).expect("seed deployment");
    install_mod(&instance, "Added", &[("Meshes/added.nif", "added")]);
    save_profile(&instance, "Default", &[("Base", true), ("Added", true)]);

    let first = prepare_and_launch(&instance, "Default", "test-runner", None, &NullSink)
        .expect("consent request");
    let PrepareOutcome::NeedsRedeploy { token, .. } = first else {
        panic!("stale deployment needs consent")
    };
    let outcome = prepare_and_launch(&instance, "Default", "test-runner", Some(token), &NullSink)
        .expect("redeploy and launch");

    assert!(
        instance
            .config
            .game_dir
            .join("Data/Meshes/added.nif")
            .exists()
    );
    finish_launch(&instance, outcome);
}

#[test]
fn stale_consent_token_returns_a_fresh_request_without_purging() {
    let (_temp, instance) = launch_fixture();
    deploy_profile(&instance, "Default", &NullSink).expect("seed deployment");
    let plugins_txt = instance.local_dir().expect("local dir").join("Plugins.txt");
    std::fs::write(&plugins_txt, b"first external edit\n").expect("first edit");

    let first = prepare_and_launch(&instance, "Default", "test-runner", None, &NullSink)
        .expect("first consent request");
    let PrepareOutcome::NeedsRedeploy {
        token: stale_token, ..
    } = first
    else {
        panic!("stale deployment needs consent")
    };
    std::fs::write(&plugins_txt, b"second external edit\n").expect("second edit");
    let deployment_before = std::fs::read(Deployment::path(&instance)).expect("journal");

    let second = prepare_and_launch(
        &instance,
        "Default",
        "test-runner",
        Some(stale_token.clone()),
        &NullSink,
    )
    .expect("fresh consent request");
    let PrepareOutcome::NeedsRedeploy {
        token: fresh_token, ..
    } = second
    else {
        panic!("moved state must not be purged")
    };

    assert_ne!(fresh_token, stale_token);
    assert_eq!(
        std::fs::read(Deployment::path(&instance)).expect("journal"),
        deployment_before
    );
}

#[test]
fn broken_deployment_redeploys_only_with_matching_consent() {
    let (_temp, instance) = launch_fixture();
    deploy_profile(&instance, "Default", &NullSink).expect("seed deployment");
    let deployed = instance.config.game_dir.join("Data/Textures/base.dds");
    std::fs::remove_file(&deployed).expect("break deployment");

    let first = prepare_and_launch(&instance, "Default", "test-runner", None, &NullSink)
        .expect("consent request");
    let PrepareOutcome::NeedsRedeploy { reason, token } = first else {
        panic!("broken deployment needs consent")
    };
    assert!(reason.contains("incomplete"));
    let outcome = prepare_and_launch(&instance, "Default", "test-runner", Some(token), &NullSink)
        .expect("repair deployment");

    assert!(deployed.exists());
    finish_launch(&instance, outcome);
}

#[test]
fn prepared_deployment_is_rebuilt_after_purge_captures_overwrite() {
    let (_temp, instance) = launch_fixture();
    deploy_profile(&instance, "Default", &NullSink).expect("seed deployment");
    let deployed = instance.config.game_dir.join("Data/Textures/base.dds");
    std::fs::remove_file(&deployed).expect("remove owned link");
    std::fs::write(&deployed, b"captured").expect("write foreign destination");
    let plugins_txt = instance.local_dir().expect("local dir").join("Plugins.txt");
    std::fs::write(&plugins_txt, b"external\n").expect("make state stale");

    let first = prepare_and_launch(&instance, "Default", "test-runner", None, &NullSink)
        .expect("consent request");
    let PrepareOutcome::NeedsRedeploy { token, .. } = first else {
        panic!("stale deployment needs consent")
    };
    let outcome = prepare_and_launch(&instance, "Default", "test-runner", Some(token), &NullSink)
        .expect("redeploy");

    assert_eq!(
        std::fs::read(&deployed).expect("deployed bytes"),
        b"captured"
    );
    let deployment = Deployment::load(&instance).expect("new deployment");
    assert!(
        deployment
            .record
            .entries
            .iter()
            .any(|entry| { entry.source == instance.overwrite_dir().join("Textures/base.dds") })
    );
    finish_launch(&instance, outcome);
}

#[test]
fn existing_marker_rejects_prepare_before_mutating_deployment() {
    let (_temp, instance) = launch_fixture();
    let path = launch_marker_path(&instance);
    std::fs::create_dir_all(path.parent().expect("marker parent")).expect("marker parent");
    std::fs::write(&path, b"active").expect("marker");

    let error = prepare_and_launch(&instance, "Default", "test-runner", None, &NullSink)
        .expect_err("marker blocks launch");

    assert!(matches!(
        error,
        LaunchError::Apply(ApplyError::LaunchActive { .. })
    ));
    assert!(!Deployment::exists(&instance));
}
