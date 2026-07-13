//! Lifecycle replace tests

use super::support::*;
use super::*;
use crate::install::InstallError;
use crate::lifecycle::replace::replace_with;

#[test]
fn replace_preserves_profiles_and_uses_actual_installed_casing() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let modlist = write_modlist(&instance, "Default", "# exact\r\n+COOLMOD");
    let settings = instance.profile_dir("Default").join("settings.ini");
    let plugins = instance.profile_dir("Default").join("plugins.txt");
    std::fs::write(&settings, [0xff, 0x00]).expect("write settings");
    std::fs::write(&plugins, b"*Cool.esp\r\n").expect("write plugins");
    let before = [
        std::fs::read(&modlist).expect("read modlist"),
        std::fs::read(&settings).expect("read settings"),
        std::fs::read(&plugins).expect("read plugins"),
    ];
    let archive = download_zip(
        &instance,
        "Replacement.zip",
        &[("nested/file.txt", b"replacement")],
    );

    let report = replace(&instance, "coolmod", &archive).expect("replace");

    assert_eq!(report.name, "CoolMod");
    assert_eq!(report.archive.as_deref(), Some("Replacement.zip"));
    assert_eq!(
        std::fs::read_to_string(instance.mods_dir().join("CoolMod/file.txt"))
            .expect("read replacement"),
        "replacement"
    );
    assert_eq!(
        [
            std::fs::read(&modlist).expect("read modlist"),
            std::fs::read(&settings).expect("read settings"),
            std::fs::read(&plugins).expect("read plugins"),
        ],
        before
    );
    assert_eq!(
        std::fs::read_to_string(
            instance
                .mods_dir()
                .join("CoolMod")
                .join(crate::lifecycle::archive::PROVENANCE)
        )
        .expect("read provenance"),
        "format = 1\narchive = \"Replacement.zip\"\n"
    );
    assert!(!pending_path(&instance).exists());
}

#[test]
fn replace_candidate_failure_preserves_old_tree() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let original = "+CoolMod\r\n";
    write_modlist(&instance, "Default", original);
    let archive = download_zip(
        &instance,
        "Scripted.zip",
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Textures/a.dds", b"x"),
        ],
    );
    let error = replace(&instance, "CoolMod", &archive).expect_err("candidate failure");

    assert!(matches!(
        error,
        LifecycleError::Install(InstallError::Fomod)
    ));
    assert_live_tree(&instance, "CoolMod");
    assert_eq!(read_modlist(&instance, "Default"), original);
    assert!(!pending_path(&instance).exists());
}

#[test]
fn replace_post_publish_failure_rolls_back_both_trees() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let archive = download_zip(
        &instance,
        "Replacement.zip",
        &[("nested/file.txt", b"replacement")],
    );
    let error = replace_with(&instance, "CoolMod", &archive, |path| {
        Err(operation_failure(path).into())
    })
    .expect_err("post-publish failure");

    assert!(matches!(error, LifecycleError::Io(_)));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn replace_incomplete_old_restore_retains_bundle() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let archive = download_zip(
        &instance,
        "Replacement.zip",
        &[("nested/file.txt", b"replacement")],
    );
    let pending = pending_path(&instance);
    let candidate = pending.join("new");
    let old = pending.join("old");
    let live = instance.mods_dir().join("CoolMod");

    let error = replace_with(&instance, "CoolMod", &archive, |path| {
        std::fs::create_dir(&candidate).expect("occupy candidate path");
        Err(operation_failure(path).into())
    })
    .expect_err("incomplete rollback");
    let LifecycleError::RollbackIncomplete { bundle, issues } = error else {
        panic!("expected incomplete rollback");
    };

    assert_eq!(bundle, pending);
    assert_eq!(issues.len(), 3);
    assert!(issues[1].contains(candidate.as_str()));
    assert!(issues[2].contains(old.as_str()));
    assert!(live.is_dir());
    assert_live_tree_in(&old);
}

/// Assert fixed old fixture bytes under an arbitrary tree root
fn assert_live_tree_in(root: &camino::Utf8Path) {
    assert_eq!(
        std::fs::read_to_string(root.join("nested/file.txt")).expect("read old"),
        "mod bytes"
    );
}
