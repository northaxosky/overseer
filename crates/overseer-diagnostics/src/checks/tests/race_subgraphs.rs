//! Tests for the race-subgraph heuristic check

use super::*;
use crate::context::SaddCount;
use crate::finding::Severity;

fn ctx(records: Vec<SaddCount>) -> GameContext {
    GameContext {
        sadd_records: records,
        ..GameContext::default()
    }
}

fn sadd(plugin: &str, count: usize) -> SaddCount {
    SaddCount {
        plugin: plugin.to_owned(),
        count,
    }
}

#[test]
fn under_the_threshold_reports_a_clean_info() {
    let findings = super::run(&ctx(vec![sadd("A.esp", 50), sadd("B.esp", 40)]));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(findings[0].title.contains("within the safe range"));
}

#[test]
fn exactly_at_the_threshold_reports_a_clean_info() {
    let findings = super::run(&ctx(vec![sadd("A.esp", 100)]));
    assert_eq!(findings.len(), 1, "the threshold is exclusive");
    assert_eq!(findings[0].severity, Severity::Info);
}

#[test]
fn over_the_threshold_warns_with_the_total_count_and_plugin_names() {
    let findings = super::run(&ctx(vec![sadd("A.esp", 80), sadd("B.esp", 40)]));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("120"), "80 + 40");
    assert!(findings[0].title.contains("2 plugins"));
    assert!(
        findings[0].title.contains("A.esp") && findings[0].title.contains("B.esp"),
        "the warning names the offending plugins"
    );
}

#[test]
fn no_records_reports_a_clean_info() {
    let findings = super::run(&ctx(Vec::new()));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
}
