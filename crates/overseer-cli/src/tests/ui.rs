//! Tests for the CLI ui helpers

use super::*;

#[test]
fn from_flags_maps_the_gate_truth_table() {
    assert_eq!(Gate::from_flags(false, false), Gate::Preview);
    assert_eq!(Gate::from_flags(false, true), Gate::Apply);
    assert_eq!(Gate::from_flags(true, false), Gate::DryRun);
}

#[test]
fn dry_run_wins_when_both_flags_are_set() {
    assert_eq!(Gate::from_flags(true, true), Gate::DryRun);
}

#[test]
fn only_apply_skips_the_preview() {
    assert!(!Gate::Apply.is_preview());
    assert!(Gate::Preview.is_preview());
    assert!(Gate::DryRun.is_preview());
}
