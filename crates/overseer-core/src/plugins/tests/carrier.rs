//! Validating the carrier ESL bytes by parsing them back with `esplugin`

use super::*;
use crate::test_support::temp;
use esplugin::{GameId, ParseOptions, Plugin};

/// Write carrier bytes under `name` and parse them as a Fallout 4 plugin
fn parse_carrier(name: &str) -> Plugin {
    let (_t, base) = temp();
    let path = base.join(name);
    std::fs::write(&path, carrier_esl()).expect("write carrier");
    let mut plugin = Plugin::new(GameId::Fallout4, path.as_std_path());
    plugin
        .parse_file(ParseOptions::header_only())
        .expect("esplugin parses the carrier bytes");
    plugin
}

#[test]
fn esplugin_accepts_the_carrier_as_a_light_master_with_no_masters() {
    let plugin = parse_carrier("CCMerged_Main.esl");
    assert!(plugin.is_light_plugin(), "carrier must be a light plugin");
    assert!(plugin.is_master_file(), "carrier must be a master");
    assert!(
        plugin.masters().expect("read masters").is_empty(),
        "carrier declares no masters"
    );
}

#[test]
fn carrier_is_record_free_with_a_v1_header() {
    let plugin = parse_carrier("CCMerged_Main.esl");
    assert_eq!(plugin.record_and_group_count(), Some(0));
    assert_eq!(plugin.header_version(), Some(1.0));
}

#[test]
fn flags_mark_light_and_master_without_the_esl_extension() {
    // A .esp name strips the extension shortcut, so light+master can only come from the header flags
    let plugin = parse_carrier("CCMerged_Main.esp");
    assert!(plugin.is_light_plugin(), "0x200 light flag must be set");
    assert!(plugin.is_master_file(), "0x1 master flag must be set");
}
