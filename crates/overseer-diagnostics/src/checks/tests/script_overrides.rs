//! Tests for the script-override check

use super::*;
use crate::finding::Severity;

fn scan(name: &str, mod_name: &str) -> ScriptOverrideScan {
    ScriptOverrideScan {
        name: name.to_owned(),
        mod_name: mod_name.to_owned(),
    }
}

fn run(scans: Vec<ScriptOverrideScan>) -> Vec<Finding> {
    super::run(&GameContext {
        script_overrides: scans,
        ..GameContext::default()
    })
}

#[test]
fn an_override_warns_and_names_the_file_and_mod() {
    let findings = run(vec![scan("Actor.pex", "Some Mod")]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("Actor.pex"));
    assert!(findings[0].title.contains("Some Mod"));
    assert!(findings[0].title.contains("base F4SE script"));
    assert!(findings[0].detail.as_deref().unwrap().contains("F4SE"));
}

#[test]
fn every_override_is_flagged() {
    let findings = run(vec![scan("Game.pex", "A"), scan("Form.pex", "B")]);
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|f| f.severity == Severity::Warning));
}

#[test]
fn no_overrides_reports_a_clean_info() {
    let findings = run(vec![]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
}
