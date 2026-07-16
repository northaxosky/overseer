//! Tests for the INI configuration check

use super::*;
use crate::finding::Severity;
use overseer_core::ini::{GameInis, Ini};

fn ctx(settings: &str) -> GameContext {
    GameContext {
        inis: Some(GameInis {
            settings: Ini::parse(settings),
            prefs: Ini::default(),
        }),
        ..GameContext::default()
    }
}

fn severities(findings: &[Finding]) -> Vec<Severity> {
    findings.iter().map(|f| f.severity).collect()
}

#[test]
fn missing_inis_warn() {
    // The default context has `inis: None` and `ini_status: Missing`
    let findings = super::run(&GameContext::default());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("were not found"));
}

#[test]
fn present_but_unparsed_inis_warn() {
    let findings = super::run(&GameContext {
        ini_status: IniStatus::Present,
        ..GameContext::default()
    });
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("could not be read"));
}

#[test]
fn unreadable_inis_warn() {
    let findings = super::run(&GameContext {
        ini_status: IniStatus::Unreadable("access denied".to_owned()),
        ..GameContext::default()
    });
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("could not be read"));
    assert!(
        findings[0]
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("access denied"))
    );
}

#[test]
fn correct_invalidation_is_a_single_info() {
    let findings = super::run(&ctx(
        "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
    ));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(findings[0].title.contains("enabled"));
}

#[test]
fn invalidation_off_is_an_error() {
    let findings = super::run(&ctx(
        "[Archive]\nbInvalidateOlderFiles=0\nsResourceDataDirsFinal=\n",
    ));
    assert!(findings.iter().any(|f| f.severity == Severity::Error));
}

#[test]
fn absent_invalidation_keys_flag_both() {
    // Empty INIs: invalidation absent (Error) and DataDirsFinal not explicitly empty (Warning)
    let sev = severities(&super::run(&ctx("")));
    assert!(sev.contains(&Severity::Error));
    assert!(sev.contains(&Severity::Warning));
}

#[test]
fn nonempty_resource_dirs_final_warns() {
    let findings = super::run(&ctx(
        "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=STRINGS\\\n",
    ));
    // Invalidation on, but DataDirsFinal not empty: one Warning, no Info/Error
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("sResourceDataDirsFinal"));
}

#[test]
fn use_my_games_directory_zero_gates_everything() {
    // Even with broken invalidation present, the gate is the only finding
    let findings = super::run(&ctx(
        "[General]\nbUseMyGamesDirectory=0\n[Archive]\nbInvalidateOlderFiles=0\n",
    ));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("My Games"));
}

#[test]
fn a_non_english_language_adds_an_info() {
    let findings = super::run(&ctx(
        "[General]\nsLanguage=DE\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
    ));
    // Case-insensitive: `DE` is non-English
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Info && f.title.contains("`de`"))
    );
}

#[test]
fn english_language_adds_no_language_info() {
    let findings = super::run(&ctx(
        "[General]\nsLanguage=en\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
    ));
    // Only the invalidation Info; English needs no note
    assert_eq!(findings.len(), 1);
    assert!(findings[0].title.contains("enabled"));
}

#[test]
fn stestfile_entries_warn() {
    let findings = super::run(&ctx(
        "[General]\nsTestFile1=WIP.esp\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
    ));
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Warning && f.title.contains("sTestFile"))
    );
}

#[test]
fn an_empty_stestfile_does_not_warn() {
    // A blank value isn't a valid test file, so it shouldn't trip the warning
    let findings = super::run(&ctx(
        "[General]\nsTestFile1=\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
    ));
    assert!(!findings.iter().any(|f| f.title.contains("sTestFile")));
}
