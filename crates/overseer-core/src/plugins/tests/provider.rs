//! Tests for lazy plugin provider resolution

use super::*;
use crate::instance::{ModKind, ModListEntry, Profile};
use crate::test_support::{install_mod, temp_instance};

fn entry(name: &str) -> ModListEntry {
    ModListEntry {
        name: name.to_owned(),
        enabled: true,
        kind: ModKind::Managed,
    }
}

fn profile(mods: &[&str]) -> Profile {
    Profile {
        name: "Default".to_owned(),
        mods: mods.iter().map(|name| entry(name)).collect(),
        local_saves: false,
    }
}

#[test]
fn returns_the_only_managed_provider() {
    let (_temp, instance) = temp_instance();
    install_mod(&instance, "Only", &[("Only.esp", "plugin")]);

    assert_eq!(
        plugin_provider(&instance, &profile(&["Only"]), "Only.esp").expect("resolve provider"),
        Some(ProviderOrigin::Mod {
            name: "Only".to_owned()
        })
    );
}

#[test]
fn returns_the_higher_priority_managed_provider() {
    let (_temp, instance) = temp_instance();
    install_mod(&instance, "High", &[("Shared.esp", "high")]);
    install_mod(&instance, "Low", &[("Shared.esp", "low")]);

    assert_eq!(
        plugin_provider(&instance, &profile(&["High", "Low"]), "Shared.esp")
            .expect("resolve provider"),
        Some(ProviderOrigin::Mod {
            name: "High".to_owned()
        })
    );
}

#[test]
fn overwrite_outranks_managed_mods() {
    let (_temp, instance) = temp_instance();
    install_mod(&instance, "Managed", &[("Shared.esp", "managed")]);
    std::fs::create_dir_all(instance.overwrite_dir()).expect("create overwrite");
    std::fs::write(instance.overwrite_dir().join("Shared.esp"), b"overwrite")
        .expect("write overwrite plugin");

    assert_eq!(
        plugin_provider(&instance, &profile(&["Managed"]), "Shared.esp").expect("resolve provider"),
        Some(ProviderOrigin::Overwrite)
    );
}

#[test]
fn returns_none_when_no_source_provides_the_plugin() {
    let (_temp, instance) = temp_instance();
    install_mod(&instance, "Managed", &[("Other.esp", "plugin")]);

    assert_eq!(
        plugin_provider(&instance, &profile(&["Managed"]), "Missing.esp")
            .expect("resolve provider"),
        None
    );
}

#[test]
fn matches_filenames_case_insensitively() {
    let (_temp, instance) = temp_instance();
    install_mod(&instance, "Managed", &[("X.ESP", "plugin")]);

    assert_eq!(
        plugin_provider(&instance, &profile(&["Managed"]), "x.esp").expect("resolve provider"),
        Some(ProviderOrigin::Mod {
            name: "Managed".to_owned()
        })
    );
}

#[test]
fn missing_mod_and_overwrite_directories_are_skipped() {
    let (_temp, instance) = temp_instance();

    assert_eq!(
        plugin_provider(&instance, &profile(&["Missing"]), "Missing.esp")
            .expect("resolve provider"),
        None
    );
}
