//! Tests for reading plugin metadata

use super::*;
use crate::test_support::{FLAG_LIGHT, FLAG_MASTER, temp, write_plugin, write_plugin_versioned};

// --- is_plugin_file ---

#[test]
fn recognizes_plugin_extensions_case_insensitively() {
    assert!(is_plugin_file("Mod.esp"));
    assert!(is_plugin_file("Mod.esm"));
    assert!(is_plugin_file("Mod.esl"));
    assert!(is_plugin_file("MOD.ESP"));
    assert!(!is_plugin_file("Texture.ba2"));
    assert!(!is_plugin_file("readme.txt"));
    assert!(!is_plugin_file("noext"));
}

// --- read_metadata ---

#[test]
fn plain_esp_is_neither_master_nor_light() {
    let (_t, base) = temp();
    let path = write_plugin(&base, "Plain.esp", 0, &[]);
    let meta = read_metadata(GameId::Fallout4, "Plain.esp", &path).expect("parse");
    assert_eq!(meta.name, "Plain.esp");
    assert!(!meta.is_master);
    assert!(!meta.is_light);
    assert!(meta.masters.is_empty());
}

#[test]
fn master_flag_marks_master() {
    let (_t, base) = temp();
    let path = write_plugin(&base, "Core.esp", FLAG_MASTER, &[]);
    let meta = read_metadata(GameId::Fallout4, "Core.esp", &path).expect("parse");
    assert!(meta.is_master);
    assert!(!meta.is_light);
}

#[test]
fn light_flag_marks_light_but_not_master() {
    // The 0x200 light flag does not imply master
    let (_t, base) = temp();
    let path = write_plugin(&base, "Patch.esp", FLAG_LIGHT, &[]);
    let meta = read_metadata(GameId::Fallout4, "Patch.esp", &path).expect("parse");
    assert!(meta.is_light);
    assert!(!meta.is_master);
}

#[test]
fn esm_extension_implies_master() {
    // No flags set; the .esm extension alone marks it a master
    let (_t, base) = temp();
    let path = write_plugin(&base, "Big.esm", 0, &[]);
    let meta = read_metadata(GameId::Fallout4, "Big.esm", &path).expect("parse");
    assert!(meta.is_master);
    assert!(!meta.is_light);
}

#[test]
fn esl_extension_implies_master_and_light() {
    // The .esl extension implies both the master and light flags
    let (_t, base) = temp();
    let path = write_plugin(&base, "Small.esl", 0, &[]);
    let meta = read_metadata(GameId::Fallout4, "Small.esl", &path).expect("parse");
    assert!(meta.is_master);
    assert!(meta.is_light);
}

#[test]
fn reads_the_master_list_in_order() {
    let (_t, base) = temp();
    let path = write_plugin(&base, "Dependent.esp", 0, &["Fallout4.esm", "DLCCoast.esm"]);
    let meta = read_metadata(GameId::Fallout4, "Dependent.esp", &path).expect("parse");
    assert_eq!(meta.masters, ["Fallout4.esm", "DLCCoast.esm"]);
}

#[test]
fn reads_the_header_version_from_the_hedr_field() {
    // The default builder stamps 1.0; a versioned fixture round-trips its own value
    let (_t, base) = temp();
    let default_path = write_plugin(&base, "Default.esp", 0, &[]);
    let default_meta = read_metadata(GameId::Fallout4, "Default.esp", &default_path).unwrap();
    assert_eq!(default_meta.header_version, Some(1.0));

    let old_path = write_plugin_versioned(&base, "Old.esp", 0, &[], 0.94);
    let old_meta = read_metadata(GameId::Fallout4, "Old.esp", &old_path).unwrap();
    assert_eq!(old_meta.header_version, Some(0.94));
}

#[test]
fn corrupt_file_is_a_parse_error() {
    let (_t, base) = temp();
    let path = base.join("Garbage.esp");
    std::fs::write(&path, b"not a plugin").expect("write");
    let err = read_metadata(GameId::Fallout4, "Garbage.esp", &path).expect_err("should fail");
    assert!(matches!(err, PluginError::Parse { .. }));
}
