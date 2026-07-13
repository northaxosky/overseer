//! Lifecycle remove success and residue tests

use super::support::*;
use super::*;
use crate::instance::Profile;

#[test]
fn remove_commits_case_insensitively_without_rewriting_profiles() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let alpha = "# exact\r\n+CoolMod\r\n";
    let zulu = "-coolmod\n";
    write_modlist(&instance, "Alpha", alpha);
    write_modlist(&instance, "Zulu", zulu);

    let report = remove(&instance, "cOoLmOd").expect("remove mod");

    assert_eq!(report.name, "CoolMod");
    assert_eq!(report.residue_warning, None);
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert_eq!(read_modlist(&instance, "Alpha"), alpha);
    assert_eq!(read_modlist(&instance, "Zulu"), zulu);
    assert!(!pending_path(&instance).exists());
}

#[test]
fn profile_reconcile_drops_a_removed_mod_lazily() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    install_tree(&instance, "Keep");
    write_modlist(&instance, "Default", "+CoolMod\r\n-Keep\r\n");
    remove(&instance, "CoolMod").expect("remove");

    let mut profile = Profile::load_existing(&instance, "Default").expect("load profile");
    assert!(profile.reconcile(&instance).expect("reconcile"));

    assert!(!profile.contains("CoolMod"));
    assert!(profile.contains("Keep"));
}

#[test]
fn remove_cleanup_failure_reports_committed_old_tree_residue() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let pending = pending_path(&instance);
    let _fail = failpoint::scoped([(failpoint::Point::Cleanup, pending.clone())]);

    let report = remove(&instance, "coolmod").expect("committed remove");

    assert_eq!(report.residue_warning, Some(pending.clone()));
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert_eq!(
        std::fs::read_to_string(pending.join("old/nested/file.txt")).expect("read residue"),
        "mod bytes"
    );
}

#[test]
fn remove_rename_failure_cleans_pending_without_mutation() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let live = instance.mods_dir().join("CoolMod");
    let _fail = failpoint::scoped([(failpoint::Point::Rename, live)]);

    let error = remove(&instance, "CoolMod").expect_err("rename failure");

    assert!(matches!(error, LifecycleError::Io(_)));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}
