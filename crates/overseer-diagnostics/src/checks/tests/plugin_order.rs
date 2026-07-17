//! Tests for the plugin-order diagnostics shim

use super::*;
use crate::finding::Severity;
use overseer_core::plugins::{PluginEntry, PluginLoadOrder};

#[test]
fn maps_core_violations_to_diagnostic_findings() {
    let ctx = GameContext {
        plugin_order: PluginLoadOrder {
            profile: "P".to_owned(),
            plugins: vec![
                PluginEntry {
                    name: "Patch.esp".to_owned(),
                    active: true,
                },
                PluginEntry {
                    name: "Armor.esp".to_owned(),
                    active: true,
                },
            ],
        },
        discovered_plugins: vec![
            overseer_core::test_support::plugin_meta("Patch.esp", false, false, &["Armor.esp"]),
            overseer_core::test_support::plugin_meta("Armor.esp", false, false, &[]),
        ],
        ..GameContext::default()
    };

    let findings = run(&ctx);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(findings[0].title.contains("Patch.esp"));
    assert!(findings[0].title.contains("Armor.esp"));
}

#[test]
fn valid_order_has_no_order_findings() {
    assert!(run(&GameContext::default()).is_empty());
}
