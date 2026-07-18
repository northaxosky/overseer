//! Tests for load-order reconciliation

use super::*;

use crate::test_support::temp_instance;

fn meta(name: &str, is_master: bool) -> PluginMeta {
    crate::test_support::plugin_meta(name, is_master, false, &[])
}

fn meta_with_masters(name: &str, is_master: bool, masters: &[&str]) -> PluginMeta {
    crate::test_support::plugin_meta(name, is_master, false, masters)
}

fn order_of(lo: &PluginLoadOrder) -> Vec<&str> {
    lo.plugins.iter().map(|e| e.name.as_str()).collect()
}

fn lo(profile: &str, plugins: Vec<PluginEntry>) -> PluginLoadOrder {
    PluginLoadOrder {
        profile: profile.to_owned(),
        plugins,
    }
}

fn active(name: &str) -> PluginEntry {
    PluginEntry {
        name: name.to_owned(),
        active: true,
    }
}

fn inactive(name: &str) -> PluginEntry {
    PluginEntry {
        name: name.to_owned(),
        active: false,
    }
}

// --- parse / serialize (asterisk format) ---

#[test]
fn parses_asterisk_active_and_bare_inactive() {
    let plugins = parse_plugins("*Active.esp\nInactive.esp\n");
    assert_eq!(
        plugins,
        vec![active("Active.esp"), inactive("Inactive.esp")]
    );
}

#[test]
fn parse_skips_blank_and_comment_lines() {
    let plugins = parse_plugins("# header\n\n*A.esp\nB.esp\n");
    assert_eq!(plugins, vec![active("A.esp"), inactive("B.esp")]);
}

#[test]
fn serialize_uses_asterisk_for_active_only() {
    let order = lo("P", vec![active("On.esp"), inactive("Off.esp")]);
    assert_eq!(order.to_plugins_string(), "*On.esp\nOff.esp\n");
}

#[test]
fn serialize_parse_round_trips() {
    let order = lo(
        "P",
        vec![active("A.esp"), inactive("B.esp"), active("C.esp")],
    );
    assert_eq!(parse_plugins(&order.to_plugins_string()), order.plugins);
}

// --- load / save ---

#[test]
fn load_missing_file_is_empty() {
    let (_t, instance) = temp_instance();
    let order = PluginLoadOrder::load(&instance, "Default").expect("load");
    assert!(order.plugins.is_empty());
    assert_eq!(order.profile, "Default");
}

#[test]
fn save_then_load_round_trips() {
    let (_t, instance) = temp_instance();
    let order = lo("Default", vec![active("A.esp"), inactive("B.esp")]);
    order.save(&instance).expect("save");
    let loaded = PluginLoadOrder::load(&instance, "Default").expect("load");
    assert_eq!(loaded.plugins, order.plugins);
}

#[test]
fn save_writes_plugins_txt_in_profile_dir() {
    let (_t, instance) = temp_instance();
    lo("Survival", vec![active("A.esp")])
        .save(&instance)
        .expect("save");
    assert!(
        instance
            .profile_dir("Survival")
            .join("plugins.txt")
            .exists()
    );
}

// --- activate / deactivate ---

#[test]
fn activate_and_deactivate_toggle_state() {
    let mut order = lo("P", vec![inactive("M.esp")]);
    order.activate("m.esp").expect("activate");
    assert!(order.is_active("M.esp"));
    order.deactivate("M.ESP").expect("deactivate");
    assert!(!order.is_active("M.esp"));
}

#[test]
fn activate_missing_is_an_error() {
    let mut order = lo("P", vec![]);
    assert!(matches!(
        order.activate("ghost.esp").expect_err("err"),
        PluginError::NotInLoadOrder(_)
    ));
}

// --- reconcile ---

#[test]
fn reconcile_appends_new_plugins_active() {
    let mut order = lo("P", vec![active("Existing.esp")]);
    let discovered = [meta("Existing.esp", false), meta("New.esp", false)];
    let changed = order.reconcile(&discovered);
    assert!(changed);
    assert_eq!(order_of(&order), ["Existing.esp", "New.esp"]);
    assert!(order.is_active("New.esp"));
}

#[test]
fn reconcile_drops_vanished_plugins() {
    let mut order = lo("P", vec![active("Keep.esp"), active("Gone.esp")]);
    let changed = order.reconcile(&[meta("Keep.esp", false)]);
    assert!(changed);
    assert_eq!(order_of(&order), ["Keep.esp"]);
}

#[test]
fn reconcile_sorts_masters_before_normal_plugins() {
    // Stored in a load-order-invalid arrangement (a normal plugin before a master)
    let mut order = lo("P", vec![active("Patch.esp"), active("Core.esm")]);
    let discovered = [meta("Patch.esp", false), meta("Core.esm", true)];
    let changed = order.reconcile(&discovered);
    assert!(changed, "the master had to move up");
    assert_eq!(order_of(&order), ["Core.esm", "Patch.esp"]);
}

#[test]
fn reconcile_is_stable_within_master_and_normal_groups() {
    let mut order = lo("P", vec![]);
    // Discovery order: m1(normal), A(master), m2(normal), B(master)
    let discovered = [
        meta("m1.esp", false),
        meta("A.esm", true),
        meta("m2.esp", false),
        meta("B.esm", true),
    ];
    order.reconcile(&discovered);
    // Masters first (A, B in their relative order), then normals (m1, m2)
    assert_eq!(order_of(&order), ["A.esm", "B.esm", "m1.esp", "m2.esp"]);
}

#[test]
fn reconcile_preserves_active_state_of_existing() {
    let mut order = lo("P", vec![inactive("Keep.esp")]);
    order.reconcile(&[meta("Keep.esp", false)]);
    assert!(
        !order.is_active("Keep.esp"),
        "existing inactive stays inactive"
    );
}

#[test]
fn reconcile_removes_duplicates_and_keeps_the_first_occurrence() {
    let mut order = lo(
        "P",
        vec![
            inactive("Patch.esp"),
            active("PATCH.ESP"),
            active("Other.esp"),
        ],
    );
    let discovered = [meta("Patch.esp", false), meta("Other.esp", false)];

    assert!(order.reconcile(&discovered));
    assert_eq!(order_of(&order), ["Patch.esp", "Other.esp"]);
    assert!(!order.is_active("patch.esp"));
}

#[test]
fn reconcile_reports_no_change_when_in_sync_and_sorted() {
    let mut order = lo("P", vec![active("Core.esm"), active("Patch.esp")]);
    let discovered = [meta("Core.esm", true), meta("Patch.esp", false)];
    assert!(!order.reconcile(&discovered));
}

#[test]
fn reconcile_orders_normal_plugin_before_its_normal_dependant() {
    let mut order = lo("P", vec![active("Patch.esp"), active("Armor.esp")]);
    let discovered = [
        meta_with_masters("Patch.esp", false, &["Armor.esp"]),
        meta("Armor.esp", false),
    ];

    assert!(order.reconcile(&discovered));
    assert_eq!(order_of(&order), ["Armor.esp", "Patch.esp"]);
}

#[test]
fn reconcile_orders_master_before_its_master_dependant() {
    let mut order = lo("P", vec![active("B.esm"), active("A.esm")]);
    let discovered = [
        meta_with_masters("B.esm", true, &["A.esm"]),
        meta("A.esm", true),
    ];

    assert!(order.reconcile(&discovered));
    assert_eq!(order_of(&order), ["A.esm", "B.esm"]);
}

#[test]
fn reconcile_hoists_normal_dependency_before_master() {
    let mut order = lo("P", vec![active("A.esm"), active("N.esp")]);
    let discovered = [
        meta_with_masters("A.esm", true, &["N.esp"]),
        meta("N.esp", false),
    ];

    assert!(order.reconcile(&discovered));
    assert_eq!(order_of(&order), ["N.esp", "A.esm"]);
}

#[test]
fn reconcile_uses_master_flag_to_break_independent_ties() {
    let mut order = lo("P", vec![active("Patch.esp"), active("Core.esm")]);
    let discovered = [meta("Patch.esp", false), meta("Core.esm", true)];

    assert!(order.reconcile(&discovered));
    assert_eq!(order_of(&order), ["Core.esm", "Patch.esp"]);
}

#[test]
fn reconcile_ignores_base_master_absent_from_managed_order() {
    let mut order = lo("P", vec![active("Patch.esp")]);
    let discovered = [meta_with_masters("Patch.esp", false, &["Fallout4.esm"])];

    assert!(!order.reconcile(&discovered));
    assert_eq!(order_of(&order), ["Patch.esp"]);
}

#[test]
fn reconcile_dependency_correct_order_is_idempotent() {
    let mut order = lo("P", vec![active("Armor.esp"), inactive("Patch.esp")]);
    let discovered = [
        meta("Armor.esp", false),
        meta_with_masters("Patch.esp", false, &["Armor.esp"]),
    ];

    assert!(!order.reconcile(&discovered));
    assert_eq!(order.plugins, [active("Armor.esp"), inactive("Patch.esp")]);
}

#[test]
fn dependency_ordered_detects_master_tiebreak_snaps() {
    let discovered = [
        meta_with_masters("J.esm", true, &["B.esp"]),
        meta("A.esp", false),
        meta("B.esp", false),
    ];
    let fixed = lo("P", vec![active("A.esp"), active("B.esp"), active("J.esm")]);
    let snapped = lo("P", vec![active("B.esp"), active("A.esp"), active("J.esm")]);

    assert!(fixed.is_dependency_ordered(&discovered));
    assert!(!snapped.is_dependency_ordered(&discovered));
}

#[test]
fn reconcile_breaks_cycles_with_the_same_deterministic_tie_break() {
    let mut order = lo("P", vec![active("B.esp"), active("A.esm")]);
    let discovered = [
        meta_with_masters("B.esp", false, &["A.esm"]),
        meta_with_masters("A.esm", true, &["B.esp"]),
    ];

    assert!(order.reconcile(&discovered));
    assert_eq!(order_of(&order), ["A.esm", "B.esp"]);
}

#[test]
fn reconcile_counts_duplicate_declared_masters_once() {
    let mut order = lo(
        "P",
        vec![active("A.esp"), active("Patch.esp"), active("Other.esp")],
    );
    let discovered = [
        meta("A.esp", false),
        meta_with_masters("Patch.esp", false, &["A.esp", "A.ESP"]),
        meta("Other.esp", false),
    ];

    assert!(!order.reconcile(&discovered));
    assert_eq!(order_of(&order), ["A.esp", "Patch.esp", "Other.esp"]);
}

#[test]
fn reconcile_orders_plugins_blocked_downstream_of_a_cycle() {
    let mut order = lo(
        "P",
        vec![active("Patch.esp"), active("A.esp"), active("B.esp")],
    );
    let discovered = [
        meta_with_masters("A.esp", false, &["B.esp"]),
        meta_with_masters("B.esp", false, &["A.esp"]),
        meta_with_masters("Patch.esp", false, &["A.esp"]),
    ];

    assert!(order.reconcile(&discovered));
    assert!(
        order.position("Patch.esp").expect("Patch is present")
            > order.position("A.esp").expect("A is present")
    );
}
