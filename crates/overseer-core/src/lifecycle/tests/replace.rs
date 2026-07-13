//! Lifecycle replace tests

use super::support::*;
use super::*;
use crate::install::InstallError;

#[test]
fn replace_preserves_profiles_and_uses_actual_installed_casing() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let original = "# exact\r\n+COOLMOD";
    write_modlist(&instance, "Default", original);
    download_zip(
        &instance,
        "Replacement.zip",
        &[("nested/file.txt", b"replacement")],
    );

    let report = replace(&instance, "coolmod", "Replacement.zip").expect("replace");

    assert_eq!(report.name, "CoolMod");
    assert_eq!(report.residue_warning, None);
    assert_eq!(
        std::fs::read_to_string(instance.mods_dir().join("CoolMod/file.txt"))
            .expect("read replacement"),
        "replacement"
    );
    assert_eq!(read_modlist(&instance, "Default"), original);
    assert!(
        !instance
            .mods_dir()
            .join("CoolMod/.overseer-mod.toml")
            .exists()
    );
    assert!(!pending_path(&instance).exists());
}

#[test]
fn replace_candidate_failure_preserves_old_tree() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    download_zip(
        &instance,
        "Scripted.zip",
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Textures/a.dds", b"x"),
        ],
    );

    let error = replace(&instance, "CoolMod", "Scripted.zip").expect_err("candidate failure");

    assert!(matches!(
        error,
        LifecycleError::Install(InstallError::Fomod)
    ));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn replace_publication_failure_rolls_back_both_tree_moves() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    download_zip(
        &instance,
        "Replacement.zip",
        &[("nested/file.txt", b"replacement")],
    );
    let candidate = pending_path(&instance).join("new");
    let _fail = failpoint::scoped([(failpoint::Point::Rename, candidate)]);

    let error = replace(&instance, "CoolMod", "Replacement.zip").expect_err("publication failure");

    assert!(matches!(error, LifecycleError::Io(_)));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn replace_incomplete_old_restore_reports_both_trees_as_residue() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    download_zip(
        &instance,
        "Replacement.zip",
        &[("nested/file.txt", b"replacement")],
    );
    let pending = pending_path(&instance);
    let candidate = pending.join("new");
    let old = pending.join("old");
    let _fail = failpoint::scoped([
        (failpoint::Point::Rename, candidate.clone()),
        (failpoint::Point::Rename, old.clone()),
    ]);

    let error = replace(&instance, "CoolMod", "Replacement.zip").expect_err("incomplete rollback");
    let LifecycleError::RollbackIncomplete { bundle, issues } = error else {
        panic!("expected incomplete rollback")
    };

    assert_eq!(bundle, pending);
    assert_eq!(issues.len(), 2);
    assert!(issues[0].contains(candidate.as_str()));
    assert!(issues[1].contains(old.as_str()));
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert_eq!(
        std::fs::read_to_string(old.join("nested/file.txt")).expect("read old"),
        "mod bytes"
    );
    assert_eq!(
        std::fs::read_to_string(candidate.join("file.txt")).expect("read candidate"),
        "replacement"
    );
}

#[test]
fn replace_cleanup_failure_reports_committed_old_tree_residue() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    download_zip(
        &instance,
        "Replacement.zip",
        &[("nested/file.txt", b"replacement")],
    );
    let pending = pending_path(&instance);
    let _fail = failpoint::scoped([(failpoint::Point::Cleanup, pending.clone())]);

    let report = replace(&instance, "CoolMod", "Replacement.zip").expect("committed replacement");

    assert_eq!(report.residue_warning, Some(pending.clone()));
    assert_eq!(
        std::fs::read_to_string(instance.mods_dir().join("CoolMod/file.txt"))
            .expect("read replacement"),
        "replacement"
    );
    assert_eq!(
        std::fs::read_to_string(pending.join("old/nested/file.txt")).expect("read old residue"),
        "mod bytes"
    );
}
