//! Tests for the DLC consistency check

use super::*;
use crate::finding::Severity;

fn ctx(groups: Vec<DlcGroupState>) -> GameContext {
    GameContext {
        dlc_consistency: groups,
        ..GameContext::default()
    }
}

fn group(name: &'static str, off: &[&'static str], missing: &[&'static str]) -> DlcGroupState {
    DlcGroupState {
        group: name,
        off_revision: off.to_vec(),
        missing: missing.to_vec(),
    }
}

#[test]
fn no_dlc_installed_is_silent() {
    assert!(super::run(&ctx(Vec::new())).is_empty());
}

#[test]
fn all_consistent_reports_a_single_info() {
    let findings = super::run(&ctx(vec![
        group("DLCCoast", &[], &[]),
        group("DLCNukaWorld", &[], &[]),
    ]));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(findings[0].title.contains("consistency revision"));
}

#[test]
fn an_off_revision_group_warns_and_names_it() {
    let findings = super::run(&ctx(vec![
        group("DLCCoast", &["Data/DLCCoast - Textures.ba2"], &[]),
        group("DLCNukaWorld", &[], &[]),
    ]));
    // Only the off-revision group warns; no clean-bill Info alongside a warning
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("DLCCoast"));
    assert!(
        findings[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("patch dlc-consistency")
    );
}

#[test]
fn a_missing_file_group_warns_and_blocks_the_clean_info() {
    let findings = super::run(&ctx(vec![group(
        "DLCCoast",
        &[],
        &["Data/DLCCoast - Textures.ba2"],
    )]));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("missing"));
    assert!(
        findings[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("Verify the game files")
    );
}
