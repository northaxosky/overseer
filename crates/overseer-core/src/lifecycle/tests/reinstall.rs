//! Lifecycle reinstall and provenance tests

use serde_json::json;

use super::support::*;
use super::*;
use crate::lifecycle::replace::reinstall_with;

#[test]
fn reinstall_uses_provenance_and_preserves_profiles() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    write_metadata(
        &instance,
        "CoolMod",
        "format = 1\narchive = \"Original.zip\"\n",
    );
    let archive = download_zip(
        &instance,
        "Original.zip",
        &[("nested/file.txt", b"reinstalled")],
    );
    let modlist = write_modlist(&instance, "Default", "# exact\r\n+COOLMOD");
    let plugins = instance.profile_dir("Default").join("plugins.txt");
    std::fs::write(&plugins, b"*Cool.esp\r\n").expect("write plugins");
    let before = [
        std::fs::read(&modlist).expect("read modlist"),
        std::fs::read(&plugins).expect("read plugins"),
    ];

    let report = reinstall(&instance, "coolmod").expect("reinstall");

    assert_eq!(report.name, "CoolMod");
    assert_eq!(report.archive.as_deref(), Some("Original.zip"));
    assert_eq!(
        std::fs::read_to_string(instance.mods_dir().join("CoolMod/file.txt"))
            .expect("read reinstalled"),
        "reinstalled"
    );
    assert_eq!(
        [
            std::fs::read(&modlist).expect("read modlist"),
            std::fs::read(&plugins).expect("read plugins"),
        ],
        before
    );
    assert!(archive.is_file());
    assert!(!pending_path(&instance).exists());
}

#[test]
fn reinstall_missing_provenance_recommends_replace() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");

    let error = reinstall(&instance, "CoolMod").expect_err("missing provenance");

    assert!(matches!(error, LifecycleError::MissingProvenance { .. }));
    assert!(error.to_string().contains("replace"));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn reinstall_rejects_malformed_and_missing_fields() {
    for text in ["not valid = [", "format = 1\n"] {
        let error = invalid_metadata_error(text);
        assert!(matches!(error, LifecycleError::InvalidProvenance { .. }));
    }
}

#[test]
fn reinstall_rejects_unknown_provenance_fields() {
    let error = invalid_metadata_error("format = 1\narchive = \"Original.zip\"\nextra = \"no\"\n");

    assert!(matches!(error, LifecycleError::InvalidProvenance { .. }));
}

#[test]
fn reinstall_rejects_unsafe_archive_basename() {
    for text in [
        "format = 1\narchive = \"../Escape.zip\"\n",
        "format = 1\narchive = \"Unsupported.rar\"\n",
        "format = 1\narchive = \"C:Drive.zip\"\n",
    ] {
        let error = invalid_metadata_error(text);
        assert!(matches!(error, LifecycleError::InvalidProvenance { .. }));
    }
}

#[test]
fn reinstall_requires_format_one() {
    let error = invalid_metadata_error("format = 2\narchive = \"Original.zip\"\n");

    assert!(matches!(error, LifecycleError::InvalidProvenance { .. }));
}

#[test]
fn reinstall_reports_missing_provenance_archive() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    write_metadata(
        &instance,
        "CoolMod",
        "format = 1\narchive = \"Missing.zip\"\n",
    );

    let error = reinstall(&instance, "CoolMod").expect_err("missing archive");

    assert!(matches!(
        &error,
        LifecycleError::MissingArchive { path, .. }
            if path == &instance.downloads_dir().join("Missing.zip")
    ));
    assert!(error.to_string().contains("replace"));
    assert_live_tree(&instance, "CoolMod");
    assert!(!pending_path(&instance).exists());
}

#[test]
fn reinstall_cleanup_warning_preserves_manifest_operation() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    write_metadata(
        &instance,
        "CoolMod",
        "format = 1\narchive = \"Original.zip\"\n",
    );
    download_zip(
        &instance,
        "Original.zip",
        &[("nested/file.txt", b"reinstalled")],
    );
    let pending = pending_path(&instance);

    let report = reinstall_with(&instance, "CoolMod", |path| Err(operation_failure(path)))
        .expect("logical success");

    assert_eq!(report.residue_warning, Some(pending.clone()));
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(pending.join("manifest.json")).expect("manifest"))
            .expect("parse manifest");
    assert_eq!(
        manifest,
        json!({
            "operation": "reinstall",
            "mod_name": "CoolMod",
            "archive": "Original.zip",
            "profiles": []
        })
    );
}

/// Write exact installed-root provenance text
fn write_metadata(instance: &crate::instance::Instance, name: &str, text: &str) {
    std::fs::write(
        instance
            .mods_dir()
            .join(name)
            .join(crate::lifecycle::archive::PROVENANCE),
        text,
    )
    .expect("write provenance");
}

/// Return the reinstall error for one invalid provenance document
fn invalid_metadata_error(text: &str) -> LifecycleError {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    write_metadata(&instance, "CoolMod", text);
    reinstall(&instance, "CoolMod").expect_err("invalid provenance")
}
