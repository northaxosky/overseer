//! End-to-end tests of the instance pipeline through overseer-core's *public* API:
//! stage mods, save a profile, deploy, inspect status, and purge. These complement the
//! in-crate unit tests by exercising the crate as an external consumer would.

use camino::Utf8PathBuf;
use overseer_core::apply::{deploy_profile, purge, status};
use overseer_core::deploy::NullSink;
use overseer_core::game::GameKind;
use overseer_core::instance::Instance;
use overseer_core::test_support::{
    FLAG_MASTER, install_mod, install_plugin, save_profile, temp_instance, write_plugin,
};

/// Absolute path of a file as it would land under the game's `Data/` directory
fn data_file(instance: &Instance, rel: &str) -> Utf8PathBuf {
    instance.config.game_dir.join("Data").join(rel)
}

/// Path of the redirected real `Plugins.txt` for this test instance
fn plugins_txt(instance: &Instance) -> Utf8PathBuf {
    instance
        .config
        .local_dir
        .as_ref()
        .expect("local dir set in tests")
        .join("Plugins.txt")
}

#[test]
fn deploy_status_purge_round_trip() {
    let (_tmp, instance) = temp_instance();
    install_mod(
        &instance,
        "Base",
        &[("Textures/base.dds", "base"), ("Meshes/m.nif", "mesh")],
    );
    // Two mods provide the same file; the higher-priority one (top of the list) wins
    install_mod(&instance, "Winner", &[("Textures/shared.dds", "winner")]);
    install_mod(&instance, "Loser", &[("Textures/shared.dds", "loser")]);
    save_profile(
        &instance,
        "Default",
        &[("Winner", true), ("Loser", true), ("Base", true)],
    );

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // Files landed under Data/, and the conflict resolved to the higher-priority mod
    assert_eq!(
        std::fs::read_to_string(data_file(&instance, "Textures/base.dds")).unwrap(),
        "base"
    );
    assert_eq!(
        std::fs::read_to_string(data_file(&instance, "Meshes/m.nif")).unwrap(),
        "mesh"
    );
    assert_eq!(
        std::fs::read_to_string(data_file(&instance, "Textures/shared.dds")).unwrap(),
        "winner"
    );

    // status reports the live deployment (one entry per distinct destination path)
    let st = status(&instance).expect("status").expect("deployed");
    assert_eq!(st.deployment.profile, "Default");
    assert!(st.verified.is_ok(), "all deployed files present");
    assert_eq!(st.deployment.record.entries.len(), 3);

    // purge removes every deployed file, the dirs it created, and clears the journal
    purge(&instance, &NullSink).expect("purge");
    assert!(!data_file(&instance, "Textures/shared.dds").exists());
    assert!(
        !data_file(&instance, "Textures").exists(),
        "created dirs removed"
    );
    assert!(
        status(&instance).expect("status").is_none(),
        "no live deployment after purge"
    );
}

#[test]
fn purge_restores_a_pre_existing_vanilla_file() {
    let (_tmp, instance) = temp_instance();
    // A vanilla file already living in Data/
    let vanilla = data_file(&instance, "Textures/vanilla.dds");
    std::fs::create_dir_all(vanilla.parent().unwrap()).unwrap();
    std::fs::write(&vanilla, "vanilla").unwrap();

    install_mod(
        &instance,
        "Overwriter",
        &[("Textures/vanilla.dds", "modded")],
    );
    save_profile(&instance, "Default", &[("Overwriter", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    assert_eq!(
        std::fs::read_to_string(&vanilla).unwrap(),
        "modded",
        "the mod overwrites vanilla while deployed"
    );

    purge(&instance, &NullSink).expect("purge");
    assert_eq!(
        std::fs::read_to_string(&vanilla).unwrap(),
        "vanilla",
        "purge restores the vanilla original byte-for-byte"
    );
}

#[test]
fn plugins_txt_is_written_with_masters_first() {
    let (_tmp, instance) = temp_instance();
    write_plugin(
        &instance.mods_dir().join("MasterMod"),
        "Master.esm",
        FLAG_MASTER,
        &[],
    );
    install_plugin(&instance, "PatchMod", "Patch.esp");
    save_profile(
        &instance,
        "Default",
        &[("PatchMod", true), ("MasterMod", true)],
    );

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // libloadorder writes the real Plugins.txt: masters sort first, '*' marks active
    let txt = std::fs::read_to_string(plugins_txt(&instance)).expect("read Plugins.txt");
    assert_eq!(txt, "*Master.esm\n*Patch.esp\n");
}

#[test]
fn deploys_a_skyrim_se_instance() {
    // The multi-game seam: a non-Fallout 4 instance deploys and writes its load order
    let (_tmp, mut instance) = temp_instance();
    instance.config.game = GameKind::SkyrimSE;
    assert_eq!(
        instance.config.game.local_appdata_dir(),
        "Skyrim Special Edition"
    );

    install_mod(&instance, "Texs", &[("Textures/se.dds", "se")]);
    install_plugin(&instance, "Texs", "SkyMod.esp");
    save_profile(&instance, "Default", &[("Texs", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    assert_eq!(
        std::fs::read_to_string(data_file(&instance, "Textures/se.dds")).unwrap(),
        "se"
    );
    assert_eq!(
        std::fs::read_to_string(plugins_txt(&instance)).unwrap(),
        "*SkyMod.esp\n"
    );
}
