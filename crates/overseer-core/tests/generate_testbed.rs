//! End-to-end test over a generated golden instance: one `build_testbed` call stands up a
//! realistic multi-mod instance, which deploys, resolves a file conflict, and purges clean —
//! with no dependency on any real game or the maintainer's personal MO2 install.

use camino::Utf8PathBuf;
use overseer_core::apply::{deploy_profile, purge, status};
use overseer_core::deploy::{NullSink, detect_conflicts};
use overseer_core::instance::{Instance, Profile};
use overseer_core::test_support::{self, FLAG_MASTER, TestbedSpec};

/// Absolute path of a file as it would land under the game's `Data/` directory
fn data_file(instance: &Instance, rel: &str) -> Utf8PathBuf {
    instance.config.game_dir.join("Data").join(rel)
}

#[test]
fn golden_instance_deploys_resolves_conflicts_and_purges_clean() {
    let (_tmp, root) = test_support::temp();
    // Winner outranks Loser (added earlier = higher priority) and shares one file with it
    let spec = TestbedSpec::new()
        .managed("Base", true, |m| {
            m.plugin("Base.esm", FLAG_MASTER, &[])
                .loose("Textures/base.dds", b"base")
        })
        .managed("Winner", true, |m| {
            m.loose("Textures/shared.dds", b"winner")
        })
        .managed("Loser", true, |m| m.loose("Textures/shared.dds", b"loser"))
        .managed("Patch", true, |m| m.plugin("Patch.esp", 0, &["Base.esm"]));
    let instance = test_support::build_testbed(&root, &spec);

    // The two providers of the shared file collapse to one conflict, winner (higher priority) last
    let profile = Profile::load(&instance, "Default").expect("load profile");
    let conflicts = detect_conflicts(&profile.deploy_sources(&instance)).expect("detect conflicts");
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].providers, ["Loser", "Winner"]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // The higher-priority mod's bytes win the shared path; unique files land untouched
    assert_eq!(
        std::fs::read_to_string(data_file(&instance, "Textures/shared.dds")).unwrap(),
        "winner"
    );
    assert_eq!(
        std::fs::read_to_string(data_file(&instance, "Textures/base.dds")).unwrap(),
        "base"
    );
    let st = status(&instance).expect("status").expect("deployed");
    assert!(st.verified.is_ok(), "all deployed files present");

    // Purge reverses the whole transaction, leaving Data/ as it found it
    purge(&instance, &NullSink).expect("purge");
    assert!(!data_file(&instance, "Textures/shared.dds").exists());
    assert!(
        status(&instance).expect("status").is_none(),
        "no live deployment after purge"
    );
}
