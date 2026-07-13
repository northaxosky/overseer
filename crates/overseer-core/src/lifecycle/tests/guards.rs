//! Entry guards and preflight failure tests

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
fn pending_file_has_precedence_and_remains_untouched() {
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
fn any_deployment_path_occupant_blocks_remove() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let deployment = instance.state_dir().join("deployment.json");
    std::fs::create_dir_all(&deployment).expect("create deployment directory");

    let error = remove(&instance, "CoolMod").expect_err("deployment must block");

    assert!(matches!(
        error,
        LifecycleError::DeploymentExists { path } if path == deployment
    ));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn missing_mod_fails_before_bundle_creation() {
    let (_temp, instance) = instance();
    let original = "+Missing\r\n";
    write_modlist(&instance, "Default", original);

    let error = remove(&instance, "Missing").expect_err("missing mod");

    assert!(matches!(
        error,
        LifecycleError::Instance(InstanceError::ModNotInstalled(name)) if name == "Missing"
    ));
    assert_eq!(read_modlist(&instance, "Default"), original);
    assert!(!pending_path(&instance).exists());
}

#[test]
fn invalid_name_fails_before_bundle_creation() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");

    let error = remove(&instance, "..\\CoolMod").expect_err("invalid name");

    assert!(matches!(
        error,
        LifecycleError::Instance(InstanceError::InvalidModName(_))
    ));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn profile_load_error_fails_before_bundle_creation() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    write_modlist(&instance, "Broken", "+CoolMod\n");
    std::fs::write(instance.profile_dir("Broken").join("settings.ini"), [0xff])
        .expect("write invalid UTF-8");

    let error = remove(&instance, "CoolMod").expect_err("profile error");

    assert!(matches!(
        error,
        LifecycleError::Instance(InstanceError::Io(_))
    ));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn representative_crash_residue_blocks_without_changes() {
    let (_temp, instance) = instance();
    let original = "+CoolMod\r\n";
    write_modlist(&instance, "Default", original);
    let pending = pending_path(&instance);
    let old_file = pending.join("old").join("nested").join("file.txt");
    std::fs::create_dir_all(old_file.parent().expect("old parent")).expect("create residue");
    std::fs::write(pending.join("manifest.json"), "manual manifest").expect("write manifest");
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
    assert_eq!(
        std::fs::read_to_string(pending.join("manifest.json")).expect("read manifest"),
        "manual manifest"
    );
    assert_eq!(read_modlist(&instance, "Default"), original);
}

#[test]
fn held_apply_lock_maps_to_busy() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let _held = InstanceLock::acquire(&instance).expect("hold lock");

    let error = remove(&instance, "CoolMod").expect_err("busy");

    assert!(matches!(error, LifecycleError::Busy));
    assert_live_tree(&instance, "CoolMod");
}

#[test]
fn pending_bundle_blocks_install_replace_and_reinstall() {
    for operation in 0..3 {
        let (_temp, instance) = instance();
        install_tree(&instance, "CoolMod");
        let pending = pending_path(&instance);
        std::fs::create_dir_all(instance.state_dir()).expect("create state");
        std::fs::write(&pending, b"crash residue").expect("write pending");

        let result = match operation {
            0 => install(
                &instance,
                "MissingProfile",
                Utf8Path::new("missing.zip"),
                "NewMod",
            ),
            1 => replace(&instance, "CoolMod", Utf8Path::new("missing.zip")),
            _ => reinstall(&instance, "CoolMod"),
        };
        let error = result.expect_err("pending guard");

        assert!(matches!(
            error,
            LifecycleError::PendingOperation { path } if path == pending
        ));
        assert_eq!(
            std::fs::read(&pending).expect("read pending"),
            b"crash residue"
        );
        assert_live_tree(&instance, "CoolMod");
    }
}

#[test]
fn deployment_record_blocks_install_replace_and_reinstall() {
    for operation in 0..3 {
        let (_temp, instance) = instance();
        install_tree(&instance, "CoolMod");
        let deployment = instance.state_dir().join("deployment.json");
        std::fs::create_dir_all(instance.state_dir()).expect("create state");
        std::fs::write(&deployment, b"deployed").expect("write deployment");

        let result = match operation {
            0 => install(
                &instance,
                "MissingProfile",
                Utf8Path::new("missing.zip"),
                "NewMod",
            ),
            1 => replace(&instance, "CoolMod", Utf8Path::new("missing.zip")),
            _ => reinstall(&instance, "CoolMod"),
        };
        let error = result.expect_err("deployment guard");

        assert!(matches!(
            error,
            LifecycleError::DeploymentExists { path } if path == deployment
        ));
        assert_eq!(
            std::fs::read(&deployment).expect("read deployment"),
            b"deployed"
        );
        assert_live_tree(&instance, "CoolMod");
        assert!(!pending_path(&instance).exists());
    }
}
