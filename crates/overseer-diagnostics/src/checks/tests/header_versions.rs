//! Tests for the plugin header-version check

use super::*;
use crate::finding::Severity;

fn plugin(name: &str, header_version: Option<f32>) -> PluginMeta {
    PluginMeta {
        header_version,
        ..overseer_core::test_support::plugin_meta(name, false, false, &[])
    }
}

fn run(plugins: Vec<PluginMeta>) -> Vec<Finding> {
    super::run(&GameContext {
        loaded_plugins: plugins,
        ..GameContext::default()
    })
}

#[test]
fn is_known_hedr_accepts_only_the_two_fallout4_versions() {
    assert!(is_known_hedr(0.95));
    assert!(is_known_hedr(1.0));
    assert!(!is_known_hedr(0.94));
    assert!(!is_known_hedr(1.2));
    // Exact bits, no tolerance: a value close to 0.95 is still rejected
    assert!(!is_known_hedr(0.951));
    assert!(!is_known_hedr(0.949));
}

#[test]
fn an_old_header_version_warns() {
    let findings = run(vec![plugin("Legacy.esp", Some(0.94))]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("Legacy.esp"));
    assert!(findings[0].title.contains("0.94"));
    assert!(
        findings[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("Creation Kit")
    );
}

#[test]
fn the_accepted_versions_report_a_clean_info() {
    let findings = run(vec![
        plugin("Ok95.esp", Some(0.95)),
        plugin("Ok1.esp", Some(1.0)),
    ]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
}

#[test]
fn a_missing_header_version_is_not_flagged() {
    let findings = run(vec![plugin("NoHeader.esp", None)]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
}

#[test]
fn only_the_offenders_are_flagged_among_many() {
    let findings = run(vec![
        plugin("Ok.esp", Some(1.0)),
        plugin("Bad.esp", Some(0.85)),
        plugin("AlsoOk.esm", Some(0.95)),
        plugin("Unknown.esp", None),
    ]);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].title.contains("Bad.esp"));
}
