//! Plugin `TES4`/`HEDR` module versions: flag any that Fallout 4 doesn't accept

use crate::context::GameContext;
use crate::finding::Finding;
use overseer_core::plugins::PluginMeta;

/// True if `v` is exactly one of the two `HEDR` versions Fallout 4 accepts (0.95 or 1.00)
fn is_known_hedr(v: f32) -> bool {
    let bits = v.to_bits();
    bits == 0.95f32.to_bits() || bits == 1.0f32.to_bits()
}

/// Flags plugins whose header version isn't one Fallout 4 recognizes
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    ctx.loaded_plugins.iter().filter_map(warn_unknown).collect()
}

/// Warn when a plugin's `HEDR` version is present but not one Fallout 4 accepts
fn warn_unknown(plugin: &PluginMeta) -> Option<Finding> {
    let v = plugin.header_version?;
    if is_known_hedr(v) {
        return None;
    }
    Some(
        Finding::warning(format!(
            "`{}` has header version {v} (Fallout 4 uses 0.95 or 1.00)",
            plugin.name
        ))
        .detail(
            "Resave it in the Creation Kit to update the header, then confirm the result in xEdit.",
        ),
    )
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::Severity;

    fn plugin(name: &str, header_version: Option<f32>) -> PluginMeta {
        PluginMeta {
            header_version,
            ..overseer_core::test_support::plugin_meta(name, false, false, &[])
        }
    }

    fn run(plugins: Vec<PluginMeta>) -> Vec<Finding> {
        super::run(&GameContext {
            loaded_plugins: plugins,
            ..GameContext::default()
        })
    }

    #[test]
    fn is_known_hedr_accepts_only_the_two_fallout4_versions() {
        assert!(is_known_hedr(0.95));
        assert!(is_known_hedr(1.0));
        assert!(!is_known_hedr(0.94));
        assert!(!is_known_hedr(1.2));
        // Exact bits, no tolerance: a value close to 0.95 is still rejected
        assert!(!is_known_hedr(0.951));
        assert!(!is_known_hedr(0.949));
    }

    #[test]
    fn an_old_header_version_warns() {
        let findings = run(vec![plugin("Legacy.esp", Some(0.94))]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("Legacy.esp"));
        assert!(findings[0].title.contains("0.94"));
        assert!(
            findings[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("Creation Kit")
        );
    }

    #[test]
    fn the_accepted_versions_are_silent() {
        assert!(
            run(vec![
                plugin("Ok95.esp", Some(0.95)),
                plugin("Ok1.esp", Some(1.0))
            ])
            .is_empty()
        );
    }

    #[test]
    fn a_missing_header_version_is_skipped() {
        assert!(run(vec![plugin("NoHeader.esp", None)]).is_empty());
    }

    #[test]
    fn only_the_offenders_are_flagged_among_many() {
        let findings = run(vec![
            plugin("Ok.esp", Some(1.0)),
            plugin("Bad.esp", Some(0.85)),
            plugin("AlsoOk.esm", Some(0.95)),
            plugin("Unknown.esp", None),
        ]);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].title.contains("Bad.esp"));
    }
}
