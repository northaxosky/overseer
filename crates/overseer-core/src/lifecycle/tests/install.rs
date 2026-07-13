//! Lifecycle install tests

use std::io::Write;

use super::support::*;
use super::*;
use crate::install::InstallError;
use crate::instance::InstanceError;
use crate::lifecycle::install::install_with;

#[test]
fn install_reuses_direct_download_and_adds_disabled_first_row() {
    let (_temp, instance) = instance();
    write_modlist(&instance, "Default", "+Existing\r\n");
    let archive = download_zip(
        &instance,
        "Cool.zip",
        &[("Textures/a.dds", b"new"), ("Cool.esp", b"plugin")],
    );
    let archive_bytes = std::fs::read(&archive).expect("read archive");

    let report = install(&instance, "Default", &archive, "CoolMod").expect("install");

    assert_eq!(report.name, "CoolMod");
    assert_eq!(report.archive.as_deref(), Some("Cool.zip"));
    assert_eq!(report.residue_warning, None);
    assert_eq!(read_modlist(&instance, "Default"), "-CoolMod\n+Existing\n");
    assert_eq!(std::fs::read(&archive).expect("read reused"), archive_bytes);
    assert_eq!(
        std::fs::read_to_string(instance.mods_dir().join("CoolMod/Textures/a.dds"))
            .expect("read installed"),
        "new"
    );
    assert_eq!(
        std::fs::read_to_string(
            instance
                .mods_dir()
                .join("CoolMod")
                .join(crate::lifecycle::archive::PROVENANCE)
        )
        .expect("read provenance"),
        "format = 1\narchive = \"Cool.zip\"\n"
    );
    let entries: Vec<_> = std::fs::read_dir(instance.mods_dir())
        .expect("read mods")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(entries, [std::ffi::OsString::from("CoolMod")]);
    assert!(!pending_path(&instance).exists());
}

#[test]
fn install_copies_external_archive_into_downloads() {
    let (temp, instance) = instance();
    write_modlist(&instance, "Default", "");
    let source = camino::Utf8Path::from_path(temp.path())
        .expect("UTF-8 temp")
        .join("External.zip");
    make_zip(&source, &[("Meshes/a.nif", b"mesh")]);
    let bytes = std::fs::read(&source).expect("read source");

    install(&instance, "Default", &source, "External").expect("install");

    assert_eq!(
        std::fs::read(instance.downloads_dir().join("External.zip")).expect("read copy"),
        bytes
    );
    assert!(instance.mods_dir().join("External/Meshes/a.nif").is_file());
}

#[test]
fn install_save_failure_restores_exact_profile_and_keeps_import() {
    let (temp, instance) = instance();
    let original = "# exact\r\n+Keep";
    let modlist = write_modlist(&instance, "Default", original);
    let source = camino::Utf8Path::from_path(temp.path())
        .expect("UTF-8 temp")
        .join("Imported.zip");
    make_zip(&source, &[("Textures/a.dds", b"new")]);

    let error = install_with(
        &instance,
        "Default",
        &source,
        "Imported",
        |profile, instance| {
            profile.save_modlist(instance)?;
            Err(operation_failure(&modlist).into())
        },
    )
    .expect_err("save failure");

    assert!(matches!(
        error,
        LifecycleError::Instance(InstanceError::Io(_))
    ));
    assert_eq!(read_modlist(&instance, "Default"), original);
    assert!(!instance.mods_dir().join("Imported").exists());
    assert!(instance.downloads_dir().join("Imported.zip").is_file());
    assert!(!pending_path(&instance).exists());
}

#[test]
fn install_save_failure_restores_absent_modlist() {
    let (_temp, instance) = instance();
    std::fs::create_dir_all(instance.profile_dir("Default")).expect("create profile");
    let archive = download_zip(&instance, "Absent.zip", &[("Textures/a.dds", b"new")]);
    let modlist = modlist_path(&instance, "Default");

    install_with(
        &instance,
        "Default",
        &archive,
        "Absent",
        |profile, instance| {
            profile.save_modlist(instance)?;
            Err(operation_failure(&modlist).into())
        },
    )
    .expect_err("save failure");

    assert!(!modlist.exists());
    assert!(!instance.mods_dir().join("Absent").exists());
    assert!(!pending_path(&instance).exists());
}

#[test]
fn lifecycle_candidate_preserves_fomod_and_empty_refusals() {
    for (name, entries, expected) in [
        (
            "Fomod",
            vec![
                ("fomod/ModuleConfig.xml", b"<config/>".as_slice()),
                ("Textures/a.dds", b"x".as_slice()),
            ],
            "fomod",
        ),
        ("Empty", Vec::new(), "empty"),
    ] {
        let (_temp, instance) = instance();
        write_modlist(&instance, "Default", "+Keep\r\n");
        let archive = download_zip(&instance, &format!("{name}.zip"), &entries);

        let error = install(&instance, "Default", &archive, name).expect_err("candidate refusal");

        assert!(
            matches!(
                (&error, expected),
                (LifecycleError::Install(InstallError::Fomod), "fomod")
                    | (LifecycleError::Install(InstallError::EmptyArchive), "empty")
            ),
            "got {error:?}"
        );
        assert_eq!(read_modlist(&instance, "Default"), "+Keep\r\n");
        assert!(!instance.mods_dir().join(name).exists());
        assert!(!pending_path(&instance).exists());
    }
}

#[test]
fn lifecycle_candidate_preserves_root_detection() {
    let (_temp, instance) = instance();
    write_modlist(&instance, "Default", "");
    let archive = download_zip(&instance, "Rooted.zip", &[("Wrapper/Root/x.dll", b"root")]);

    install(&instance, "Default", &archive, "Rooted").expect("install rooted");

    assert!(instance.mods_dir().join("Rooted/Root/x.dll").is_file());
    assert!(!instance.mods_dir().join("Rooted/Wrapper").exists());
}

#[test]
fn lifecycle_candidate_rejects_archive_provenance() {
    let (_temp, instance) = instance();
    let original = "+Keep\r\n";
    write_modlist(&instance, "Default", original);
    let archive = download_zip(
        &instance,
        "Spoofed.zip",
        &[
            (".OVERSEER-MOD.TOML", b"format = 1"),
            ("Textures/a.dds", b"x"),
        ],
    );

    let error = install(&instance, "Default", &archive, "Spoofed").expect_err("reserved file");

    assert!(matches!(
        error,
        LifecycleError::Install(InstallError::ReservedProvenance)
    ));
    assert_eq!(read_modlist(&instance, "Default"), original);
    assert!(!instance.mods_dir().join("Spoofed").exists());
}

#[test]
fn lifecycle_candidate_preserves_bomb_guard() {
    let (_temp, instance) = instance();
    write_modlist(&instance, "Default", "");
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

    let error = install(&instance, "Default", &archive, "Bomb").expect_err("bomb refusal");

    assert!(matches!(
        error,
        LifecycleError::Install(InstallError::TooLarge { .. })
    ));
    assert!(!instance.mods_dir().join("Bomb").exists());
    assert!(!pending_path(&instance).exists());
}

#[test]
fn install_preflight_collision_does_not_import_archive() {
    let (temp, instance) = instance();
    write_modlist(&instance, "Default", "*COLLIDE\r\n");
    let source = camino::Utf8Path::from_path(temp.path())
        .expect("UTF-8 temp")
        .join("Collide.zip");
    make_zip(&source, &[("Textures/a.dds", b"x")]);

    let error = install(&instance, "Default", &source, "Collide").expect_err("profile collision");

    assert!(matches!(
        error,
        LifecycleError::Instance(InstanceError::ModAlreadyInList(name)) if name == "Collide"
    ));
    assert!(!instance.downloads_dir().join("Collide.zip").exists());
    assert!(!pending_path(&instance).exists());
}
