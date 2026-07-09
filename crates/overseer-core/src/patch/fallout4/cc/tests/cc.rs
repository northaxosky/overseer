//! Tests for Creation Club load-order selection

use super::*;
use crate::plugins::{PluginEntry, PluginLoadOrder};

fn make_order(entries: &[(&str, bool)]) -> PluginLoadOrder {
    PluginLoadOrder {
        profile: "Default".to_owned(),
        plugins: entries
            .iter()
            .map(|(name, active)| PluginEntry {
                name: (*name).to_owned(),
                active: *active,
            })
            .collect(),
    }
}

#[test]
fn selects_catalog_members_keeping_order_and_inactive() {
    let order = make_order(&[
        ("Fallout4.esm", true),
        ("ccBGSFO4001-PipBoy(Black).esl", true),
        ("MyMod.esp", true),
        ("ccRZRFO4001-TunnelSnakes.esm", false),
    ]);
    assert_eq!(
        cc_plugins(&order),
        vec![
            "ccBGSFO4001-PipBoy(Black).esl".to_owned(),
            "ccRZRFO4001-TunnelSnakes.esm".to_owned(),
        ]
    );
}

#[test]
fn returns_empty_when_no_cc_present() {
    let order = make_order(&[("Fallout4.esm", true), ("MyMod.esp", true)]);
    assert!(cc_plugins(&order).is_empty());
}
