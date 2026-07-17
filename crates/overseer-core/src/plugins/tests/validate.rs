//! Tests for the pure plugin load-order validator

use super::*;
use crate::plugins::PluginEntry;
use crate::test_support::plugin_meta;

fn entry(name: &str, active: bool) -> PluginEntry {
    PluginEntry {
        name: name.to_owned(),
        active,
    }
}

fn order(entries: &[(&str, bool)]) -> PluginLoadOrder {
    PluginLoadOrder {
        profile: "P".to_owned(),
        plugins: entries
            .iter()
            .map(|(name, active)| entry(name, *active))
            .collect(),
    }
}

#[test]
fn dependency_after_dependant_is_an_error() {
    let order = order(&[("Patch.esp", true), ("Armor.esp", true)]);
    let discovered = [
        plugin_meta("Patch.esp", false, false, &["Armor.esp"]),
        plugin_meta("Armor.esp", false, false, &[]),
    ];

    assert_eq!(
        validate_order(&order, &discovered),
        vec![PluginViolation::DependencyAfterDependant {
            plugin: "Patch.esp".to_owned(),
            dependency: "Armor.esp".to_owned(),
        }]
    );
    assert_eq!(
        validate_order(&order, &discovered)[0].severity(),
        Severity::Error
    );
}

#[test]
fn preceding_and_inactive_dependencies_do_not_violate_order() {
    let order = order(&[("Armor.esp", true), ("Patch.esp", false)]);
    let discovered = [
        plugin_meta("Patch.esp", false, false, &["Armor.esp"]),
        plugin_meta("Armor.esp", false, false, &[]),
    ];

    assert!(validate_order(&order, &discovered).is_empty());
}

#[test]
fn base_master_absent_from_order_is_satisfied() {
    let order = order(&[("Patch.esp", true)]);
    let discovered = [plugin_meta("Patch.esp", false, false, &["Fallout4.esm"])];

    assert!(validate_order(&order, &discovered).is_empty());
}

#[test]
fn master_after_normal_is_an_error() {
    let order = order(&[("Patch.esp", true), ("Core.esm", true)]);
    let discovered = [
        plugin_meta("Patch.esp", false, false, &[]),
        plugin_meta("Core.esm", true, false, &[]),
    ];

    assert_eq!(
        validate_order(&order, &discovered),
        vec![PluginViolation::MasterAfterNormal("Core.esm".to_owned())]
    );
}

#[test]
fn hoisted_master_after_its_declared_normal_is_not_flagged() {
    let order = order(&[("N.esp", true), ("A.esm", true)]);
    let discovered = [
        plugin_meta("A.esm", true, false, &["N.esp"]),
        plugin_meta("N.esp", false, false, &[]),
    ];

    assert!(validate_order(&order, &discovered).is_empty());
}

#[test]
fn unhoisted_master_after_normal_is_still_flagged() {
    let order = order(&[("N.esp", true), ("M.esm", true)]);
    let discovered = [
        plugin_meta("M.esm", true, false, &[]),
        plugin_meta("N.esp", false, false, &[]),
    ];

    let violations = validate_order(&order, &discovered);
    assert_eq!(
        violations,
        vec![PluginViolation::MasterAfterNormal("M.esm".to_owned())]
    );
    assert_eq!(violations[0].severity(), Severity::Error);
}

#[test]
fn masters_before_normals_are_valid() {
    let order = order(&[("Core.esm", true), ("Patch.esp", true)]);
    let discovered = [
        plugin_meta("Core.esm", true, false, &[]),
        plugin_meta("Patch.esp", false, false, &[]),
    ];

    assert!(validate_order(&order, &discovered).is_empty());
}

#[test]
fn duplicate_plugin_is_reported_once_case_insensitively() {
    let order = order(&[
        ("Patch.esp", true),
        ("PATCH.ESP", false),
        ("patch.esp", true),
    ]);
    let discovered = [plugin_meta("Patch.esp", false, false, &[])];

    assert_eq!(
        validate_order(&order, &discovered),
        vec![PluginViolation::DuplicatePlugin("Patch.esp".to_owned())]
    );
}

#[test]
fn unique_plugins_have_no_duplicate_violation() {
    let order = order(&[("A.esp", true), ("B.esp", true)]);
    let discovered = [
        plugin_meta("A.esp", false, false, &[]),
        plugin_meta("B.esp", false, false, &[]),
    ];

    assert!(validate_order(&order, &discovered).is_empty());
}

#[test]
fn missing_order_reference_is_a_warning() {
    let order = order(&[("Gone.esp", true)]);

    let violations = validate_order(&order, &[]);
    assert_eq!(
        violations,
        vec![PluginViolation::OrderReferencesMissing(
            "Gone.esp".to_owned()
        )]
    );
    assert_eq!(violations[0].severity(), Severity::Warning);
}

#[test]
fn discovered_order_reference_is_not_missing() {
    let order = order(&[("Present.esp", true)]);
    let discovered = [plugin_meta("present.ESP", false, false, &[])];

    assert!(validate_order(&order, &discovered).is_empty());
}

#[test]
fn self_dependency_is_a_cycle() {
    let order = order(&[("A.esp", true)]);
    let discovered = [plugin_meta("A.esp", false, false, &["A.esp"])];

    assert_eq!(
        validate_order(&order, &discovered),
        vec![PluginViolation::CyclicDependency(vec!["A.esp".to_owned()])]
    );
}

#[test]
fn mutual_dependency_is_one_cycle_without_order_violations() {
    let order = order(&[("A.esp", true), ("B.esp", true)]);
    let discovered = [
        plugin_meta("A.esp", false, false, &["B.esp"]),
        plugin_meta("B.esp", false, false, &["A.esp"]),
    ];

    let violations = validate_order(&order, &discovered);
    assert_eq!(
        violations,
        vec![PluginViolation::CyclicDependency(vec![
            "A.esp".to_owned(),
            "B.esp".to_owned(),
        ])]
    );
    assert!(
        !violations
            .iter()
            .any(|violation| matches!(violation, PluginViolation::DependencyAfterDependant { .. }))
    );
}

#[test]
fn inactive_dependency_cycle_is_reported() {
    let order = order(&[("A.esp", false), ("B.esp", false)]);
    let discovered = [
        plugin_meta("A.esp", false, false, &["B.esp"]),
        plugin_meta("B.esp", false, false, &["A.esp"]),
    ];

    assert_eq!(
        validate_order(&order, &discovered),
        vec![PluginViolation::CyclicDependency(vec![
            "A.esp".to_owned(),
            "B.esp".to_owned(),
        ])]
    );
}
