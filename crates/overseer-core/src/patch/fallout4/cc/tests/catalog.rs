//! Tests for the Creation Club catalog allow-list

use super::*;

#[test]
fn matches_a_seeded_entry_case_insensitively() {
    assert!(is_cc("ccBGSFO4001-PipBoy(Black).esl"));
    assert!(is_cc("CCBGSFO4001-PIPBOY(BLACK).ESL"));
    assert!(is_cc("ccbgsfo4001-pipboy(black).esl"));
    assert!(is_cc("ccRZRFO4001-TunnelSnakes.esm"));
}

#[test]
fn rejects_lookalike_user_mods_and_normal_plugins() {
    assert!(!is_cc("ccFakeUserMod.esl"));
    assert!(!is_cc("MyMod.esp"));
}

#[test]
fn ignores_comments_and_blank_lines_in_the_catalog() {
    assert!(!is_cc(""));
    assert!(!is_cc("#"));
    // the header comment text must never be treated as a catalog entry
    assert!(!is_cc("# Fallout 4 Creation Club plugin allow-list."));
}
