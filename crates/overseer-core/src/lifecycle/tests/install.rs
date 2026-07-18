//! Lifecycle install tests

use std::io::Write;

use super::support::*;
use super::*;
use crate::install::InstallError;
use crate::instance::{InstanceError, Profile};

#[test]
fn install_publishes_direct_download_and_leaves_profiles_unchanged() {
    let (_temp, instance) = instance();
    install_tree(&instance, "Existing");
    let original = "+Existing\r\n";
    write_modlist(&instance, "Default", original);
    download_zip(
        &instance,
        "Cool.zip",
        &[("Textures/a.dds", b"new"), ("Cool.esp", b"plugin")],
    );

    let report = install(&instance, "Cool.zip", "CoolMod").expect("install");

    assert_eq!(report.name, "CoolMod");
    assert_eq!(report.residue_warning, None);
    assert_eq!(read_modlist(&instance, "Default"), original);
    assert_eq!(
        std::fs::read_to_string(instance.mods_dir().join("CoolMod/Textures/a.dds"))
            .expect("read installed"),
        "new"
    );
    assert!(
        !instance
            .mods_dir()
            .join("CoolMod/.overseer-mod.toml")
            .exists()
    );
    assert!(!pending_path(&instance).exists());
}

#[test]
fn profile_reconcile_discovers_an_install_disabled() {
    let (_temp, instance) = instance();
    install_tree(&instance, "Existing");
    write_modlist(&instance, "Default", "+Existing\r\n");
    download_zip(&instance, "Cool.zip", &[("Textures/a.dds", b"new")]);
    install(&instance, "Cool.zip", "CoolMod").expect("install");

    let mut profile = Profile::load_existing(&instance, "Default").expect("load profile");
    assert!(profile.reconcile(&instance).expect("reconcile"));
    let entry = profile
        .items()
        .find(|entry| entry.name == "CoolMod")
        .expect("new mod row");

    assert!(!entry.enabled);
    profile.save_modlist(&instance).expect("save modlist");
    assert_eq!(read_modlist(&instance, "Default"), "+Existing\n-CoolMod\n");
}

#[test]
fn install_duplicate_check_precedes_archive_resolution() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");

    let error = install(&instance, "Missing.zip", "coolmod").expect_err("duplicate");

    assert!(matches!(
        error,
        LifecycleError::Instance(InstanceError::ModAlreadyInstalled(name)) if name == "coolmod"
    ));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn candidate_validation_cleans_pending_work() {
    for (archive_name, entries, expected) in [
        (
            "Fomod.zip",
            vec![
                ("fomod/ModuleConfig.xml", b"<config/>".as_slice()),
                ("Textures/a.dds", b"x".as_slice()),
            ],
            "fomod",
        ),
        ("Empty.zip", Vec::new(), "empty"),
        (
            "Reserved.zip",
            vec![
                (".OVERSEER-MOD.TOML", b"reserved".as_slice()),
                ("Textures/a.dds", b"x".as_slice()),
            ],
            "reserved",
        ),
    ] {
        let (_temp, instance) = instance();
        download_zip(&instance, archive_name, &entries);

        let error = install(&instance, archive_name, "Rejected").expect_err("candidate refusal");

        assert!(
            matches!(
                (&error, expected),
                (LifecycleError::Install(InstallError::Fomod), "fomod")
                    | (LifecycleError::Install(InstallError::EmptyArchive), "empty")
                    | (
                        LifecycleError::Install(InstallError::ReservedMetadata),
                        "reserved"
                    )
            ),
            "got {error:?}"
        );
        assert!(!instance.mods_dir().join("Rejected").exists());
        assert!(!pending_path(&instance).exists());
    }
}

#[test]
fn install_rename_failure_cleans_candidate_and_does_not_publish() {
    let (_temp, instance) = instance();
    download_zip(&instance, "Cool.zip", &[("Textures/a.dds", b"new")]);
    let candidate = pending_path(&instance).join("new");
    let _fail = failpoint::scoped([(failpoint::Point::Rename, candidate)]);

    let error = install(&instance, "Cool.zip", "CoolMod").expect_err("publication failure");

    assert!(matches!(error, LifecycleError::Io(_)));
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert!(!pending_path(&instance).exists());
}

#[test]
fn install_cleanup_failure_reports_committed_residue() {
    let (_temp, instance) = instance();
    download_zip(&instance, "Cool.zip", &[("Textures/a.dds", b"new")]);
    let pending = pending_path(&instance);
    let _fail = failpoint::scoped([(failpoint::Point::Cleanup, pending.clone())]);

    let report = install(&instance, "Cool.zip", "CoolMod").expect("committed install");

    assert_eq!(report.residue_warning, Some(pending.clone()));
    assert!(instance.mods_dir().join("CoolMod/Textures/a.dds").is_file());
    assert!(pending.is_dir());
}

#[test]
fn lifecycle_candidate_preserves_root_detection() {
    let (_temp, instance) = instance();
    download_zip(&instance, "Rooted.zip", &[("Wrapper/Root/x.dll", b"root")]);

    install(&instance, "Rooted.zip", "Rooted").expect("install rooted");

    assert!(instance.mods_dir().join("Rooted/Root/x.dll").is_file());
    assert!(!instance.mods_dir().join("Rooted/Wrapper").exists());
}

#[test]
fn lifecycle_candidate_preserves_bomb_guard() {
    let (_temp, instance) = instance();
    let archive = instance.downloads_dir().join("Bomb.zip");
    std::fs::create_dir_all(instance.downloads_dir()).expect("create downloads");
    let file = std::fs::File::create(&archive).expect("create bomb zip");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("Textures/large.dds", options)
        .expect("start bomb entry");
    let chunk = vec![0; 1024 * 1024];
    for _ in 0..101 {
        zip.write_all(&chunk).expect("write bomb entry");
    }
    zip.finish().expect("finish bomb zip");

    let error = install(&instance, "Bomb.zip", "Bomb").expect_err("bomb refusal");

    assert!(matches!(
        error,
        LifecycleError::Install(InstallError::TooLarge { .. })
    ));
    assert!(!instance.mods_dir().join("Bomb").exists());
    assert!(!pending_path(&instance).exists());
}
