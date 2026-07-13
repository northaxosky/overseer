//! Lifecycle archive basename and custody tests

use super::support::*;
use super::*;
use crate::install::InstallError;

#[test]
fn install_rejects_path_like_archive_names_without_copying() {
    let (temp, instance) = instance();
    let external = camino::Utf8Path::from_path(temp.path())
        .expect("UTF-8 temp")
        .join("External.zip");
    make_zip(&external, &[("Textures/a.dds", b"x")]);

    for name in [
        external.as_str(),
        "nested/Nested.zip",
        r"nested\Nested.zip",
        "../Escape.zip",
        "C:Drive.zip",
        "",
    ] {
        let error = install(&instance, name, "Unsafe").expect_err("unsafe basename");
        assert!(
            matches!(error, LifecycleError::InvalidArchiveName { .. }),
            "got {error:?} for {name:?}"
        );
    }

    assert!(!instance.downloads_dir().join("External.zip").exists());
    assert!(!instance.mods_dir().join("Unsafe").exists());
    assert!(!pending_path(&instance).exists());
}

#[test]
fn install_rejects_unsupported_archive_format() {
    let (_temp, instance) = instance();

    let error = install(&instance, "Unsupported.rar", "Unsupported").expect_err("unsupported");

    assert!(matches!(
        error,
        LifecycleError::Install(InstallError::UnsupportedFormat { extension })
            if extension == "rar"
    ));
    assert!(!pending_path(&instance).exists());
}

#[test]
fn install_requires_an_existing_direct_regular_file() {
    let (_temp, instance) = instance();
    let missing = instance.downloads_dir().join("Missing.zip");

    let error = install(&instance, "Missing.zip", "Missing").expect_err("missing archive");

    assert!(matches!(
        error,
        LifecycleError::ArchiveUnavailable { path } if path == missing
    ));

    let directory = instance.downloads_dir().join("Directory.zip");
    std::fs::create_dir_all(&directory).expect("create archive directory");
    let error = install(&instance, "Directory.zip", "Directory").expect_err("archive directory");
    assert!(matches!(
        error,
        LifecycleError::ArchiveUnavailable { path } if path == directory
    ));
    assert!(!pending_path(&instance).exists());
}

#[test]
fn install_rejects_a_direct_download_symlink() {
    let (_temp, instance) = instance();
    let target = download_zip(&instance, "Target.zip", &[("Textures/a.dds", b"x")]);
    let link = instance.downloads_dir().join("Linked.zip");
    if let Err(error) = create_file_symlink(&target, &link) {
        if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied {
            return;
        }
        panic!("create archive symlink: {error}");
    }

    let error = install(&instance, "Linked.zip", "Linked").expect_err("archive symlink");

    assert!(matches!(
        error,
        LifecycleError::ArchiveUnavailable { path } if path == link
    ));
    assert!(!instance.mods_dir().join("Linked").exists());
    assert!(!pending_path(&instance).exists());
}

#[test]
fn replace_applies_the_same_basename_boundary() {
    let (temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let external = camino::Utf8Path::from_path(temp.path())
        .expect("UTF-8 temp")
        .join("Replacement.zip");
    make_zip(&external, &[("nested/file.txt", b"replacement")]);

    let error = replace(&instance, "CoolMod", external.as_str()).expect_err("external replacement");

    assert!(matches!(error, LifecycleError::InvalidArchiveName { .. }));
    assert_live_tree(&instance, "CoolMod");
    assert!(!instance.downloads_dir().join("Replacement.zip").exists());
    assert!(!pending_path(&instance).exists());
}

#[cfg(unix)]
fn create_file_symlink(target: &camino::Utf8Path, link: &camino::Utf8Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &camino::Utf8Path, link: &camino::Utf8Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
