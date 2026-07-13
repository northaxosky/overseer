//! Lifecycle archive import tests

use super::support::*;
use super::*;

#[test]
fn nested_download_is_copied_to_direct_child() {
    let (_temp, instance) = instance();
    write_modlist(&instance, "Default", "");
    let nested = instance.downloads_dir().join("nested/Nested.zip");
    make_zip(&nested, &[("Textures/a.dds", b"x")]);
    let bytes = std::fs::read(&nested).expect("read nested");

    install(&instance, "Default", &nested, "Nested").expect("install nested");

    assert_eq!(
        std::fs::read(instance.downloads_dir().join("Nested.zip")).expect("read direct"),
        bytes
    );
    assert_eq!(std::fs::read(&nested).expect("read source"), bytes);
}

#[cfg(windows)]
#[test]
fn direct_download_detection_accepts_windows_path_casing_alias() {
    let (_temp, instance) = instance();
    write_modlist(&instance, "Default", "");
    let archive = download_zip(&instance, "Case.zip", &[("Textures/a.dds", b"x")]);
    let alias = instance
        .downloads_dir()
        .parent()
        .expect("instance root")
        .join("DOWNLOADS/Case.zip");

    install(&instance, "Default", &alias, "Case").expect("reuse direct alias");

    assert!(archive.is_file());
    assert!(instance.mods_dir().join("Case/Textures/a.dds").is_file());
}

#[test]
fn direct_download_must_be_a_regular_file() {
    let (_temp, instance) = instance();
    write_modlist(&instance, "Default", "");
    let archive = instance.downloads_dir().join("Directory.zip");
    std::fs::create_dir_all(&archive).expect("create archive directory");

    let error = install(&instance, "Default", &archive, "Directory").expect_err("non-file");

    assert!(matches!(
        error,
        LifecycleError::InvalidArchive { path } if path == archive
    ));
    assert!(!pending_path(&instance).exists());
}

#[test]
fn external_copy_collision_never_truncates_existing_download() {
    let (temp, instance) = instance();
    write_modlist(&instance, "Default", "");
    let source = camino::Utf8Path::from_path(temp.path())
        .expect("UTF-8 temp")
        .join("Collision.zip");
    make_zip(&source, &[("Textures/a.dds", b"source")]);
    let destination = instance.downloads_dir().join("Collision.zip");
    std::fs::create_dir_all(instance.downloads_dir()).expect("create downloads");
    std::fs::write(&destination, b"existing bytes").expect("write collision");

    let error = install(&instance, "Default", &source, "Collision").expect_err("collision");

    assert!(matches!(
        error,
        LifecycleError::DownloadCollision { path } if path == destination
    ));
    assert_eq!(
        std::fs::read(&destination).expect("read existing"),
        b"existing bytes"
    );
    assert!(!pending_path(&instance).exists());
}
