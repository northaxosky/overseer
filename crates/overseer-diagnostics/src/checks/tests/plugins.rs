//! Tests for the unreadable-plugins check

use super::*;
use crate::finding::Severity;
use overseer_core::plugins::UnreadablePlugin;

#[test]
fn no_unreadable_plugins_reports_all_clear() {
    let findings = super::run(&GameContext::default());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(
        findings[0]
            .title
            .contains("All plugins were read successfully")
    );
}

#[test]
fn each_unreadable_plugin_warns_with_its_reason() {
    let findings = super::run(&GameContext {
        unreadable_plugins: vec![
            UnreadablePlugin {
                name: "Broken.esp".to_owned(),
                reason: "unexpected end of file".to_owned(),
            },
            UnreadablePlugin {
                name: "Bad.esl".to_owned(),
                reason: "bad record header".to_owned(),
            },
        ],
        ..GameContext::default()
    });
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|f| f.severity == Severity::Warning));
    assert!(findings[0].title.contains("Broken.esp"));
    assert!(findings[0].title.contains("could not be read"));
    assert!(
        findings[0]
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("unexpected end of file"))
    );
    assert!(findings[1].title.contains("Bad.esl"));
}
