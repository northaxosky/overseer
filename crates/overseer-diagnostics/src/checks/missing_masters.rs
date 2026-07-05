//! Active plugins whose masters aren't present; the game wont load

use crate::context::GameContext;
use crate::finding::Finding;
use overseer_core::plugins::PluginMeta;
use std::collections::BTreeSet;

/// Flag any active plugin that depends on a master which isn't present
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    // A master is satisfied only if its provider is actually loaded: the active; mod plugins plus the base/DLC/CC the engine force-loads (not merely on disk)
    let loaded: BTreeSet<String> = ctx
        .loaded_plugins
        .iter()
        .map(|p| p.name.to_lowercase())
        .collect();
    let mut findings: Vec<Finding> = ctx
        .active_plugins
        .iter()
        .filter_map(|plugin| missing_for(plugin, &loaded))
        .collect();

    if findings.is_empty() {
        findings.push(Finding::info("All plugin masters are present"));
    }
    findings
}

/// A finding for one plugin if any of its masters isn't present
fn missing_for(plugin: &PluginMeta, present: &BTreeSet<String>) -> Option<Finding> {
    let missing: Vec<String> = plugin
        .masters
        .iter()
        .filter(|m| !present.contains(&m.to_lowercase()))
        .map(|m| format!("`{m}`"))
        .collect();

    if missing.is_empty() {
        return None;
    }
    Some(
        Finding::error(format!(
            "`{}` is missing {}",
            plugin.name,
            missing.join(", ")
        ))
        .detail("Install or activate the master(s), or deactivate this plugin"),
    )
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::Severity;

    fn meta(name: &str, masters: &[&str]) -> PluginMeta {
        overseer_core::test_support::plugin_meta(name, false, false, masters)
    }

    fn ctx(active: Vec<PluginMeta>, present: &[&str]) -> GameContext {
        GameContext {
            active_plugins: active,
            loaded_plugins: present.iter().map(|p| meta(p, &[])).collect(),
            ..GameContext::default()
        }
    }

    #[test]
    fn present_masters_are_ok() {
        let c = ctx(
            vec![meta("Patch.esp", &["Fallout4.esm", "ArmorMod.esp"])],
            &["Fallout4.esm", "ArmorMod.esp", "Patch.esp"],
        );
        let findings = super::run(&c);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
    }

    #[test]
    fn a_missing_master_is_an_error() {
        let c = ctx(
            vec![meta("Patch.esp", &["Fallout4.esm", "Gone.esm"])],
            &["Fallout4.esm", "Patch.esp"],
        );
        let findings = super::run(&c);
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].title.contains("Patch.esp"));
        assert!(findings[0].title.contains("Gone.esm"));
        // Plugins are activated/deactivated, not enabled/disabled (glossary)
        let detail = findings[0].detail.as_deref().expect("detail");
        assert!(detail.contains("deactivate") && !detail.contains("disable"));
    }

    #[test]
    fn master_matching_is_case_insensitive() {
        let c = ctx(
            vec![meta("Patch.esp", &["FALLOUT4.ESM"])],
            &["fallout4.esm", "patch.esp"],
        );
        assert_eq!(super::run(&c)[0].severity, Severity::Info);
    }
}
