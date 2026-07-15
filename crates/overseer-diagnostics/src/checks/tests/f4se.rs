//! Tests for the F4SE health check

use super::*;
use crate::context::{F4sePluginScan, UnreadableF4se};
use crate::finding::Severity;
use overseer_core::detect::Generation;
use overseer_core::f4se::F4sePlugin;

fn run(
    game: Option<Generation>,
    loader: Option<Generation>,
    address: AddressLibraryStatus,
) -> Vec<Finding> {
    super::run(&GameContext {
        runtime_family: game,
        loader_family: loader,
        address_library: address,
        ..GameContext::default()
    })
}

#[test]
fn a_matching_loader_is_silent() {
    let findings = run(
        Some(Generation::OldGen),
        Some(Generation::OldGen),
        AddressLibraryStatus::NotApplicable,
    );
    assert!(findings.is_empty());
}

#[test]
fn a_mismatched_loader_errors() {
    let findings = run(
        Some(Generation::OldGen),
        Some(Generation::NextGen),
        AddressLibraryStatus::NotApplicable,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(findings[0].title.contains("NextGen"));
    assert!(findings[0].title.contains("OldGen"));
}

#[test]
fn an_unknown_family_does_not_warn() {
    assert!(
        run(
            None,
            Some(Generation::NextGen),
            AddressLibraryStatus::NotApplicable
        )
        .is_empty()
    );
    assert!(
        run(
            Some(Generation::NextGen),
            None,
            AddressLibraryStatus::NotApplicable
        )
        .is_empty()
    );
}

#[test]
fn a_missing_address_library_warns() {
    let findings = run(
        Some(Generation::OldGen),
        Some(Generation::OldGen),
        AddressLibraryStatus::Missing {
            expected: "version-1-10-163-0.bin".to_owned(),
        },
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("version-1-10-163-0.bin"));
}

#[test]
fn a_present_address_library_is_silent() {
    assert!(
        run(
            Some(Generation::OldGen),
            Some(Generation::OldGen),
            AddressLibraryStatus::Present
        )
        .is_empty()
    );
}

fn plugin_ctx(scans: Vec<F4sePluginScan>, packed: Option<u32>) -> GameContext {
    GameContext {
        runtime_family: Some(Generation::Anniversary),
        runtime_packed: packed,
        f4se_plugins: scans,
        ..GameContext::default()
    }
}

fn scan(name: &str, supports_ngae: bool, compatible: &[u32]) -> F4sePluginScan {
    F4sePluginScan {
        name: name.to_owned(),
        mod_name: "ModA".to_owned(),
        plugin: F4sePlugin {
            supports_og: !supports_ngae,
            supports_ngae,
            compatible: compatible.to_vec(),
            address_independence: 0,
            structure_independence: 0,
        },
    }
}

#[test]
fn a_plugin_advertising_the_runtime_is_silent() {
    let findings = plugin_ctx(
        vec![scan("ok.dll", true, &[0x010B_0DD0])],
        Some(0x010B_0DD0),
    );
    assert!(super::run(&findings).is_empty());
}

#[test]
fn a_plugin_missing_the_runtime_warns() {
    let findings = super::run(&plugin_ctx(
        vec![scan("old.dll", true, &[0x010A_3D80])],
        Some(0x010B_0DD0),
    ));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("old.dll"));
}

#[test]
fn an_og_only_plugin_warns_on_anniversary() {
    let findings = super::run(&plugin_ctx(
        vec![scan("legacy.dll", false, &[])],
        Some(0x010B_0DD0),
    ));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
}

#[test]
fn plugins_are_silent_when_runtime_unknown() {
    assert!(super::run(&plugin_ctx(vec![scan("x.dll", true, &[0x010A_3D80])], None)).is_empty());
}

#[test]
fn a_version_independent_plugin_is_silent_without_an_exact_match() {
    // AE-band address + structure independence, so F4SE loads it on AE despite compat listing only OG
    let mut s = scan("indep.dll", true, &[0x010A_3D80]);
    s.plugin.address_independence = 0x4; // Address Library 1.11.137
    s.plugin.structure_independence = 0x4; // 1.11.137 struct layout
    assert!(super::run(&plugin_ctx(vec![s], Some(0x010B_0DD0))).is_empty());
}

#[test]
fn a_nextgen_only_independent_plugin_still_warns_on_anniversary() {
    // NG-band independence (1.10.980) doesn't cover AE, and compat omits it → warn
    let mut s = scan("ng.dll", true, &[]);
    s.plugin.address_independence = 0x2;
    s.plugin.structure_independence = 0x2;
    let findings = super::run(&plugin_ctx(vec![s], Some(0x010B_0DD0)));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
}

#[test]
fn a_plugin_with_no_og_entry_point_warns_on_old_gen() {
    // Exports F4SEPlugin_Load but neither Query nor Version: no valid entry point on OG
    let ctx = GameContext {
        runtime_family: Some(Generation::OldGen),
        runtime_packed: Some(0x010A_3D80),
        f4se_plugins: vec![F4sePluginScan {
            name: "loadonly.dll".to_owned(),
            mod_name: "ModA".to_owned(),
            plugin: F4sePlugin::default(),
        }],
        ..GameContext::default()
    };
    let findings = super::run(&ctx);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("loadonly.dll"));
}

#[test]
fn a_real_og_plugin_is_silent_on_old_gen() {
    // supports_og = true (exports Query); it advertises OG and must not be flagged
    let ctx = GameContext {
        runtime_family: Some(Generation::OldGen),
        runtime_packed: Some(0x010A_3D80),
        f4se_plugins: vec![scan("legacy.dll", false, &[])],
        ..GameContext::default()
    };
    assert!(super::run(&ctx).is_empty());
}

#[test]
fn an_unreadable_f4se_plugin_warns() {
    let ctx = GameContext {
        unreadable_f4se: vec![UnreadableF4se {
            name: "Buffout4.dll".to_owned(),
            mod_name: "Buffout 4".to_owned(),
            reason: "access denied".to_owned(),
        }],
        ..GameContext::default()
    };
    let findings = super::run(&ctx);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("Buffout4.dll"));
    assert!(findings[0].title.contains("Buffout 4"));
    assert!(findings[0].title.contains("could not be read"));
}
