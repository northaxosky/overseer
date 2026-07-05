//! Tests for the missing-masters check

use super::*;
use crate::finding::Severity;

fn meta(name: &str, masters: &[&str]) -> PluginMeta {
    overseer_core::test_support::plugin_meta(name, false, false, masters)
}

fn ctx(active: Vec<PluginMeta>, present: &[&str]) -> GameContext {
    GameContext {
        active_plugins: active,
        loaded_plugins: present.iter().map(|p| meta(p, &[])).collect(),
        ..GameContext::default()
    }
}

#[test]
fn present_masters_are_ok() {
    let c = ctx(
        vec![meta("Patch.esp", &["Fallout4.esm", "ArmorMod.esp"])],
        &["Fallout4.esm", "ArmorMod.esp", "Patch.esp"],
    );
    let findings = super::run(&c);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
}

#[test]
fn a_missing_master_is_an_error() {
    let c = ctx(
        vec![meta("Patch.esp", &["Fallout4.esm", "Gone.esm"])],
        &["Fallout4.esm", "Patch.esp"],
    );
    let findings = super::run(&c);
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(findings[0].title.contains("Patch.esp"));
    assert!(findings[0].title.contains("Gone.esm"));
    // Plugins are activated/deactivated, not enabled/disabled (glossary)
    let detail = findings[0].detail.as_deref().expect("detail");
    assert!(detail.contains("deactivate") && !detail.contains("disable"));
}

#[test]
fn master_matching_is_case_insensitive() {
    let c = ctx(
        vec![meta("Patch.esp", &["FALLOUT4.ESM"])],
        &["fallout4.esm", "patch.esp"],
    );
    assert_eq!(super::run(&c)[0].severity, Severity::Info);
}
