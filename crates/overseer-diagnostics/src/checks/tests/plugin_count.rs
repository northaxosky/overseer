//! Tests for the plugin-count limits check

use super::*;
use crate::finding::Severity;
use overseer_core::plugins::PluginMeta;

fn plugin(is_light: bool) -> PluginMeta {
    overseer_core::test_support::plugin_meta(
        if is_light { "Light.esl" } else { "Full.esp" },
        false,
        is_light,
        &[],
    )
}

fn ctx(full: usize, light: usize) -> GameContext {
    let mut loaded = vec![plugin(false); full];
    loaded.extend(vec![plugin(true); light]);
    GameContext {
        loaded_plugins: loaded,
        ..GameContext::default()
    }
}

#[test]
fn within_limits_is_info() {
    let findings = super::run(&ctx(10, 10));
    assert!(findings.iter().all(|f| f.severity == Severity::Info));
    assert!(findings[0].title.contains("10 / 254"));
    assert!(findings[1].title.contains("10 / 4096"));
}

#[test]
fn over_the_full_limit_is_an_error() {
    let findings = super::run(&ctx(255, 0));
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(findings[0].title.contains("255 / 254"));
}

#[test]
fn approaching_the_full_limit_warns() {
    // 254 * 9/10 = 228 is the warning threshold
    let findings = super::run(&ctx(245, 0));
    assert_eq!(findings[0].severity, Severity::Warning);
}

#[test]
fn light_plugins_count_against_the_light_limit() {
    let findings = super::run(&ctx(0, 4097));
    assert_eq!(findings[0].severity, Severity::Info, "no full plugins");
    assert_eq!(findings[1].severity, Severity::Error);
    assert!(findings[1].title.contains("4097 / 4096"));
}
