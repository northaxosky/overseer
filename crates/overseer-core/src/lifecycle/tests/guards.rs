//! Lifecycle entry guard and preflight tests

use std::io::ErrorKind;

use camino::Utf8Path;

use super::support::*;
use super::*;
use crate::apply::InstanceLock;
use crate::instance::InstanceError;

#[test]
fn strict_probe_propagates_metadata_errors() {
    let invalid = Utf8Path::new("pending\0metadata");

    let error = bundle::occupied(invalid).expect_err("metadata error");

    assert_eq!(error.path, invalid);
    assert_ne!(error.source.kind(), ErrorKind::NotFound);
}

#[test]
fn pending_occupant_has_precedence_and_remains_untouched() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let pending = pending_path(&instance);
    std::fs::create_dir_all(instance.state_dir()).expect("create state");
    std::fs::write(&pending, "pending bytes").expect("write pending occupant");
    std::fs::create_dir(instance.state_dir().join("deployment.json")).expect("deployment occupant");

    let error = remove(&instance, "CoolMod").expect_err("pending must block");

    assert!(matches!(
        error,
        LifecycleError::PendingOperation { path } if path == pending
    ));
    assert_eq!(
        std::fs::read_to_string(pending).expect("read pending"),
        "pending bytes"
    );
    assert_live_tree(&instance, "CoolMod");
}

#[test]
fn any_deployment_path_occupant_blocks_lifecycle_operations() {
    for operation in 0..3 {
        let (_temp, instance) = instance();
        install_tree(&instance, "CoolMod");
        download_zip(
            &instance,
            "Archive.zip",
            &[("nested/file.txt", b"replacement")],
        );
        let deployment = instance.state_dir().join("deployment.json");
        std::fs::create_dir_all(&deployment).expect("create deployment occupant");

        let result = match operation {
            0 => install(&instance, "Archive.zip", "NewMod"),
            1 => remove(&instance, "CoolMod"),
            _ => replace(&instance, "CoolMod", "Archive.zip"),
        };
        let error = result.expect_err("deployment must block");

        assert!(matches!(
            error,
            LifecycleError::DeploymentExists { path } if path == deployment
        ));
        assert_live_tree(&instance, "CoolMod");
        assert!(!pending_path(&instance).exists());
    }
}

#[test]
fn pending_residue_blocks_all_lifecycle_operations() {
    for operation in 0..3 {
        let (_temp, instance) = instance();
        install_tree(&instance, "CoolMod");
        download_zip(
            &instance,
            "Archive.zip",
            &[("nested/file.txt", b"replacement")],
        );
        let pending = pending_path(&instance);
        std::fs::create_dir_all(instance.state_dir()).expect("create state");
        std::fs::write(&pending, b"manual residue").expect("write pending");

        let result = match operation {
            0 => install(&instance, "Archive.zip", "NewMod"),
            1 => remove(&instance, "CoolMod"),
            _ => replace(&instance, "CoolMod", "Archive.zip"),
        };
        let error = result.expect_err("pending must block");

        assert!(matches!(
            error,
            LifecycleError::PendingOperation { path } if path == pending
        ));
        assert_eq!(
            std::fs::read(&pending).expect("read pending"),
            b"manual residue"
        );
        assert_live_tree(&instance, "CoolMod");
    }
}

#[test]
fn missing_and_invalid_mod_names_fail_before_pending_creation() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");

    let missing = remove(&instance, "Missing").expect_err("missing mod");
    assert!(matches!(
        missing,
        LifecycleError::Instance(InstanceError::ModNotInstalled(name)) if name == "Missing"
    ));

    let invalid = remove(&instance, r"..\CoolMod").expect_err("invalid name");
    assert!(matches!(
        invalid,
        LifecycleError::Instance(InstanceError::InvalidModName(_))
    ));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn representative_crash_residue_blocks_without_inference() {
    let (_temp, instance) = instance();
    let pending = pending_path(&instance);
    let old_file = pending.join("old/nested/file.txt");
    std::fs::create_dir_all(old_file.parent().expect("old parent")).expect("create residue");
    std::fs::write(&old_file, "old bytes").expect("write old tree");

    let error = remove(&instance, "CoolMod").expect_err("crash residue must block");

    assert!(matches!(
        error,
        LifecycleError::PendingOperation { path } if path == pending
    ));
    assert_eq!(
        std::fs::read_to_string(old_file).expect("read old tree"),
        "old bytes"
    );
}

#[test]
fn held_instance_lock_maps_to_busy() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let _held = InstanceLock::acquire(&instance).expect("hold lock");

    let error = remove(&instance, "CoolMod").expect_err("busy");

    assert!(matches!(error, LifecycleError::Busy));
    assert_live_tree(&instance, "CoolMod");
}
