//! Ordinary and incomplete rollback tests

use super::support::*;
use super::*;

#[test]
fn profile_save_failure_restores_exact_profiles_and_tree() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let alpha = "# exact\r\n+CoolMod\r\n+Keep";
    let beta = "-Other\n+CoolMod\n";
    write_modlist(&instance, "Alpha", alpha);
    let beta_path = write_modlist(&instance, "Beta", beta);
    let _fail = failpoint::scoped([(failpoint::Point::Save, beta_path)]);

    let error = remove(&instance, "CoolMod").expect_err("save failure");

    assert!(matches!(error, LifecycleError::Io(_)));
    assert_eq!(read_modlist(&instance, "Alpha"), alpha);
    assert_eq!(read_modlist(&instance, "Beta"), beta);
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn initial_tree_move_failure_removes_bundle_without_mutation() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let original = "+CoolMod\r\n+Keep";
    write_modlist(&instance, "Default", original);
    let live = instance.mods_dir().join("CoolMod");
    let _fail = failpoint::scoped([(failpoint::Point::Rename, live)]);

    let error = remove(&instance, "CoolMod").expect_err("move failure");

    assert!(matches!(error, LifecycleError::Io(_)));
    assert_eq!(read_modlist(&instance, "Default"), original);
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn failed_inverses_are_all_reported_and_leave_bundle() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let alpha = "# exact\r\n+CoolMod\r\n+Keep";
    let beta = "-Other\n+CoolMod\n";
    let alpha_path = write_modlist(&instance, "Alpha", alpha);
    let beta_path = write_modlist(&instance, "Beta", beta);
    let pending = pending_path(&instance);
    let old = pending.join("old");
    let live = instance.mods_dir().join("CoolMod");
    let _fail = failpoint::scoped([
        (failpoint::Point::Save, beta_path.clone()),
        (failpoint::Point::Restore, alpha_path.clone()),
        (failpoint::Point::Rename, old.clone()),
    ]);

    let error = remove(&instance, "CoolMod").expect_err("incomplete rollback");
    let LifecycleError::RollbackIncomplete { bundle, issues } = error else {
        panic!("expected incomplete rollback");
    };

    assert_eq!(bundle, pending);
    assert_eq!(issues.len(), 3);
    assert!(issues[0].contains(beta_path.as_str()));
    assert!(issues[1].contains(alpha_path.as_str()));
    assert!(issues[2].contains(old.as_str()));
    assert!(issues[2].contains(live.as_str()));
    assert_eq!(read_modlist(&instance, "Alpha"), "+Keep\n");
    assert_eq!(read_modlist(&instance, "Beta"), beta);
    assert!(!live.exists());
    let bundled_file = old.join("nested").join("file.txt");
    assert_eq!(
        std::fs::read_to_string(bundled_file).expect("read bundled tree"),
        "mod bytes"
    );
    assert!(bundle.join("manifest.json").is_file());
}
