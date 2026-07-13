//! Successful remove behavior and bundle manifest tests

use serde_json::json;

use super::support::*;
use super::*;

#[test]
fn remove_deletes_tree_and_only_matching_managed_rows() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    write_modlist(
        &instance,
        "Zulu",
        "-Other\n+coolmod\n*COOLMOD\n-CoolMod\n-Group_separator\n",
    );
    write_modlist(&instance, "Alpha", "+Keep\r\n+CoolMod\r\n-Off\r\n");

    let report = remove(&instance, "cOoLmOd").expect("remove mod");

    assert_eq!(report.name, "CoolMod");
    assert_eq!(report.archive, None);
    assert_eq!(report.residue_warning, None);
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert_eq!(read_modlist(&instance, "Alpha"), "+Keep\n-Off\n");
    assert_eq!(
        read_modlist(&instance, "Zulu"),
        "-Other\n*COOLMOD\n-Group_separator\n"
    );
    assert!(!pending_path(&instance).exists());
}

#[test]
fn remove_unreferenced_mod_does_not_rewrite_profiles() {
    let (_temp, instance) = instance();
    install_tree(&instance, "Unused");
    let original = "# preserved\r\n+Other";
    write_modlist(&instance, "Default", original);

    let report = remove(&instance, "Unused").expect("remove unreferenced mod");

    assert_eq!(report.name, "Unused");
    assert!(!instance.mods_dir().join("Unused").exists());
    assert_eq!(read_modlist(&instance, "Default"), original);
}

#[test]
fn cleanup_failure_returns_success_and_preserves_exact_manifest() {
    let (_temp, instance) = instance();
    install_tree(&instance, "CoolMod");
    let alpha = "+CoolMod\r\n+Keep\r\n";
    let zulu = "-Other\n-CoolMod\n";
    write_modlist(&instance, "Zulu", zulu);
    write_modlist(&instance, "Alpha", alpha);
    let pending = pending_path(&instance);
    let _fail = failpoint::scoped([(failpoint::Point::Cleanup, pending.clone())]);

    let report = remove(&instance, "coolmod").expect("logical success");

    assert_eq!(report.residue_warning, Some(pending.clone()));
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert_live_tree_in_bundle(&pending);
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(pending.join("manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(
        manifest,
        json!({
            "operation": "remove",
            "mod_name": "CoolMod",
            "profiles": [
                {"profile": "Alpha", "original_modlist": alpha},
                {"profile": "Zulu", "original_modlist": zulu}
            ]
        })
    );

    let absent = bundle::Manifest {
        operation: bundle::Operation::Remove,
        mod_name: "Absent".to_owned(),
        archive: None,
        profiles: vec![bundle::ManifestProfile {
            profile: "Empty".to_owned(),
            original_modlist: None,
        }],
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bundle::serialize(&pending, &absent).expect("serialize"))
            .expect("parse");
    assert_eq!(
        value["profiles"][0]["original_modlist"],
        serde_json::Value::Null
    );
}

#[test]
fn archive_operations_serialize_operation_and_archive_fields() {
    let (_temp, instance) = instance();
    let pending = pending_path(&instance);
    for (operation, expected) in [
        (bundle::Operation::Install, "install"),
        (bundle::Operation::Replace, "replace"),
        (bundle::Operation::Reinstall, "reinstall"),
    ] {
        let manifest = bundle::Manifest {
            operation,
            mod_name: "CoolMod".to_owned(),
            archive: Some("Cool.zip".to_owned()),
            profiles: Vec::new(),
        };
        let value: serde_json::Value =
            serde_json::from_slice(&bundle::serialize(&pending, &manifest).expect("serialize"))
                .expect("parse");

        assert_eq!(value["operation"], expected);
        assert_eq!(value["archive"], "Cool.zip");
    }
}

/// Assert that cleanup residue still owns the removed tree
fn assert_live_tree_in_bundle(pending: &camino::Utf8Path) {
    let path = pending.join("old").join("nested").join("file.txt");
    assert_eq!(
        std::fs::read_to_string(path).expect("read old tree"),
        "mod bytes"
    );
}
