//! Tests for the instance model and mod operations

use super::*;
use crate::instance::{ModKind, ModListEntry};
use crate::test_support::{install_mod, save_profile, temp, temp_instance};

#[test]
fn path_helpers_compose_under_root() {
    let instance = Instance::new("C:/inst", "C:/game");
    assert_eq!(instance.mods_dir(), Utf8PathBuf::from("C:/inst/mods"));
    assert_eq!(
        instance.profiles_dir(),
        Utf8PathBuf::from("C:/inst/profiles")
    );
    assert_eq!(
        instance.profile_dir("Default"),
        Utf8PathBuf::from("C:/inst/profiles/Default")
    );
}

#[test]
fn discovery_is_empty_on_a_fresh_instance() {
    // Nothing created yet: missing mods/ and profiles/ are a normal empty state
    let (_tmp, instance) = temp_instance();
    assert!(instance.installed_mods().expect("mods").is_empty());
    assert!(instance.profiles().expect("profiles").is_empty());
}

#[test]
fn installed_mods_lists_subdirs_sorted() {
    let (_tmp, instance) = temp_instance();
    for name in ["Zebra", "Alpha", "Mango"] {
        std::fs::create_dir_all(instance.mods_dir().join(name)).expect("mkdir");
    }
    // A stray file in mods/ must not be reported as a mod
    std::fs::write(instance.mods_dir().join("loose.txt"), "x").expect("write");

    let names: Vec<String> = instance
        .installed_mods()
        .expect("mods")
        .into_iter()
        .map(|m| m.name)
        .collect();
    assert_eq!(names, ["Alpha", "Mango", "Zebra"]);
}

#[test]
fn profiles_lists_profile_dirs_sorted() {
    let (_tmp, instance) = temp_instance();
    for name in ["Survival", "Default"] {
        std::fs::create_dir_all(instance.profile_dir(name)).expect("mkdir");
    }
    assert_eq!(
        instance.profiles().expect("profiles"),
        ["Default", "Survival"]
    );
}

// --- config persistence ---

fn config(game_dir: &str) -> InstanceConfig {
    InstanceConfig {
        game_dir: Utf8PathBuf::from(game_dir),
        game: GameKind::default(),
        local_dir: None,
        ini_dir: None,
        default_profile: "Default".to_owned(),
        deployer: DeployerKind::default(),
        executables: Vec::new(),
    }
}

#[test]
fn init_writes_config_and_creates_dirs() {
    let (_tmp, root) = temp();
    let instance = Instance::init(&root, config("C:/games/FO4")).expect("init");

    assert!(Instance::config_path(&root).exists());
    assert!(instance.mods_dir().is_dir());
    assert!(instance.profiles_dir().is_dir());
    assert_eq!(
        instance.config.game_dir.as_path(),
        Utf8Path::new("C:/games/FO4")
    );
}

#[test]
fn init_then_load_round_trips_the_config() {
    let (_tmp, root) = temp();
    let cfg = InstanceConfig {
        game_dir: Utf8PathBuf::from("D:/FO4"),
        game: GameKind::SkyrimSE,
        local_dir: Some(Utf8PathBuf::from("C:/Users/Me/AppData/Local/Fallout4")),
        ini_dir: None,
        default_profile: "Survival".to_owned(),
        deployer: DeployerKind::Usvfs,
        executables: vec![Executable {
            name: "xEdit".to_owned(),
            path: Utf8PathBuf::from("C:/Tools/xEdit.exe"),
            args: vec!["-FO4".to_owned()],
        }],
    };
    Instance::init(&root, cfg).expect("init");

    let loaded = Instance::load(&root).expect("load");
    assert_eq!(loaded.config.game_dir, Utf8PathBuf::from("D:/FO4"));
    assert_eq!(
        loaded.config.local_dir,
        Some(Utf8PathBuf::from("C:/Users/Me/AppData/Local/Fallout4"))
    );
    assert_eq!(loaded.config.default_profile, "Survival");
    assert_eq!(loaded.config.deployer, DeployerKind::Usvfs);
    assert_eq!(loaded.config.game, GameKind::SkyrimSE);
    assert_eq!(loaded.config.executables.len(), 1);
    assert_eq!(loaded.config.executables[0].name, "xEdit");
    assert_eq!(loaded.config.executables[0].args, ["-FO4"]);
}

#[test]
fn default_executables_seed_the_game_and_script_extender() {
    let exes =
        InstanceConfig::default_executables(GameKind::SkyrimSE, Utf8Path::new("D:/SkyrimSE"));

    assert_eq!(exes.len(), 2);
    assert_eq!(exes[0].name, "game");
    assert_eq!(exes[0].path, Utf8PathBuf::from("D:/SkyrimSE/SkyrimSE.exe"));
    assert!(exes[0].args.is_empty());
    assert_eq!(exes[1].name, "script-extender");
    assert_eq!(
        exes[1].path,
        Utf8PathBuf::from("D:/SkyrimSE/skse64_loader.exe")
    );
    assert!(exes[1].args.is_empty());
}

#[test]
fn legacy_config_without_game_key_defaults_to_fallout4() {
    // A pre-multi-game overseer.toml only had `game_dir`; serde defaults fill; in the rest, and `game` must resolve to Fallout 4 so existing instances; keep working untouched
    let cfg: InstanceConfig = toml::from_str("game_dir = \"D:/FO4\"\n").expect("legacy load");
    assert_eq!(cfg.game, GameKind::Fallout4);
    assert_eq!(cfg.default_profile, "Default");
    assert_eq!(cfg.deployer, DeployerKind::default());
    assert_eq!(cfg.local_dir, None);
    assert!(cfg.executables.is_empty());
}

#[test]
fn load_missing_config_is_not_an_instance() {
    let (_tmp, root) = temp();
    let err = Instance::load(&root).expect_err("should fail");
    assert!(matches!(err, InstanceError::NotAnInstance { .. }));
}

#[test]
fn init_refuses_to_clobber_existing_instance() {
    let (_tmp, root) = temp();
    Instance::init(&root, config("C:/a")).expect("first init");
    let err = Instance::init(&root, config("C:/b")).expect_err("should refuse");
    assert!(matches!(err, InstanceError::AlreadyAnInstance { .. }));
}

#[test]
fn omitted_local_dir_is_absent_from_the_toml_and_loads_as_none() {
    let (_tmp, root) = temp();
    Instance::init(&root, config("C:/FO4")).expect("init");

    let text = std::fs::read_to_string(Instance::config_path(&root)).expect("read");
    assert!(!text.contains("local_dir"), "None local_dir is omitted");

    let loaded = Instance::load(&root).expect("load");
    assert_eq!(loaded.config.local_dir, None);
}

#[test]
fn minimal_toml_uses_default_profile() {
    // A hand-written config with only game_dir must load with the default profile
    let (_tmp, root) = temp();
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(Instance::config_path(&root), "game_dir = \"C:/FO4\"\n").expect("write");

    let loaded = Instance::load(&root).expect("load");
    assert_eq!(loaded.config.default_profile, "Default");
    assert_eq!(loaded.config.local_dir, None);
    assert_eq!(loaded.config.deployer, DeployerKind::HardLink);
}

#[test]
fn create_profile_makes_an_empty_profile_on_disk() {
    let (_tmp, instance) = temp_instance();
    let profile = instance.create_profile("Survival").expect("create");

    assert_eq!(profile.name, "Survival");
    assert!(profile.mods.is_empty());
    // The directory and an (empty) modlist are persisted...
    assert!(instance.profile_dir("Survival").is_dir());
    assert!(
        instance
            .profile_dir("Survival")
            .join("modlist.txt")
            .exists()
    );
    // ...and the profile now shows up in the listing
    assert_eq!(instance.profiles().expect("profiles"), ["Survival"]);
}

#[test]
fn create_profile_refuses_to_overwrite_an_existing_one() {
    let (_tmp, instance) = temp_instance();
    instance.create_profile("Default").expect("first create");

    let err = instance
        .create_profile("Default")
        .expect_err("should refuse");
    assert!(matches!(err, InstanceError::ProfileExists(name) if name == "Default"));
}

#[test]
fn create_profile_rejects_a_filesystem_unsafe_name() {
    let (_tmp, instance) = temp_instance();
    let err = instance
        .create_profile("bad/name")
        .expect_err("invalid name must be rejected");
    assert!(matches!(err, InstanceError::InvalidProfileName(_)));
}

#[test]
fn rename_profile_moves_the_directory_and_its_contents() {
    let (_tmp, instance) = temp_instance();
    save_profile(&instance, "Old", &[]);
    // A file living inside the profile dir must travel with the rename
    std::fs::write(instance.profile_dir("Old").join("plugins.txt"), "*A.esp\n").expect("seed");

    instance.rename_profile("Old", "New").expect("rename");

    assert!(!instance.profile_dir("Old").exists());
    assert!(instance.profile_dir("New").is_dir());
    assert_eq!(
        std::fs::read_to_string(instance.profile_dir("New").join("plugins.txt")).expect("read"),
        "*A.esp\n"
    );
    assert_eq!(instance.profiles().expect("profiles"), ["New"]);
}

#[test]
fn rename_profile_moves_redirected_local_saves() {
    let (_tmp, instance) = temp_instance();
    let profile = Profile {
        name: "Old".to_owned(),
        mods: Vec::new(),
        local_saves: true,
    };
    profile.save(&instance).expect("save profile");
    let old_saves = instance.saves_dir("Old").expect("saves dir");
    std::fs::create_dir_all(&old_saves).expect("mk saves");
    std::fs::write(old_saves.join("Quicksave.fos"), "save").expect("seed save");

    instance.rename_profile("Old", "New").expect("rename");

    let new_saves = instance.saves_dir("New").expect("saves dir");
    assert!(!old_saves.exists(), "old saves dir moved");
    assert_eq!(
        std::fs::read_to_string(new_saves.join("Quicksave.fos")).expect("read save"),
        "save"
    );
}

#[test]
fn rename_profile_rejects_a_colliding_target() {
    let (_tmp, instance) = temp_instance();
    save_profile(&instance, "Old", &[]);
    save_profile(&instance, "Taken", &[]);

    let err = instance
        .rename_profile("Old", "taken")
        .expect_err("collision must be rejected");
    assert!(matches!(err, InstanceError::ProfileExists(name) if name == "taken"));
    assert!(instance.profile_dir("Old").is_dir());
    assert!(instance.profile_dir("Taken").is_dir());
}

#[test]
fn rename_profile_rejects_a_missing_source() {
    let (_tmp, instance) = temp_instance();
    let err = instance
        .rename_profile("Ghost", "New")
        .expect_err("missing source must be rejected");
    assert!(matches!(err, InstanceError::ProfileNotFound(name) if name == "Ghost"));
}

#[test]
fn rename_profile_rejects_invalid_and_case_only_names() {
    let (_tmp, instance) = temp_instance();
    save_profile(&instance, "Old", &[]);

    let bad = instance
        .rename_profile("Old", "a/b")
        .expect_err("invalid name must be rejected");
    assert!(matches!(bad, InstanceError::InvalidProfileName(_)));

    let case_only = instance
        .rename_profile("Old", "old")
        .expect_err("case-only rename must be rejected");
    assert!(matches!(case_only, InstanceError::InvalidProfileName(_)));
    assert!(instance.profile_dir("Old").is_dir(), "nothing moved");
}

#[test]
fn rename_mod_renames_folder_and_rewrites_referencing_profiles() {
    let (_tmp, instance) = temp_instance();
    install_mod(
        &instance,
        "CoolMod",
        &[("Cool.esp", "plugin bytes"), ("plugins.txt", "*Cool.esp\n")],
    );
    install_mod(&instance, "Other", &[("Data.txt", "other")]);
    save_profile(&instance, "Default", &[("CoolMod", true), ("Other", false)]);

    let mut survival = Profile {
        name: "Survival".to_owned(),
        mods: vec![
            ModListEntry {
                name: "Other".to_owned(),
                enabled: true,
                kind: ModKind::Managed,
            },
            ModListEntry {
                name: "CoolMod".to_owned(),
                enabled: false,
                kind: ModKind::Managed,
            },
        ],
        local_saves: false,
    };
    survival.save(&instance).expect("save survival");
    save_profile(&instance, "Clean", &[("Other", true)]);
    let clean_before = std::fs::read_to_string(instance.profile_dir("Clean").join("modlist.txt"))
        .expect("read clean before");

    instance
        .rename_mod("CoolMod", "BetterMod")
        .expect("rename mod");

    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert!(instance.mods_dir().join("BetterMod").is_dir());
    assert_eq!(
        std::fs::read_to_string(instance.mods_dir().join("BetterMod").join("Cool.esp"))
            .expect("read plugin"),
        "plugin bytes"
    );
    assert_eq!(
        std::fs::read_to_string(instance.mods_dir().join("BetterMod").join("plugins.txt"))
            .expect("read plugins.txt"),
        "*Cool.esp\n"
    );

    let default = Profile::load(&instance, "Default").expect("load default");
    assert_eq!(default.mods[0].name, "BetterMod");
    assert!(default.mods[0].enabled);
    assert_eq!(default.mods[1].name, "Other");
    assert!(!default.mods[1].enabled);

    survival = Profile::load(&instance, "Survival").expect("load survival");
    assert_eq!(survival.mods[0].name, "Other");
    assert!(survival.mods[0].enabled);
    assert_eq!(survival.mods[1].name, "BetterMod");
    assert!(!survival.mods[1].enabled);

    let clean_after = std::fs::read_to_string(instance.profile_dir("Clean").join("modlist.txt"))
        .expect("read clean after");
    assert_eq!(clean_after, clean_before, "unreferencing profile untouched");
}

#[test]
fn rename_mod_rejects_a_colliding_target_folder() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("a.txt", "a")]);
    install_mod(&instance, "Existing", &[("b.txt", "b")]);

    let err = instance
        .rename_mod("CoolMod", "existing")
        .expect_err("collision must be rejected");
    assert!(matches!(err, InstanceError::ModAlreadyInstalled(name) if name == "existing"));
    assert!(instance.mods_dir().join("CoolMod").is_dir());
    assert!(instance.mods_dir().join("Existing").is_dir());
}

#[test]
fn rename_mod_rejects_reserved_and_separator_names() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("a.txt", "a")]);

    for name in ["Foo_separator", "overwrite"] {
        let err = instance
            .rename_mod("CoolMod", name)
            .expect_err("invalid target must be rejected");
        assert!(
            matches!(err, InstanceError::InvalidModName(_)),
            "{name} should be invalid, got {err:?}"
        );
    }
}

/// Windows device names and trailing dot/space make broken, undeletable dirs, so profile creation refuses them
#[test]
fn create_profile_rejects_windows_reserved_and_trailing_dot_names() {
    let (_tmp, instance) = temp_instance();
    for name in ["CON", "nul", "Com1", "trailing.", "trailing "] {
        let err = instance
            .create_profile(name)
            .expect_err("unsafe name must be rejected");
        assert!(
            matches!(err, InstanceError::InvalidProfileName(_)),
            "{name:?} should be rejected"
        );
    }
    // Validation runs before any directory is created
    assert!(instance.profiles().expect("profiles").is_empty());
}

#[test]
fn rename_mod_rejects_case_only_and_noop_renames() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("a.txt", "a")]);

    let err = instance
        .rename_mod("CoolMod", "coolmod")
        .expect_err("case-only rename must be rejected");
    assert!(matches!(err, InstanceError::InvalidModName(_)));

    let err = instance
        .rename_mod("CoolMod", "CoolMod")
        .expect_err("same-name rename must be rejected");
    assert!(matches!(err, InstanceError::InvalidModName(_)));
}

#[test]
fn rename_mod_rejects_profiles_that_already_list_both_names() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("a.txt", "a")]);
    save_profile(
        &instance,
        "Default",
        &[("CoolMod", true), ("BetterMod", true)],
    );

    let err = instance
        .rename_mod("CoolMod", "BetterMod")
        .expect_err("both names in a profile must be rejected");
    assert!(matches!(err, InstanceError::ModAlreadyInList(name) if name == "BetterMod"));
    assert!(instance.mods_dir().join("CoolMod").is_dir());
    assert!(!instance.mods_dir().join("BetterMod").exists());
}

#[test]
fn rename_mod_reports_missing_source_mod() {
    let (_tmp, instance) = temp_instance();

    let err = instance
        .rename_mod("Missing", "BetterMod")
        .expect_err("missing mod must be rejected");
    assert!(matches!(err, InstanceError::ModNotInstalled(name) if name == "Missing"));
}
