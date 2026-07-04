//! F4SE health: the loader must match the game's runtime, and deployed F4SE plugins need
//! the matching Address Library.

use super::Check;
use crate::context::{AddressLibraryStatus, GameContext};
use crate::finding::{Finding, Severity};
use overseer_core::detect::Generation;

/// Reports F4SE setup problems: a loader for the wrong runtime, or a missing Address Library
pub struct F4se;

impl Check for F4se {
    fn id(&self) -> &'static str {
        "f4se"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        // A loader for the wrong runtime fails to launch the game; only flag when both the game and loader families are known and disagree.
        if let (Some(game), Some(loader)) = (ctx.runtime_family, ctx.loader_family)
            && game != loader
        {
            findings.push(Finding::new(
                Severity::Error,
                format!("F4SE is for {loader:?} but the game is {game:?}"),
                Some("F4SE won't launch; install the F4SE build for your game version".to_owned()),
            ));
        }

        if let AddressLibraryStatus::Missing { expected } = &ctx.address_library {
            findings.push(Finding::new(
                Severity::Warning,
                format!("Address Library `{expected}` is missing"),
                Some("F4SE plugins need it; install Address Library (Nexus mod 47327)".to_owned()),
            ));
        }

        // A deployed F4SE plugin that doesn't advertise the installed runtime won't load.
        if let (Some(packed), Some(game)) = (ctx.runtime_packed, ctx.runtime_family) {
            for p in &ctx.f4se_plugins {
                let advertises = if p.plugin.supports_ngae {
                    p.plugin.supports(packed) || p.plugin.version_independent_for(game)
                } else {
                    game == Generation::OldGen // OG-only plugins (Query, no Version)
                };
                if !advertises {
                    findings.push(Finding::new(
                        Severity::Warning,
                        format!(
                            "`{}` (from `{}`) may not support {game:?}",
                            p.name, p.mod_name
                        ),
                        Some("Update the plugin for your F4SE/runtime version".to_owned()),
                    ));
                }
            }
        }

        findings
    }
}

// ---------------------------------------------------------------------------; Tests; ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::F4sePluginScan;
    use overseer_core::detect::Generation;
    use overseer_core::f4se::F4sePlugin;

    fn run(
        game: Option<Generation>,
        loader: Option<Generation>,
        address: AddressLibraryStatus,
    ) -> Vec<Finding> {
        F4se.run(&GameContext {
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
        assert!(F4se.run(&findings).is_empty());
    }

    #[test]
    fn a_plugin_missing_the_runtime_warns() {
        let findings = F4se.run(&plugin_ctx(
            vec![scan("old.dll", true, &[0x010A_3D80])],
            Some(0x010B_0DD0),
        ));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("old.dll"));
    }

    #[test]
    fn an_og_only_plugin_warns_on_anniversary() {
        let findings = F4se.run(&plugin_ctx(
            vec![scan("legacy.dll", false, &[])],
            Some(0x010B_0DD0),
        ));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn plugins_are_silent_when_runtime_unknown() {
        assert!(
            F4se.run(&plugin_ctx(vec![scan("x.dll", true, &[0x010A_3D80])], None))
                .is_empty()
        );
    }

    #[test]
    fn a_version_independent_plugin_is_silent_without_an_exact_match() {
        // AE-band address + structure independence, so F4SE loads it on AE despite compat listing only OG.
        let mut s = scan("indep.dll", true, &[0x010A_3D80]);
        s.plugin.address_independence = 0x4; // Address Library 1.11.137
        s.plugin.structure_independence = 0x4; // 1.11.137 struct layout
        assert!(F4se.run(&plugin_ctx(vec![s], Some(0x010B_0DD0))).is_empty());
    }

    #[test]
    fn a_nextgen_only_independent_plugin_still_warns_on_anniversary() {
        // NG-band independence (1.10.980) doesn't cover AE, and compat omits it → warn.
        let mut s = scan("ng.dll", true, &[]);
        s.plugin.address_independence = 0x2;
        s.plugin.structure_independence = 0x2;
        let findings = F4se.run(&plugin_ctx(vec![s], Some(0x010B_0DD0)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
    }
}
